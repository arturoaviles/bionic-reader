//! Incremental terminal text transformation for bionic reading.
//!
//! [`Transformer`] buffers incomplete words and terminal controls across input
//! chunks. Use [`transform`] when the full input is already available.

use std::collections::BTreeMap;
use std::mem;
use std::ops::Range;
use std::str;

use unicode_segmentation::UnicodeSegmentation;

use crate::Error;
use crate::segmentation::{
    FixationEngine, UnicodeSegmenter, WordSegmenter, is_grapheme_extension, is_letter_grapheme,
    is_word_connector,
};
use crate::settings::{Emphasis, Settings};

const MAX_PENDING: usize = 64 * 1024;

#[cfg(test)]
thread_local! {
    static TOKEN_SCAN_BYTES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ControlKind {
    Sgr,
    Metadata,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum C1ControlKind {
    Csi,
    Osc,
    String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TokenKind {
    Text(char),
    Raw,
    Control(ControlKind),
}

#[derive(Clone, Copy, Debug)]
struct Token {
    start: usize,
    end: usize,
    kind: TokenKind,
}

#[derive(Clone, Copy, Debug)]
struct ParseResult {
    complete_len: usize,
    incomplete_at: Option<usize>,
}

#[derive(Clone, Copy, Debug, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "terminal attributes track independent SGR state and style certainty"
)]
struct StyleState {
    bold: bool,
    dim: bool,
    underline: UnderlineStyle,
    reverse: bool,
    known: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum UnderlineStyle {
    #[default]
    Off,
    Single,
    Double,
    DoubleSgr21,
    Curly,
    Dotted,
    Dashed,
    Other(u16),
}

impl UnderlineStyle {
    fn from_colon_parameter(parameter: Option<u16>) -> Option<Self> {
        match parameter {
            Some(0) => Some(Self::Off),
            Some(1) => Some(Self::Single),
            Some(2) => Some(Self::Double),
            Some(3) => Some(Self::Curly),
            Some(4) => Some(Self::Dotted),
            Some(5) => Some(Self::Dashed),
            Some(other) => Some(Self::Other(other)),
            None => None,
        }
    }

    const fn is_active(self) -> bool {
        !matches!(self, Self::Off)
    }

    fn write_start(self, output: &mut Vec<u8>) {
        match self {
            Self::Off => {}
            Self::Single => output.extend_from_slice(b"\x1b[4m"),
            Self::DoubleSgr21 => output.extend_from_slice(b"\x1b[21m"),
            Self::Double => output.extend_from_slice(b"\x1b[4:2m"),
            Self::Curly => output.extend_from_slice(b"\x1b[4:3m"),
            Self::Dotted => output.extend_from_slice(b"\x1b[4:4m"),
            Self::Dashed => output.extend_from_slice(b"\x1b[4:5m"),
            Self::Other(parameter) => {
                output.extend_from_slice(format!("\x1b[4:{parameter}m").as_bytes());
            }
        }
    }
}

impl StyleState {
    #[must_use]
    fn initial() -> Self {
        Self {
            known: true,
            ..Self::default()
        }
    }

    fn apply_sgr(&mut self, bytes: &[u8]) {
        let Some(params) = sgr_params(bytes) else {
            self.known = false;
            return;
        };
        let mut index = 0;
        while index < params.len() {
            let param = params[index];
            if matches!(param.code, 38 | 48 | 58) {
                if param.colon_form {
                    index += 1;
                    continue;
                }
                match params.get(index + 1).map(|value| value.code) {
                    Some(5) => index += 3,
                    Some(2) => index += 5,
                    _ => {
                        self.known = false;
                        return;
                    }
                }
                continue;
            }
            match param.code {
                0 => {
                    self.bold = false;
                    self.dim = false;
                    self.underline = UnderlineStyle::Off;
                    self.reverse = false;
                    self.known = true;
                }
                1 if self.known => self.bold = true,
                2 if self.known => self.dim = true,
                4 if self.known && param.colon_form => {
                    if let Some(underline) = UnderlineStyle::from_colon_parameter(param.colon_value)
                    {
                        self.underline = underline;
                    } else {
                        self.known = false;
                    }
                }
                4 if self.known => self.underline = UnderlineStyle::Single,
                21 if self.known => self.underline = UnderlineStyle::DoubleSgr21,
                7 if self.known => self.reverse = true,
                22 => {
                    self.bold = false;
                    self.dim = false;
                }
                24 => self.underline = UnderlineStyle::Off,
                27 => self.reverse = false,
                _ => {}
            }
            index += 1;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GraphemeKind {
    Letter,
    Apostrophe,
    Connector,
    Number,
    Raw,
    Other,
}

#[derive(Clone, Debug)]
struct Grapheme {
    start_token: usize,
    end_token: usize,
    kind: GraphemeKind,
    extends_previous_word: bool,
    contains_extension: bool,
}

#[derive(Clone, Copy, Debug)]
struct CharMap {
    start: usize,
    end: usize,
    token: usize,
}

#[derive(Clone, Copy, Debug)]
struct SgrParam {
    code: u16,
    colon_form: bool,
    colon_value: Option<u16>,
}

#[derive(Clone, Copy, Debug)]
enum DrainMode {
    EscapeIntro,
    Csi,
    Osc {
        previous_escape: bool,
        utf8: Utf8ContinuationState,
    },
    String {
        previous_escape: bool,
        utf8: Utf8ContinuationState,
    },
    Escape {
        intermediate: bool,
    },
}

#[derive(Clone, Copy, Debug, Default)]
struct Utf8ContinuationState {
    remaining: u8,
    next_min: u8,
    next_max: u8,
    c1_prefix: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Utf8ByteState {
    Other,
    Continuation,
    EncodedC1St,
}

impl Utf8ContinuationState {
    /// Classifies the byte and advances UTF-8 state for a terminal string payload.
    fn advance(&mut self, byte: u8) -> Utf8ByteState {
        if self.remaining > 0 && (self.next_min..=self.next_max).contains(&byte) {
            let encoded_c1_st = self.c1_prefix && self.remaining == 1 && byte == 0x9c;
            self.remaining -= 1;
            self.next_min = 0x80;
            self.next_max = 0xbf;
            if self.remaining == 0 {
                self.c1_prefix = false;
            }
            return if encoded_c1_st {
                Utf8ByteState::EncodedC1St
            } else {
                Utf8ByteState::Continuation
            };
        }

        self.remaining = 0;
        self.c1_prefix = false;
        let (remaining, next_min, next_max) = match byte {
            0xc2..=0xdf => (1, 0x80, 0xbf),
            0xe0 => (2, 0xa0, 0xbf),
            0xe1..=0xec | 0xee..=0xef => (2, 0x80, 0xbf),
            0xed => (2, 0x80, 0x9f),
            0xf0 => (3, 0x90, 0xbf),
            0xf1..=0xf3 => (3, 0x80, 0xbf),
            0xf4 => (3, 0x80, 0x8f),
            _ => return Utf8ByteState::Other,
        };
        self.remaining = remaining;
        self.next_min = next_min;
        self.next_max = next_max;
        self.c1_prefix = byte == 0xc2;
        Utf8ByteState::Other
    }

    fn consume_valid_continuation_prefix(&mut self, bytes: &[u8]) -> usize {
        let mut consumed = 0;
        for byte in bytes.iter().copied() {
            if self.remaining == 0 || !(self.next_min..=self.next_max).contains(&byte) {
                *self = Self::default();
                break;
            }
            self.advance(byte);
            consumed += 1;
        }
        consumed
    }
}

#[derive(Debug)]
struct DrainingControl {
    bytes: Vec<u8>,
    mode: DrainMode,
    tracking: bool,
}

#[derive(Clone, Copy, Debug)]
struct PendingControl {
    mode: DrainMode,
}

impl PendingControl {
    fn new(bytes: &[u8]) -> Option<Self> {
        Some(Self {
            mode: control_mode(bytes)?,
        })
    }

    fn consume(&mut self, bytes: &[u8]) -> bool {
        bytes
            .iter()
            .copied()
            .any(|byte| drain_mode_complete(&mut self.mode, byte))
    }
}

/// Transforms terminal output while preserving stream boundaries and controls.
///
/// Original text bytes and terminal controls retain their order; only emphasis
/// escape sequences are inserted. Disabled transformation returns the original bytes.
///
/// A transformer retains incomplete words and terminal sequences between calls
/// to [`push`](Self::push). Call [`finish`](Self::finish) when the stream ends so
/// any remaining complete word can be emitted. Settings supplied to [`new`](Self::new)
/// are validated at construction. Use [`try_new`](Self::try_new) for fallible construction.
#[derive(Debug)]
#[must_use = "use the transformer to process terminal output"]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent stream states can coexist, including a partial word and an uncertain terminal style"
)]
pub struct Transformer {
    fixation: FixationEngine,
    segmenter: Box<dyn WordSegmenter>,
    emphasis: Emphasis,
    enabled: bool,
    pending: Vec<u8>,
    pending_word: bool,
    style: StyleState,
    passthrough_word: bool,
    passthrough_context: String,
    passthrough_collapsed_letters: usize,
    passthrough_apostrophe: bool,
    pending_control: Option<PendingControl>,
    draining_control: Option<DrainingControl>,
    utf8_continuation: Utf8ContinuationState,
    pending_c1_shielded: usize,
}

impl Transformer {
    /// Creates a transformer with a snapshot of the supplied emphasis settings.
    ///
    /// `enabled` controls whether this transformer adds bionic reading emphasis.
    /// Later configuration changes do not affect this stream. Session exclusions
    /// and the style policy are resolved by the caller and are not retained here.
    ///
    /// # Panics
    ///
    /// Panics immediately if settings are invalid. Use [`Self::try_new`] for untrusted settings.
    pub fn new(settings: &Settings, enabled: bool) -> Self {
        Self::try_new(settings, enabled).expect("transformer settings must be valid")
    }

    /// Creates a transformer that uses a caller-supplied word segmenter.
    ///
    /// The segmenter must return sorted, non-overlapping UTF-8 and grapheme-aligned
    /// ranges, as documented by [`WordSegmenter`].
    ///
    /// # Panics
    ///
    /// Panics immediately if settings are invalid. Use
    /// [`Self::try_new_with_segmenter`] for untrusted settings.
    pub fn new_with_segmenter(
        settings: &Settings,
        enabled: bool,
        segmenter: Box<dyn WordSegmenter>,
    ) -> Self {
        Self::try_new_with_segmenter(settings, enabled, segmenter)
            .expect("transformer settings must be valid")
    }

    /// Validates settings and creates a transformer with its own emphasis snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error when [`Settings::validate`] rejects the supplied settings.
    pub fn try_new(settings: &Settings, enabled: bool) -> Result<Self, Error> {
        Self::try_new_with_segmenter(settings, enabled, Box::new(UnicodeSegmenter::new()))
    }

    /// Validates settings and creates a transformer with a caller-supplied segmenter.
    ///
    /// # Errors
    ///
    /// Returns an error when [`Settings::validate`] rejects the supplied settings.
    pub fn try_new_with_segmenter(
        settings: &Settings,
        enabled: bool,
        segmenter: Box<dyn WordSegmenter>,
    ) -> Result<Self, Error> {
        settings.validate()?;
        Ok(Self {
            fixation: FixationEngine::new(settings),
            segmenter,
            emphasis: settings.emphasis,
            enabled,
            pending: Vec::new(),
            pending_word: false,
            style: StyleState::initial(),
            passthrough_word: false,
            passthrough_context: String::new(),
            passthrough_collapsed_letters: 0,
            passthrough_apostrophe: false,
            pending_control: None,
            draining_control: None,
            utf8_continuation: Utf8ContinuationState::default(),
            pending_c1_shielded: 0,
        })
    }

    /// Processes an input chunk and returns bytes that are ready to emit.
    ///
    /// Incomplete words and terminal sequences remain buffered until more input
    /// arrives or the stream is flushed or finished.
    ///
    #[must_use = "write or buffer the processed output"]
    pub fn push(&mut self, input: &[u8]) -> Vec<u8> {
        if !self.enabled {
            return input.to_vec();
        }

        let mut output = Vec::new();
        let mut remaining = input;
        while !remaining.is_empty() {
            if self.draining_control.is_some() {
                let previous_len = remaining.len();
                let (bytes, rest) = self.drain_control(remaining);
                output.extend_from_slice(bytes);
                remaining = rest;
                if remaining.len() == previous_len {
                    break;
                }
                if remaining.is_empty() {
                    break;
                }
            }

            if self.passthrough_word
                && self
                    .pending
                    .len()
                    .saturating_add(self.passthrough_context.len())
                    >= MAX_PENDING
            {
                self.collapse_passthrough_context();
            }

            let available = MAX_PENDING
                .saturating_sub(self.pending.len())
                .saturating_sub(self.passthrough_context.len());
            if available == 0 {
                if self.passthrough_word {
                    output.extend(self.release_passthrough_pending());
                } else {
                    output.extend(self.fallback_pending());
                }
                continue;
            }

            let count = remaining.len().min(available);
            let appended = &remaining[..count];
            let pending_before = self.pending.len();
            self.pending_c1_shielded = self.pending_c1_shielded.saturating_add(
                self.utf8_continuation
                    .consume_valid_continuation_prefix(appended),
            );
            self.pending.extend_from_slice(appended);
            remaining = &remaining[count..];

            if let Some(mut control) = self.pending_control.take() {
                if !control.consume(appended) {
                    self.pending_control = Some(control);
                    if self.pending.len() >= MAX_PENDING {
                        if self.passthrough_word {
                            output.extend(self.release_passthrough_pending());
                        } else {
                            output.extend(self.fallback_pending());
                        }
                    }
                    continue;
                }
            }

            if self.passthrough_word {
                let (raw, resume) = self.passthrough_until_delimiter();
                output.extend(raw);
                if resume {
                    output.extend(self.process_available());
                }
                if self.pending.len() >= MAX_PENDING {
                    output.extend(self.release_passthrough_pending());
                }
                continue;
            }

            if !self.pending_word
                || appended.contains(&0x1b)
                || !chunk_can_extend_word_after_pending(
                    &self.pending[..pending_before],
                    appended,
                    self.pending_word,
                )
            {
                output.extend(self.process_available());
            }
            if self.pending.len() >= MAX_PENDING {
                output.extend(self.fallback_pending());
            }
        }
        output
    }

    /// Releases a stalled partial word or control sequence without transforming it.
    ///
    /// Returns the released bytes. The remainder of an open word stays in
    /// passthrough until its delimiter arrives.
    #[must_use = "write or buffer the released bytes"]
    pub fn flush_pending(&mut self) -> Vec<u8> {
        if self.pending.is_empty() {
            return Vec::new();
        }
        if self.passthrough_word {
            return self.release_passthrough_pending();
        }
        self.fallback_pending()
    }

    /// Whether a partial word or terminal sequence is waiting for more input.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Finishes the stream. A final complete word can be transformed; malformed or
    /// incomplete bytes are emitted unchanged.
    ///
    #[must_use = "write or buffer the remaining output"]
    pub fn finish(&mut self) -> Vec<u8> {
        if !self.enabled {
            self.pending.clear();
            self.pending_word = false;
            self.pending_control = None;
            return Vec::new();
        }

        let mut output = Vec::new();
        if self.draining_control.is_some() {
            // The partial escape was already written by flush_pending. There are no
            // buffered bytes to recover, and its terminal state is uncertain.
            self.draining_control = None;
            self.style.known = false;
        }
        self.pending_control = None;

        if self.passthrough_word {
            let (tokens, _) = scan_tokens_with_c1_shield(
                &self.pending,
                /* eof */ false,
                self.pending_c1_shielded,
            );
            let (visible, _) = visible_text(&tokens);
            self.advance_passthrough_text(&visible, true);
            output.append(&mut self.pending);
            self.passthrough_word = false;
            self.pending_word = false;
            self.passthrough_context.clear();
            self.passthrough_collapsed_letters = 0;
            self.pending_c1_shielded = 0;
            return output;
        }

        if !self.pending.is_empty() {
            let pending = mem::take(&mut self.pending);
            let (tokens, _) =
                scan_tokens_with_c1_shield(&pending, /* eof */ true, self.pending_c1_shielded);
            output.extend(self.render(&pending, &tokens));
        }
        self.pending_c1_shielded = 0;
        self.pending_word = false;
        output
    }

    #[must_use]
    fn process_available(&mut self) -> Vec<u8> {
        if self.pending.is_empty() {
            self.pending_control = None;
            return Vec::new();
        }

        let (tokens, parse) = scan_tokens_with_c1_shield(
            &self.pending,
            /* eof */ false,
            self.pending_c1_shielded,
        );
        let graphemes = graphemes(&tokens);
        let candidate = trailing_candidate_start(&graphemes)
            .into_iter()
            .chain(trailing_segmenter_candidate(
                &tokens,
                self.segmenter.as_ref(),
            ))
            .min();
        self.pending_word = candidate.is_some();
        let safe_token_count = candidate.unwrap_or(tokens.len());
        let cut = if let Some(token) = candidate {
            tokens[token].start
        } else {
            parse.complete_len
        };

        if cut == 0 {
            self.pending_control = parse
                .incomplete_at
                .and_then(|start| self.pending_control_at(start));
            return Vec::new();
        }
        let source = self.pending[..cut].to_vec();
        let output = self.render(&source, &tokens[..safe_token_count]);
        self.pending.drain(..cut);
        self.pending_c1_shielded = self.pending_c1_shielded.saturating_sub(cut);
        self.pending_control = parse
            .incomplete_at
            .filter(|start| *start >= cut)
            .and_then(|start| self.pending_control_at(start - cut));
        output
    }

    fn pending_control_at(&self, start: usize) -> Option<PendingControl> {
        let tail = self.pending.get(start..)?;
        PendingControl::new(tail)
    }

    #[must_use]
    fn fallback_pending(&mut self) -> Vec<u8> {
        if self.pending.is_empty() {
            return Vec::new();
        }

        self.pending_control = None;
        let c1_shielded = mem::take(&mut self.pending_c1_shielded);
        let source = mem::take(&mut self.pending);
        let (tokens, parse) =
            scan_tokens_with_c1_shield(&source, /* eof */ false, c1_shielded);
        self.update_style_from_tokens(&source, &tokens);
        let graphemes = graphemes(&tokens);
        let (visible, _) = visible_text(&tokens);
        let candidate = trailing_candidate_start(&graphemes)
            .into_iter()
            .chain(trailing_segmenter_candidate(
                &tokens,
                self.segmenter.as_ref(),
            ))
            .min();
        if candidate.is_some() {
            self.passthrough_word = true;
            self.passthrough_context.clear();
            self.passthrough_collapsed_letters = 0;
            self.advance_passthrough_text(&visible, false);
            self.passthrough_apostrophe = graphemes
                .last()
                .is_some_and(|unit| unit.kind == GraphemeKind::Apostrophe);
        } else {
            self.passthrough_word = false;
            self.passthrough_context.clear();
            self.passthrough_collapsed_letters = 0;
            self.passthrough_apostrophe = false;
            self.advance_passthrough_text(&visible, true);
        }
        self.pending_word = false;

        if let Some(start) = parse.incomplete_at {
            let tail = &source[start..];
            if control_mode(tail).is_some() {
                let drain = DrainingControl::new(tail);
                if !drain.tracking {
                    self.style.known = false;
                }
                self.draining_control = Some(drain);
            } else if let Some(state) = utf8_state_for_incomplete_suffix(tail) {
                self.utf8_continuation = state;
            }
        }

        source
    }

    #[must_use]
    fn release_passthrough_pending(&mut self) -> Vec<u8> {
        if self.pending.is_empty() {
            return Vec::new();
        }

        self.pending_control = None;
        let c1_shielded = mem::take(&mut self.pending_c1_shielded);
        let source = mem::take(&mut self.pending);
        let (tokens, parse) =
            scan_tokens_with_c1_shield(&source, /* eof */ false, c1_shielded);
        self.update_style_from_tokens(&source[..parse.complete_len], &tokens);
        let mut output = source[..parse.complete_len].to_vec();
        if let Some(start) = parse.incomplete_at {
            let tail = &source[start..];
            if control_mode(tail).is_some() {
                let drain = DrainingControl::new(tail);
                if !drain.tracking {
                    self.style.known = false;
                }
                self.draining_control = Some(drain);
            } else if let Some(state) = utf8_state_for_incomplete_suffix(tail) {
                self.utf8_continuation = state;
            }
            output.extend_from_slice(tail);
        }
        output
    }

    fn advance_passthrough_text(&mut self, appended: &str, final_boundary: bool) {
        self.track_passthrough_text(appended, final_boundary, false);
    }

    fn track_passthrough_text(&mut self, appended: &str, final_boundary: bool, force: bool) {
        if self.passthrough_collapsed_letters > 0 {
            self.passthrough_collapsed_letters = self
                .passthrough_collapsed_letters
                .saturating_add(letter_grapheme_count(appended));
            if final_boundary {
                self.fixation
                    .advance_if_eligible(self.passthrough_collapsed_letters);
                self.passthrough_collapsed_letters = 0;
                self.passthrough_context.clear();
            }
            return;
        }

        let mut text = mem::take(&mut self.passthrough_context);
        text.push_str(appended);
        // Selection cannot resume until this run closes. Accumulate a bounded
        // suffix instead of resegmenting the growing suffix on every tiny read.
        if !force && !final_boundary && text.len() < MAX_PENDING {
            self.passthrough_context = text;
            return;
        }
        let ranges = self.segmenter.word_ranges(&text);
        let trailing_start = if final_boundary {
            None
        } else {
            segmenter_trailing_start(self.segmenter.as_ref(), &text)
                .or_else(|| trailing_grapheme_candidate_start(&text))
        };
        let end_before = trailing_start.unwrap_or(text.len());
        self.fixation
            .advance_ranges_before(&text, &ranges, end_before);

        if final_boundary {
            self.passthrough_context.clear();
        } else if let Some(start) = trailing_start {
            self.passthrough_context.push_str(&text[start..]);
        }
    }

    fn collapse_passthrough_context(&mut self) {
        if self.passthrough_context.is_empty() {
            return;
        }
        // Resolve completed ICU/custom ranges before treating the remaining
        // open word as oversized. A partial UTF-8 scalar can occupy pending bytes.
        self.track_passthrough_text("", false, true);
        if self
            .passthrough_context
            .len()
            .saturating_add(self.pending.len())
            < MAX_PENDING
        {
            return;
        }
        self.passthrough_collapsed_letters = self
            .passthrough_collapsed_letters
            .saturating_add(letter_grapheme_count(&self.passthrough_context));
        self.passthrough_context.clear();
    }

    #[must_use]
    fn passthrough_until_delimiter(&mut self) -> (Vec<u8>, bool) {
        let (tokens, parse) = scan_tokens_with_c1_shield(
            &self.pending,
            /* eof */ false,
            self.pending_c1_shielded,
        );
        let units = graphemes(&tokens);
        let delimiter_index = units.iter().position(|unit| match unit.kind {
            GraphemeKind::Letter | GraphemeKind::Number | GraphemeKind::Connector => {
                self.passthrough_apostrophe = false;
                false
            }
            GraphemeKind::Apostrophe if !self.passthrough_apostrophe => {
                self.passthrough_apostrophe = true;
                false
            }
            GraphemeKind::Other if unit.extends_previous_word => false,
            _ => true,
        });

        if let Some(delimiter_index) = delimiter_index {
            let delimiter = &units[delimiter_index];
            let cut = tokens[delimiter.end_token - 1].end;
            let (visible_prefix, _) = visible_text(&tokens[..delimiter.start_token]);
            self.advance_passthrough_text(&visible_prefix, true);
            self.passthrough_apostrophe = false;
            let source = self.pending[..cut].to_vec();
            self.update_style_from_tokens(&source, &tokens[..delimiter.end_token]);
            let raw: Vec<u8> = self.pending.drain(..cut).collect();
            self.pending_c1_shielded = self.pending_c1_shielded.saturating_sub(cut);
            self.passthrough_word = false;
            return (raw, true);
        }

        let cut = parse.complete_len;
        let source = self.pending[..cut].to_vec();
        let (visible, _) = visible_text(&tokens);
        self.advance_passthrough_text(&visible, false);
        self.update_style_from_tokens(&source, &tokens);
        let raw: Vec<u8> = self.pending.drain(..cut).collect();
        self.pending_c1_shielded = self.pending_c1_shielded.saturating_sub(cut);
        self.pending_control = parse
            .incomplete_at
            .filter(|start| *start >= cut)
            .and_then(|start| self.pending_control_at(start - cut));
        (raw, false)
    }

    #[must_use]
    fn drain_control<'a>(&mut self, input: &'a [u8]) -> (&'a [u8], &'a [u8]) {
        let Some(mut drain) = self.draining_control.take() else {
            return (&input[..0], input);
        };

        let indexed_bytes = input.iter().copied().enumerate();
        for (index, byte) in indexed_bytes {
            if drain.bytes.len() < MAX_PENDING {
                drain.bytes.push(byte);
            }
            if drain_mode_complete(&mut drain.mode, byte) {
                if drain.tracking {
                    let (tokens, parse) = scan_tokens(&drain.bytes, /* eof */ true);
                    if parse.complete_len > 0 {
                        self.update_style_from_tokens(&drain.bytes, &tokens);
                    }
                } else {
                    self.style.known = false;
                }
                return (&input[..=index], &input[index + 1..]);
            }
            if drain.bytes.len() >= MAX_PENDING {
                drain.bytes.clear();
                drain.tracking = false;
                self.style.known = false;
            }
        }
        self.draining_control = Some(drain);
        (input, &input[input.len()..])
    }

    #[must_use]
    fn render(&mut self, source: &[u8], tokens: &[Token]) -> Vec<u8> {
        let starts = self.select_emphasis(tokens);
        render_ansi(source, tokens, &starts, &mut self.style, self.emphasis)
    }

    fn select_emphasis(&mut self, tokens: &[Token]) -> BTreeMap<usize, usize> {
        select_emphasis(tokens, &mut self.fixation, self.segmenter.as_ref())
    }

    fn update_style_from_tokens(&mut self, source: &[u8], tokens: &[Token]) {
        for token in tokens {
            if let TokenKind::Control(ControlKind::Sgr) = token.kind {
                self.style.apply_sgr(&source[token.start..token.end]);
            } else if matches!(token.kind, TokenKind::Control(ControlKind::Other)) {
                self.style.known = false;
            }
        }
    }
}

#[must_use]
fn render_ansi(
    source: &[u8],
    tokens: &[Token],
    starts: &BTreeMap<usize, usize>,
    style: &mut StyleState,
    emphasis: Emphasis,
) -> Vec<u8> {
    // Reserve for actual emphasized spans, rather than every character token.
    // SGR resets inside a span may still grow the buffer as required.
    let mut output =
        Vec::with_capacity(source.len().saturating_add(starts.len().saturating_mul(17)));
    let mut emphasis_end = None;
    let mut emphasizing = false;
    let indexed_tokens = tokens.iter().enumerate();
    for (index, token) in indexed_tokens {
        if let Some(event) = starts
            .get(&index)
            .filter(|_| style.known && !emphasis_is_active(*style, emphasis))
        {
            write_emphasis_start(&mut output, emphasis);
            emphasizing = true;
            emphasis_end = Some(*event);
        }

        output.extend_from_slice(&source[token.start..token.end]);
        if let TokenKind::Control(ControlKind::Sgr) = token.kind {
            let resets_emphasis = sgr_resets_emphasis(&source[token.start..token.end], emphasis);
            style.apply_sgr(&source[token.start..token.end]);
            if emphasizing && resets_emphasis {
                write_emphasis_start(&mut output, emphasis);
            }
        } else if matches!(token.kind, TokenKind::Control(ControlKind::Other)) {
            style.known = false;
        }

        if emphasizing && emphasis_end == Some(index + 1) {
            restore_emphasis(&mut output, *style, emphasis);
            emphasizing = false;
            emphasis_end = None;
        }
    }
    output
}

#[must_use]
fn emphasis_is_active(style: StyleState, emphasis: Emphasis) -> bool {
    match emphasis {
        Emphasis::Bold => style.bold,
        Emphasis::Underline => style.underline.is_active(),
        Emphasis::Reverse => style.reverse,
    }
}

fn write_emphasis_start(output: &mut Vec<u8>, emphasis: Emphasis) {
    let sequence: &[u8] = match emphasis {
        Emphasis::Bold => b"\x1b[1m",
        Emphasis::Underline => b"\x1b[4m",
        Emphasis::Reverse => b"\x1b[7m",
    };
    output.extend_from_slice(sequence);
}

fn restore_emphasis(output: &mut Vec<u8>, style: StyleState, emphasis: Emphasis) {
    match emphasis {
        Emphasis::Bold => restore_bold_dim(output, style),
        Emphasis::Underline => {
            output.extend_from_slice(b"\x1b[24m");
            style.underline.write_start(output);
        }
        Emphasis::Reverse => {
            output.extend_from_slice(b"\x1b[27m");
            if style.reverse {
                output.extend_from_slice(b"\x1b[7m");
            }
        }
    }
}

/// Transforms a complete byte slice using bionic reading emphasis.
///
/// Original text bytes and terminal controls are preserved in order, with only
/// emphasis escape sequences inserted.
/// When `enabled` is false, the input is returned unchanged.
///
/// # Panics
///
/// Panics immediately if settings are invalid. For untrusted settings, construct
/// a [`Transformer`] with [`Transformer::try_new`]. Malformed UTF-8 and incomplete
/// terminal sequences do not cause a panic; those bytes are preserved.
///
/// # Examples
///
/// ```
/// use bionic_reader::settings::Settings;
/// use bionic_reader::transformer;
///
/// let output = transformer::transform(b"reading", &Settings::default(), true);
/// assert_eq!(output, b"\x1b[1mread\x1b[22ming");
/// ```
#[must_use = "use the transformed bytes"]
pub fn transform(input: &[u8], settings: &Settings, enabled: bool) -> Vec<u8> {
    let mut transformer = Transformer::new(settings, enabled);
    let mut output = transformer.push(input);
    output.extend(transformer.finish());
    output
}

/// Transforms a complete byte slice using a caller-supplied word segmenter.
///
/// Unlike [`Transformer`], this convenience function does not retain any state
/// between calls. Original bytes and terminal controls are preserved in order,
/// with only emphasis escape sequences inserted. A disabled transformer returns
/// the original bytes unchanged. Use [`Transformer::new_with_segmenter`] for
/// incremental streams with bounded pending input.
///
/// # Panics
///
/// Panics immediately if settings are invalid or the segmenter returns ranges
/// that violate [`WordSegmenter`]'s range contract. Use [`Settings::validate`]
/// for fallible validation of untrusted settings.
#[must_use = "use the transformed bytes"]
pub fn transform_with_segmenter(
    input: &[u8],
    settings: &Settings,
    enabled: bool,
    segmenter: &dyn WordSegmenter,
) -> Vec<u8> {
    settings
        .validate()
        .expect("transformer settings must be valid");
    if !enabled {
        return input.to_vec();
    }

    let (tokens, _) = scan_tokens(input, /* eof */ true);
    let mut fixation = FixationEngine::new(settings);
    let starts = select_emphasis(&tokens, &mut fixation, segmenter);
    let mut style = StyleState::initial();
    render_ansi(input, &tokens, &starts, &mut style, settings.emphasis)
}

fn select_emphasis(
    tokens: &[Token],
    fixation: &mut FixationEngine,
    segmenter: &dyn WordSegmenter,
) -> BTreeMap<usize, usize> {
    let (visible, char_map) = visible_text(tokens);
    let spans = fixation.spans(&visible, segmenter);
    let mut starts = BTreeMap::new();

    for span in spans {
        let Some(token_range) = token_range_for_span(&span, &char_map) else {
            continue;
        };
        if token_range.end <= token_range.start
            || tokens[token_range.clone()]
                .iter()
                .any(|token| matches!(token.kind, TokenKind::Control(ControlKind::Other)))
        {
            continue;
        }
        let raw_start = tokens[token_range.start].start;
        let raw_end = tokens[token_range.end - 1].end;
        if raw_end.saturating_sub(raw_start) <= MAX_PENDING {
            starts.insert(token_range.start, token_range.end);
        }
    }

    starts
}

fn restore_bold_dim(output: &mut Vec<u8>, style: StyleState) {
    output.extend_from_slice(b"\x1b[22m");
    if style.bold {
        output.extend_from_slice(b"\x1b[1m");
    }
    if style.dim {
        output.extend_from_slice(b"\x1b[2m");
    }
}

#[must_use]
fn scan_tokens(source: &[u8], eof: bool) -> (Vec<Token>, ParseResult) {
    scan_tokens_with_c1_shield(source, eof, 0)
}

#[must_use]
fn scan_tokens_with_c1_shield(
    source: &[u8],
    eof: bool,
    c1_shielded_prefix: usize,
) -> (Vec<Token>, ParseResult) {
    #[cfg(test)]
    TOKEN_SCAN_BYTES.with(|counter| {
        counter.set(counter.get().saturating_add(source.len()));
    });

    let mut tokens = Vec::new();
    let mut offset = 0;
    while offset < source.len() {
        let control = if source[offset] == 0x1b {
            Some(parse_escape(source, offset))
        } else if offset < c1_shielded_prefix {
            None
        } else {
            c1_control_intro(source, offset)
                .map(|(header_len, kind)| parse_c1_control(source, offset, header_len, kind))
        };
        if let Some(control) = control {
            match control {
                Some((end, kind)) => {
                    tokens.push(Token {
                        start: offset,
                        end,
                        kind: TokenKind::Control(kind),
                    });
                    offset = end;
                }
                None if eof => {
                    tokens.push(Token {
                        start: offset,
                        end: source.len(),
                        kind: TokenKind::Control(ControlKind::Other),
                    });
                    offset = source.len();
                }
                None => {
                    return (
                        tokens,
                        ParseResult {
                            complete_len: offset,
                            incomplete_at: Some(offset),
                        },
                    );
                }
            }
            continue;
        }

        let width = utf8_width(source[offset]);
        if width == 0 {
            tokens.push(Token {
                start: offset,
                end: offset + 1,
                kind: TokenKind::Raw,
            });
            offset += 1;
            continue;
        }
        if offset + width > source.len() {
            if eof {
                tokens.push(Token {
                    start: offset,
                    end: source.len(),
                    kind: TokenKind::Raw,
                });
                offset = source.len();
                continue;
            }
            return (
                tokens,
                ParseResult {
                    complete_len: offset,
                    incomplete_at: Some(offset),
                },
            );
        }

        if let Ok(text) = str::from_utf8(&source[offset..offset + width]) {
            let character = text.chars().next().expect("one valid UTF-8 character");
            tokens.push(Token {
                start: offset,
                end: offset + width,
                kind: TokenKind::Text(character),
            });
            offset += width;
        } else {
            tokens.push(Token {
                start: offset,
                end: offset + 1,
                kind: TokenKind::Raw,
            });
            offset += 1;
        }
    }

    (
        tokens,
        ParseResult {
            complete_len: source.len(),
            incomplete_at: None,
        },
    )
}

#[must_use]
fn parse_escape(source: &[u8], start: usize) -> Option<(usize, ControlKind)> {
    let next = *source.get(start + 1)?;
    match next {
        b'[' => parse_csi(source, start, start + 2),
        b']' => parse_osc(source, start + 2),
        b'P' | b'^' | b'_' | b'X' => parse_string_control(source, start + 2),
        _ => {
            if !(0x20..=0x2f).contains(&next) {
                return Some((start + 2, ControlKind::Other));
            }

            let mut index = start + 2;
            while let Some(byte) = source.get(index).copied() {
                if (0x20..=0x2f).contains(&byte) {
                    index += 1;
                } else if (0x30..=0x7e).contains(&byte) {
                    let end = index + 1;
                    let kind = if is_presentation_neutral_control(&source[start..end]) {
                        ControlKind::Metadata
                    } else {
                        ControlKind::Other
                    };
                    return Some((end, kind));
                } else {
                    return Some((index, ControlKind::Other));
                }
            }
            None
        }
    }
}

fn c1_control_intro(source: &[u8], start: usize) -> Option<(usize, C1ControlKind)> {
    let first = *source.get(start)?;
    let (header_len, byte) = if first == 0xc2
        && source
            .get(start + 1)
            .is_some_and(|byte| matches!(*byte, 0x90 | 0x98 | 0x9b | 0x9d | 0x9e | 0x9f))
    {
        (2, source[start + 1])
    } else {
        (1, first)
    };
    let kind = match byte {
        0x9b => C1ControlKind::Csi,
        0x9d => C1ControlKind::Osc,
        0x90 | 0x98 | 0x9e | 0x9f => C1ControlKind::String,
        _ => return None,
    };
    Some((header_len, kind))
}

fn parse_c1_control(
    source: &[u8],
    start: usize,
    header_len: usize,
    kind: C1ControlKind,
) -> Option<(usize, ControlKind)> {
    let payload_start = start + header_len;
    match kind {
        C1ControlKind::Csi => parse_csi(source, start, payload_start),
        C1ControlKind::Osc => parse_osc(source, payload_start),
        C1ControlKind::String => parse_string_control(source, payload_start),
    }
}

fn parse_csi(source: &[u8], start: usize, parameter_start: usize) -> Option<(usize, ControlKind)> {
    let mut index = parameter_start;
    while let Some(byte) = source.get(index).copied() {
        if (0x40..=0x7e).contains(&byte) {
            let end = index + 1;
            let sequence = &source[start..end];
            let kind =
                if end - start <= MAX_PENDING && byte == b'm' && sgr_params(sequence).is_some() {
                    ControlKind::Sgr
                } else if is_presentation_neutral_control(sequence) {
                    ControlKind::Metadata
                } else {
                    ControlKind::Other
                };
            return Some((end, kind));
        }
        index += 1;
    }
    None
}

fn parse_osc(source: &[u8], payload_start: usize) -> Option<(usize, ControlKind)> {
    let mut index = payload_start;
    let mut terminator = None;
    let mut utf8 = Utf8ContinuationState::default();
    while index < source.len() {
        let byte = source[index];
        let utf8_state = utf8.advance(byte);
        if byte == 0x07 || byte == 0x9c && utf8_state == Utf8ByteState::Other {
            terminator = Some((index + 1, index));
            break;
        }
        if byte == 0x9c && utf8_state == Utf8ByteState::EncodedC1St {
            terminator = Some((index + 1, index - 1));
            break;
        }
        if byte == 0x1b && source.get(index + 1) == Some(&b'\\') {
            terminator = Some((index + 2, index));
            break;
        }
        index += 1;
    }
    let (end, payload_end) = terminator?;
    let payload = &source[payload_start..payload_end];
    let kind = if is_safe_osc(payload) {
        ControlKind::Metadata
    } else {
        ControlKind::Other
    };
    Some((end, kind))
}

fn parse_string_control(source: &[u8], payload_start: usize) -> Option<(usize, ControlKind)> {
    let mut index = payload_start;
    let mut utf8 = Utf8ContinuationState::default();
    while index < source.len() {
        let byte = source[index];
        let utf8_state = utf8.advance(byte);
        if byte == 0x9c
            && matches!(
                utf8_state,
                Utf8ByteState::Other | Utf8ByteState::EncodedC1St
            )
        {
            return Some((index + 1, ControlKind::Other));
        }
        if byte == 0x1b && source.get(index + 1) == Some(&b'\\') {
            return Some((index + 2, ControlKind::Other));
        }
        index += 1;
    }
    None
}

/// These OSC commands carry terminal metadata or hyperlink destinations; they
/// do not reposition the cursor or change the intensity state of visible text.
#[must_use]
pub(crate) fn is_safe_osc(payload: &[u8]) -> bool {
    let Some(separator) = payload.iter().position(|byte| *byte == b';') else {
        return false;
    };
    match &payload[..separator] {
        b"0" | b"2" | b"7" | b"133" => true,
        b"8" => payload[separator + 1..].contains(&b';'),
        _ => false,
    }
}

/// Bracketed paste changes input reporting; the ASCII charset reset leaves
/// Latin output and intensity unchanged. Both occur in older fish prompt hooks.
#[must_use]
pub(crate) fn is_presentation_neutral_control(sequence: &[u8]) -> bool {
    if sequence == b"\x1b(B" {
        return true;
    }
    csi_parameter_start(sequence)
        .is_some_and(|start| matches!(&sequence[start..], b"?2004h" | b"?2004l"))
}

#[must_use]
fn utf8_width(byte: u8) -> usize {
    match byte {
        0x00..=0x7f => 1,
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => 0,
    }
}

#[must_use]
fn sgr_params(sequence: &[u8]) -> Option<Vec<SgrParam>> {
    let parameter_start = csi_parameter_start(sequence)?;
    if sequence.len() <= parameter_start || *sequence.last()? != b'm' {
        return None;
    }
    let parameter_bytes = &sequence[parameter_start..sequence.len() - 1];
    if !parameter_bytes
        .iter()
        .all(|byte| byte.is_ascii_digit() || matches!(*byte, b';' | b':'))
    {
        return None;
    }
    if parameter_bytes.is_empty() {
        return Some(vec![SgrParam {
            code: 0,
            colon_form: false,
            colon_value: None,
        }]);
    }
    let mut params = Vec::new();
    let semicolon_separated_parameters = parameter_bytes.split(|byte| *byte == b';');
    for part in semicolon_separated_parameters {
        let mut colon_parts = part.split(|byte| *byte == b':');
        let code_part = colon_parts.next().unwrap_or_default();
        let colon_form = colon_parts.next().is_some();
        let colon_value = if colon_form {
            let value_part = part.split(|byte| *byte == b':').nth(1).unwrap_or_default();
            if value_part.is_empty() {
                None
            } else {
                Some(str::from_utf8(value_part).ok()?.parse().ok()?)
            }
        } else {
            None
        };
        if code_part.is_empty() {
            params.push(SgrParam {
                code: 0,
                colon_form: false,
                colon_value: None,
            });
        } else {
            let text = str::from_utf8(code_part).ok()?;
            let code: u16 = text.parse().ok()?;
            params.push(SgrParam {
                code,
                colon_form,
                colon_value,
            });
        }
    }
    let mut index = 0;
    while index < params.len() {
        let parameter = params[index];
        if matches!(parameter.code, 38 | 48 | 58) && !parameter.colon_form {
            match params.get(index + 1).map(|value| value.code) {
                Some(5) if index + 2 < params.len() => index += 3,
                Some(2) if index + 4 < params.len() => index += 5,
                _ => return None,
            }
        } else {
            index += 1;
        }
    }
    Some(params)
}

fn csi_parameter_start(sequence: &[u8]) -> Option<usize> {
    if sequence.starts_with(b"\x1b[") {
        Some(2)
    } else if sequence.first() == Some(&0x9b) {
        Some(1)
    } else if sequence.starts_with(b"\xc2\x9b") {
        Some(2)
    } else {
        None
    }
}

#[must_use]
fn sgr_resets_emphasis(sequence: &[u8], emphasis: Emphasis) -> bool {
    let Some(params) = sgr_params(sequence) else {
        return false;
    };
    let mut index = 0;
    while index < params.len() {
        let parameter = params[index];
        if matches!(parameter.code, 38 | 48 | 58) && !parameter.colon_form {
            match params.get(index + 1).map(|value| value.code) {
                Some(5) => index += 3,
                Some(2) => index += 5,
                _ => return false,
            }
            continue;
        }
        let resets = match emphasis {
            Emphasis::Bold => matches!(parameter.code, 0 | 22),
            Emphasis::Underline => {
                matches!(parameter.code, 0 | 24)
                    || (parameter.code == 4
                        && parameter.colon_form
                        && parameter.colon_value == Some(0))
            }
            Emphasis::Reverse => matches!(parameter.code, 0 | 27),
        };
        if resets && (parameter.code != 4 || parameter.colon_form) {
            return true;
        }
        index += 1;
    }
    false
}

#[must_use]
fn visible_text(tokens: &[Token]) -> (String, Vec<CharMap>) {
    let mut visible = String::new();
    let mut char_map = Vec::new();
    let indexed_tokens = tokens.iter().enumerate();
    for (token_index, token) in indexed_tokens {
        match token.kind {
            TokenKind::Text(character) => {
                let start = visible.len();
                visible.push(character);
                char_map.push(CharMap {
                    start,
                    end: visible.len(),
                    token: token_index,
                });
            }
            TokenKind::Raw => {
                let start = visible.len();
                visible.push('\u{fffd}');
                char_map.push(CharMap {
                    start,
                    end: visible.len(),
                    token: token_index,
                });
            }
            TokenKind::Control(_) => {}
        }
    }

    (visible, char_map)
}

#[must_use]
#[expect(
    clippy::range_plus_one,
    reason = "the result must be an end-exclusive Range for slicing token arrays"
)]
fn token_range_for_span(span: &Range<usize>, char_map: &[CharMap]) -> Option<Range<usize>> {
    let first = char_map
        .iter()
        .find(|mapping| mapping.start <= span.start && span.start < mapping.end)?;
    let last = char_map
        .iter()
        .find(|mapping| mapping.start < span.end && mapping.end >= span.end)?;
    Some(first.token..last.token + 1)
}

#[must_use]
fn graphemes(tokens: &[Token]) -> Vec<Grapheme> {
    let (visible, char_map) = visible_text(tokens);
    let mut result = Vec::new();
    let mut map_index = 0;
    let grapheme_ranges = visible.grapheme_indices(true);
    for (start, value) in grapheme_ranges {
        let end = start + value.len();
        while char_map[map_index].end <= start {
            map_index += 1;
        }
        let mut last_index = map_index;
        while last_index + 1 < char_map.len() && char_map[last_index + 1].start < end {
            last_index += 1;
        }
        let first = char_map[map_index];
        let last = char_map[last_index];
        result.push(Grapheme {
            start_token: first.token,
            end_token: last.token + 1,
            kind: if matches!(tokens[first.token].kind, TokenKind::Raw) {
                GraphemeKind::Raw
            } else {
                classify_grapheme(value)
            },
            extends_previous_word: !matches!(tokens[first.token].kind, TokenKind::Raw)
                && grapheme_extends_letter(value),
            contains_extension: value.chars().any(is_grapheme_extension),
        });
        map_index = last_index + 1;
    }
    result
}

#[must_use]
fn classify_grapheme(value: &str) -> GraphemeKind {
    if matches!(value, "'" | "’" | "ʼ") {
        return GraphemeKind::Apostrophe;
    }
    if value.chars().next().is_some_and(is_word_connector) {
        return GraphemeKind::Connector;
    }
    if is_letter_grapheme(value) {
        return GraphemeKind::Letter;
    }
    if value.chars().any(char::is_numeric) {
        return GraphemeKind::Number;
    }
    GraphemeKind::Other
}

fn grapheme_extends_letter(value: &str) -> bool {
    let mut probe = String::with_capacity(value.len().saturating_add(1));
    probe.push('a');
    probe.push_str(value);
    let mut graphemes = probe.graphemes(true);
    let Some(first) = graphemes.next() else {
        return false;
    };
    graphemes.next().is_none() && is_letter_grapheme(first)
}

#[must_use]
fn trailing_candidate_start(units: &[Grapheme]) -> Option<usize> {
    let last = units.len().checked_sub(1)?;
    // Combining marks can affect ICU's following complex-script word boundaries,
    // including when a malformed byte is their grapheme's visible placeholder.
    let is_word_unit = |unit: &Grapheme| {
        matches!(unit.kind, GraphemeKind::Letter | GraphemeKind::Number) || unit.contains_extension
    };
    let is_connector = |kind| matches!(kind, GraphemeKind::Apostrophe | GraphemeKind::Connector);
    if !is_word_unit(&units[last]) && !is_connector(units[last].kind) {
        return None;
    }

    // Keep the whole contiguous possible word, including a trailing or
    // leading connector run: ICU can resolve those connectors only after a
    // neighboring word character arrives (for example, ExtendNumLet).
    let mut cursor = last;
    // A raw or nonword base still closes the preceding word. Retain that
    // grapheme only as context for the word that may follow its extending marks.
    while cursor > 0
        && !matches!(units[cursor].kind, GraphemeKind::Raw | GraphemeKind::Other)
        && (is_word_unit(&units[cursor - 1]) || is_connector(units[cursor - 1].kind))
    {
        cursor -= 1;
    }
    Some(units[cursor].start_token)
}

#[must_use]
fn trailing_segmenter_candidate(tokens: &[Token], segmenter: &dyn WordSegmenter) -> Option<usize> {
    let (visible, char_map) = visible_text(tokens);
    let start = segmenter.terminal_trailing_word_start(&visible)?;
    char_map
        .iter()
        .find(|mapping| mapping.start <= start && start < mapping.end)
        .map(|mapping| mapping.token)
}

fn segmenter_trailing_start(segmenter: &dyn WordSegmenter, text: &str) -> Option<usize> {
    let start = segmenter.terminal_trailing_word_start(text)?;
    assert!(
        start < text.len()
            && text.is_char_boundary(start)
            && text
                .grapheme_indices(true)
                .any(|(boundary, _)| boundary == start),
        "word segmenter returned a trailing start that splits a character or grapheme"
    );
    Some(start)
}

fn trailing_grapheme_candidate_start(text: &str) -> Option<usize> {
    let units: Vec<(usize, GraphemeKind)> = text
        .grapheme_indices(true)
        .map(|(start, grapheme)| (start, classify_grapheme(grapheme)))
        .collect();
    let last = units.len().checked_sub(1)?;
    let is_word_unit = |kind| matches!(kind, GraphemeKind::Letter | GraphemeKind::Number);
    let is_connector = |kind| matches!(kind, GraphemeKind::Apostrophe | GraphemeKind::Connector);
    if !is_word_unit(units[last].1) && !is_connector(units[last].1) {
        return None;
    }

    let mut cursor = last;
    while cursor > 0 && (is_word_unit(units[cursor - 1].1) || is_connector(units[cursor - 1].1)) {
        cursor -= 1;
    }
    Some(units[cursor].0)
}

fn letter_grapheme_count(text: &str) -> usize {
    text.graphemes(true)
        .filter(|grapheme| is_letter_grapheme(grapheme))
        .count()
}

#[must_use]
fn chunk_can_extend_word(input: &[u8]) -> bool {
    if input
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'\'')
    {
        return true;
    }
    let (tokens, _) = scan_tokens(input, /* eof */ false);
    if tokens
        .iter()
        .any(|token| matches!(token.kind, TokenKind::Raw))
    {
        return false;
    }
    graphemes(&tokens).iter().all(|unit| {
        matches!(
            unit.kind,
            GraphemeKind::Letter
                | GraphemeKind::Number
                | GraphemeKind::Apostrophe
                | GraphemeKind::Connector
        )
    })
}

fn chunk_can_extend_word_after_pending(pending: &[u8], appended: &[u8], open_word: bool) -> bool {
    if open_word {
        if let Some(start) = incomplete_utf8_suffix_start(pending) {
            let mut suffix = Vec::with_capacity(pending.len() - start + appended.len());
            suffix.extend_from_slice(&pending[start..]);
            suffix.extend_from_slice(appended);
            if chunk_can_extend_word(&suffix) || chunk_can_extend_with_grapheme_extension(&suffix) {
                return true;
            }
            // Keep an unresolved scalar with the open word. A definitely
            // malformed sequence is a delimiter and must be processed now.
            if appended.iter().all(|byte| *byte >= 0x80) && is_incomplete_utf8(&suffix) {
                return true;
            }
            return false;
        }
    }

    if chunk_can_extend_word(appended) {
        return true;
    }

    if open_word && appended.iter().all(|byte| *byte >= 0x80) {
        if is_incomplete_utf8(appended) {
            // A partial scalar needs a following byte before its
            // relationship to the preceding word can be decided.
            return true;
        }
        return chunk_can_extend_with_grapheme_extension(appended);
    }

    false
}

fn is_incomplete_utf8(input: &[u8]) -> bool {
    str::from_utf8(input).is_err_and(|error| error.error_len().is_none())
}

fn incomplete_utf8_suffix_start(input: &[u8]) -> Option<usize> {
    for start in input.len().saturating_sub(3)..input.len() {
        let width = utf8_width(input[start]);
        if width < 2 || start + width <= input.len() {
            continue;
        }
        let error = str::from_utf8(&input[start..]).err()?;
        if error.error_len().is_none() && error.valid_up_to() == 0 {
            return Some(start);
        }
    }
    None
}

fn utf8_state_for_incomplete_suffix(input: &[u8]) -> Option<Utf8ContinuationState> {
    if incomplete_utf8_suffix_start(input) != Some(0) {
        return None;
    }
    let mut state = Utf8ContinuationState::default();
    for byte in input.iter().copied() {
        state.advance(byte);
    }
    (state.remaining > 0).then_some(state)
}

fn chunk_can_extend_with_grapheme_extension(input: &[u8]) -> bool {
    let (tokens, _) = scan_tokens(input, /* eof */ false);
    if tokens
        .iter()
        .any(|token| matches!(token.kind, TokenKind::Raw))
    {
        return false;
    }
    let (visible, _) = visible_text(&tokens);
    let mut probe = String::with_capacity(visible.len().saturating_add(1));
    probe.push('a');
    probe.push_str(&visible);
    probe.graphemes(true).all(|grapheme| {
        matches!(
            classify_grapheme(grapheme),
            GraphemeKind::Letter
                | GraphemeKind::Number
                | GraphemeKind::Apostrophe
                | GraphemeKind::Connector
        )
    })
}

impl DrainingControl {
    #[must_use]
    fn new(bytes: &[u8]) -> Self {
        let mode = control_mode(bytes).expect("incomplete control has a drain mode");
        Self {
            bytes: if bytes.len() <= MAX_PENDING {
                bytes.to_vec()
            } else {
                Vec::new()
            },
            mode,
            tracking: bytes.len() <= MAX_PENDING,
        }
    }
}

fn control_mode(bytes: &[u8]) -> Option<DrainMode> {
    if bytes.first() == Some(&0x1b) {
        return Some(if bytes == b"\x1b" {
            DrainMode::EscapeIntro
        } else if bytes.starts_with(b"\x1b[") {
            DrainMode::Csi
        } else if bytes.starts_with(b"\x1b]") {
            DrainMode::Osc {
                previous_escape: bytes.last() == Some(&0x1b),
                utf8: string_utf8_state(bytes, 2),
            }
        } else if bytes.starts_with(b"\x1bP")
            || bytes.starts_with(b"\x1b^")
            || bytes.starts_with(b"\x1b_")
            || bytes.starts_with(b"\x1bX")
        {
            DrainMode::String {
                previous_escape: bytes.last() == Some(&0x1b),
                utf8: string_utf8_state(bytes, 2),
            }
        } else {
            DrainMode::Escape {
                intermediate: bytes
                    .iter()
                    .skip(1)
                    .any(|byte| (0x20..=0x2f).contains(byte)),
            }
        });
    }

    let (header_len, kind) = c1_control_intro(bytes, 0)?;
    Some(match kind {
        C1ControlKind::Csi => DrainMode::Csi,
        C1ControlKind::Osc => DrainMode::Osc {
            previous_escape: bytes.last() == Some(&0x1b),
            utf8: string_utf8_state(bytes, header_len),
        },
        C1ControlKind::String => DrainMode::String {
            previous_escape: bytes.last() == Some(&0x1b),
            utf8: string_utf8_state(bytes, header_len),
        },
    })
}

fn string_utf8_state(bytes: &[u8], payload_start: usize) -> Utf8ContinuationState {
    let mut state = Utf8ContinuationState::default();
    for byte in bytes.iter().copied().skip(payload_start) {
        state.advance(byte);
    }
    state
}

#[must_use]
fn drain_mode_complete(mode: &mut DrainMode, byte: u8) -> bool {
    match mode {
        DrainMode::EscapeIntro => match byte {
            b'[' => {
                *mode = DrainMode::Csi;
                false
            }
            b']' => {
                *mode = DrainMode::Osc {
                    previous_escape: false,
                    utf8: Utf8ContinuationState::default(),
                };
                false
            }
            b'P' | b'^' | b'_' | b'X' => {
                *mode = DrainMode::String {
                    previous_escape: false,
                    utf8: Utf8ContinuationState::default(),
                };
                false
            }
            0x20..=0x2f => {
                *mode = DrainMode::Escape { intermediate: true };
                false
            }
            _ => true,
        },
        DrainMode::Csi => (0x40..=0x7e).contains(&byte),
        DrainMode::Osc {
            previous_escape,
            utf8,
        } => {
            let utf8_state = utf8.advance(byte);
            let complete = (*previous_escape && byte == b'\\')
                || byte == 0x07
                || (byte == 0x9c
                    && matches!(
                        utf8_state,
                        Utf8ByteState::Other | Utf8ByteState::EncodedC1St
                    ));
            *previous_escape = byte == 0x1b;
            complete
        }
        DrainMode::String {
            previous_escape,
            utf8,
        } => {
            let utf8_state = utf8.advance(byte);
            let complete = (*previous_escape && byte == b'\\')
                || (byte == 0x9c
                    && matches!(
                        utf8_state,
                        Utf8ByteState::Other | Utf8ByteState::EncodedC1St
                    ));
            *previous_escape = byte == 0x1b;
            complete
        }
        DrainMode::Escape { intermediate } => {
            if *intermediate {
                !(0x20..=0x2f).contains(&byte)
            } else if (0x20..=0x2f).contains(&byte) {
                *intermediate = true;
                false
            } else {
                (0x30..=0x7e).contains(&byte) || !(0x20..=0x2f).contains(&byte)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use proptest::prelude::{Just, ProptestConfig, prop_oneof};
    use proptest::{prop_assert_eq, proptest};

    use crate::segmentation::WordSegmenter;
    use crate::settings::{Emphasis, LengthRule};

    use super::{MAX_PENDING, Transformer};
    use crate::settings::Settings;

    #[derive(Clone, Copy, Debug)]
    struct SplitAfterTwo;

    impl WordSegmenter for SplitAfterTwo {
        fn word_ranges(&self, text: &str) -> Vec<Range<usize>> {
            if text == "abcdef!" {
                vec![0..2, 2..6]
            } else {
                Vec::new()
            }
        }
    }

    #[derive(Clone, Copy, Debug)]
    struct AlphabeticRuns;

    impl WordSegmenter for AlphabeticRuns {
        fn word_ranges(&self, text: &str) -> Vec<Range<usize>> {
            let mut ranges = Vec::new();
            let mut start = None;
            let mut end = 0;
            for (offset, character) in text.char_indices() {
                if character.is_alphabetic() {
                    start.get_or_insert(offset);
                    end = offset + character.len_utf8();
                } else if let Some(range_start) = start.take() {
                    ranges.push(range_start..end);
                }
            }
            if let Some(range_start) = start {
                ranges.push(range_start..end);
            }
            ranges
        }
    }

    fn strip_emphasis(input: &[u8]) -> Vec<u8> {
        let mut output = Vec::with_capacity(input.len());
        let mut cursor = 0;
        while cursor < input.len() {
            if input[cursor..].starts_with(b"\x1b[1m") {
                cursor += 4;
            } else if input[cursor..].starts_with(b"\x1b[22m") {
                cursor += 5;
            } else {
                output.push(input[cursor]);
                cursor += 1;
            }
        }
        output
    }

    fn assert_bytewise_scan_work_is_linear(input: &[u8], settings: &Settings) {
        super::TOKEN_SCAN_BYTES.with(|counter| counter.set(0));
        let expected = super::transform(input, settings, true);
        let expected_scan_bytes = super::TOKEN_SCAN_BYTES.with(std::cell::Cell::get);
        super::TOKEN_SCAN_BYTES.with(|counter| counter.set(0));

        let mut renderer = Transformer::new(settings, true);
        let mut actual = Vec::new();
        for byte in input {
            actual.extend(renderer.push(std::slice::from_ref(byte)));
        }
        actual.extend(renderer.finish());
        let scanned_bytes = super::TOKEN_SCAN_BYTES.with(std::cell::Cell::get);

        if actual != expected {
            let mismatch = actual
                .iter()
                .zip(&expected)
                .position(|(actual, expected)| actual != expected)
                .unwrap_or(actual.len().min(expected.len()));
            let start = mismatch.saturating_sub(16);
            let actual_end = (mismatch + 32).min(actual.len());
            let expected_end = (mismatch + 32).min(expected.len());
            panic!(
                "stream output differs at byte {mismatch}: actual_len={}, expected_len={}, actual={:?}, expected={:?}",
                actual.len(),
                expected.len(),
                &actual[start..actual_end],
                &expected[start..expected_end]
            );
        }
        assert!(
            scanned_bytes <= input.len().saturating_mul(12).saturating_add(64),
            "token scanner revisited too much input: {scanned_bytes} bytes scanned for {} input bytes (slice reference scanned {expected_scan_bytes})",
            input.len()
        );
    }

    #[test]
    fn invalid_settings_are_rejected_before_processing_input() {
        let invalid = [
            Settings {
                every: 0,
                ..Settings::default()
            },
            Settings {
                fraction: f64::NAN,
                ..Settings::default()
            },
            Settings {
                fraction: 1.1,
                ..Settings::default()
            },
            Settings {
                min_word_length: 0,
                ..Settings::default()
            },
        ];
        for settings in invalid {
            for enabled in [false, true] {
                let error = Transformer::try_new(&settings, enabled)
                    .expect_err("invalid settings must fail at construction");
                assert!(error.is_invalid_input());
            }
        }
    }

    #[test]
    #[should_panic(expected = "transformer settings must be valid")]
    fn infallible_constructor_reports_invalid_settings_immediately() {
        let _transformer = Transformer::new(
            &Settings {
                every: 0,
                ..Settings::default()
            },
            true,
        );
    }

    #[test]
    fn configuration_changes_apply_only_to_a_new_transformer() {
        let mut settings = Settings::default();
        let mut current = Transformer::new(&settings, /* enabled */ true);
        settings.fraction = 1.0;

        let mut output = current.push(b"reading");
        output.extend(current.finish());
        assert_eq!(output, b"\x1b[1mread\x1b[22ming");
        assert_eq!(
            super::transform(b"reading", &settings, /* enabled */ true),
            b"\x1b[1mreading\x1b[22m"
        );
    }

    #[test]
    fn custom_word_segmenter_drives_ansi_fixation_boundaries() {
        let segmenter = SplitAfterTwo;
        let mut transformer =
            Transformer::new_with_segmenter(&Settings::default(), true, Box::new(segmenter));
        let mut actual = transformer.push(b"abcdef!");
        actual.extend(transformer.finish());
        assert_eq!(actual, b"\x1b[1ma\x1b[22mb\x1b[1mcd\x1b[22mef!");

        for width in 1..=b"abcdef!".len() {
            let mut transformer =
                Transformer::new_with_segmenter(&Settings::default(), true, Box::new(segmenter));
            let mut output = Vec::new();
            for chunk in b"abcdef!".chunks(width) {
                output.extend(transformer.push(chunk));
            }
            output.extend(transformer.finish());
            assert_eq!(
                output, b"\x1b[1ma\x1b[22mb\x1b[1mcd\x1b[22mef!",
                "width={width}"
            );
        }

        assert_eq!(
            super::transform_with_segmenter(b"abcdef!", &Settings::default(), true, &segmenter),
            b"\x1b[1ma\x1b[22mb\x1b[1mcd\x1b[22mef!"
        );
    }

    #[test]
    fn bracketed_paste_input_mode_does_not_disable_output_emphasis() {
        assert_eq!(
            super::transform(b"\x1b[?2004l\x1b(Breading!", &Settings::default(), true),
            b"\x1b[?2004l\x1b(B\x1b[1mread\x1b[22ming!"
        );
    }

    #[test]
    fn terminal_metadata_does_not_disable_emphasis_or_change_payloads() {
        let input = b"\x1b]133;C;cmdline=reading\x1b\\\x1b]0;reading title\x07reading!";
        assert_eq!(
            super::transform(input, &Settings::default(), true),
            b"\x1b]133;C;cmdline=reading\x1b\\\x1b]0;reading title\x07\x1b[1mread\x1b[22ming!"
        );
    }

    #[test]
    fn arbitrary_chunking_preserves_mixed_text_and_terminal_sequences() {
        let samples: &[&[u8]] = &[
            "we're l’amour cafe\u{301} naïve a_b-12.log https://example.test Καλημέρα 😀\n"
                .as_bytes(),
            b"alpha''beta 'gamma' delta\xffepsilon\r\n",
            b"\x1b[2;38;2;1;2;22mreading\x1b[0m next \x1b[1mword\x1b[22m!",
            b"\x1b]8;;https://example.test\x1b\\reading\x1b]8;;\x1b\\ next!",
            b"before \x1bPopaque payload\x1b\\after \x1b[0mreading!",
            b"rea\x1b[31mding\x1b[0m foo'\x1b[2mbar\x1b[22m end\x1b[",
        ];
        for every in [1, 2, 3] {
            let settings = Settings {
                every,
                ..Settings::default()
            };
            for input in samples {
                let expected = super::transform(input, &settings, true);
                for width in 1..=input.len() {
                    let mut renderer = Transformer::new(&settings, true);
                    let mut actual = Vec::new();
                    let chunks = input.chunks(width);
                    for chunk in chunks {
                        actual.extend(renderer.push(chunk));
                    }
                    actual.extend(renderer.finish());
                    assert_eq!(
                        actual, expected,
                        "every={every}, width={width}, input={input:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn oversized_word_keeps_following_word_selection_consistent_across_chunks() {
        let mut input = vec![b'a'; MAX_PENDING + 1];
        input.extend_from_slice(b" beta gamma delta ");
        let settings = Settings {
            every: 2,
            ..Settings::default()
        };
        let mut expected = vec![b'a'; MAX_PENDING + 1];
        expected.extend_from_slice(b" beta \x1b[1mgam\x1b[22mma delta ");
        for width in [1, 8192, MAX_PENDING, input.len()] {
            let mut renderer = Transformer::new(&settings, true);
            let mut actual = Vec::new();
            let chunks = input.chunks(width);
            for chunk in chunks {
                actual.extend(renderer.push(chunk));
            }
            actual.extend(renderer.finish());
            assert_eq!(actual, expected, "width={width}");
        }
    }

    #[test]
    fn timed_flush_counts_each_completed_custom_segmenter_range() {
        let settings = Settings {
            every: 2,
            ..Settings::default()
        };
        let mut renderer =
            Transformer::new_with_segmenter(&settings, true, Box::new(AlphabeticRuns));
        assert!(renderer.push(b"aa_bb").is_empty());
        let mut actual = renderer.flush_pending();
        actual.extend(renderer.push(b"! cc dd "));
        actual.extend(renderer.finish());

        assert_eq!(actual, b"aa_bb! \x1b[1mc\x1b[22mc dd ");
    }

    #[test]
    fn overflow_counts_each_completed_custom_segmenter_range() {
        let settings = Settings {
            every: 2,
            ..Settings::default()
        };
        let mut oversized = Vec::with_capacity(MAX_PENDING);
        for _ in 0..(MAX_PENDING - 1) / 3 {
            oversized.extend_from_slice(b"aa_");
        }
        oversized.push(b'a');
        assert_eq!(oversized.len(), MAX_PENDING);

        let mut renderer =
            Transformer::new_with_segmenter(&settings, true, Box::new(AlphabeticRuns));
        let mut actual = renderer.push(&oversized);
        actual.extend(renderer.push(b"a! bb cc dd "));
        actual.extend(renderer.finish());

        let mut expected = oversized;
        expected.extend_from_slice(b"a! \x1b[1mb\x1b[22mb cc \x1b[1md\x1b[22md ");
        assert_eq!(actual, expected);
    }

    #[test]
    fn repeated_apostrophes_delimit_a_timed_out_word() {
        let settings = Settings {
            every: 2,
            ..Settings::default()
        };
        for chunks in [
            vec![&b"''beta gamma delta "[..]],
            vec![&b"'"[..], &b"'beta gamma delta "[..]],
        ] {
            let mut renderer = Transformer::new(&settings, true);
            assert!(renderer.push(b"alpha").is_empty());
            let mut actual = renderer.flush_pending();
            for chunk in chunks {
                actual.extend(renderer.push(chunk));
            }
            actual.extend(renderer.finish());
            assert_eq!(actual, b"alpha''beta \x1b[1mgam\x1b[22mma delta ");
        }
    }

    #[test]
    fn emphasizes_the_selected_prefix_of_a_word() {
        let settings = Settings::default();
        assert_eq!(
            super::transform(b"reading", &settings, true),
            b"\x1b[1mread\x1b[22ming"
        );
    }

    #[test]
    fn oversized_complete_word_in_one_chunk_stays_unchanged() {
        let input = format!("{} ", "a".repeat(MAX_PENDING + 1));
        assert_eq!(
            super::transform(input.as_bytes(), &Settings::default(), true),
            input.as_bytes()
        );
    }

    #[test]
    fn color_components_do_not_change_saved_bold_or_dim_state() {
        let settings = Settings::default();
        let truecolor = b"\x1b[2;38;2;255;1;2mreading!";
        assert_eq!(
            super::transform(truecolor, &settings, true),
            b"\x1b[2;38;2;255;1;2m\x1b[1mread\x1b[22m\x1b[2ming!"
        );

        let indexed = b"\x1b[2;38;5;22mreading!";
        assert_eq!(
            super::transform(indexed, &settings, true),
            b"\x1b[2;38;5;22m\x1b[1mread\x1b[22m\x1b[2ming!"
        );
    }

    #[test]
    fn chunk_boundaries_inside_utf8_and_sgr_do_not_change_output() {
        let input = b"cafe\xcc\x81 \x1b[31mreading\x1b[0m";
        let settings = Settings::default();
        let expected = super::transform(input, &settings, true);

        for split in 0..=input.len() {
            let mut transformer = Transformer::new(&settings, true);
            let mut output = transformer.push(&input[..split]);
            output.extend(transformer.push(&input[split..]));
            output.extend(transformer.finish());
            assert_eq!(output, expected, "split at byte {split}");
        }
    }

    #[test]
    fn timed_out_word_and_its_remainder_stay_passthrough() {
        let mut transformer = Transformer::new(&Settings::default(), true);
        assert!(transformer.push(b"read").is_empty());
        assert_eq!(transformer.flush_pending(), b"read");
        assert_eq!(
            transformer.push(b"ing! next "),
            b"ing! \x1b[1mne\x1b[22mxt "
        );
        assert!(transformer.finish().is_empty());
    }

    #[test]
    fn timeout_between_utf8_bytes_preserves_them_and_transforms_later_words() {
        let mut transformer = Transformer::new(&Settings::default(), true);
        let mut actual = transformer.push(&[0xc3]);
        assert!(actual.is_empty());

        actual.extend(transformer.flush_pending());
        actual.extend(transformer.push(b"\x9bnicode reading "));
        actual.extend(transformer.finish());

        assert_eq!(
            actual,
            b"\xc3\x9b\x1b[1mnic\x1b[22mode \x1b[1mread\x1b[22ming "
        );

        let mut transformer = Transformer::new(&Settings::default(), true);
        let mut actual = transformer.push(&[0xc2]);
        assert!(actual.is_empty());
        actual.extend(transformer.flush_pending());
        actual.extend(transformer.push(b"\x9b reading "));
        actual.extend(transformer.finish());
        assert_eq!(actual, b"\xc2\x9b \x1b[1mread\x1b[22ming ");
    }

    #[test]
    fn flushing_incomplete_bytes_does_not_end_a_timed_out_word() {
        let settings = Settings {
            every: 2,
            ..Settings::default()
        };
        let mut transformer = Transformer::new(&settings, true);
        assert!(transformer.push(b"alpha").is_empty());
        let mut actual = transformer.flush_pending();
        assert!(transformer.push(&[0xc3]).is_empty());
        actual.extend(transformer.flush_pending());
        actual.extend(transformer.push(b"\xff beta gamma "));
        actual.extend(transformer.finish());

        assert_eq!(actual, b"alpha\xc3\xff beta \x1b[1mgam\x1b[22mma ");
    }

    #[test]
    fn timed_out_word_advances_frequency_when_it_reaches_a_delimiter() {
        let settings = Settings {
            every: 2,
            ..Settings::default()
        };
        let mut transformer = Transformer::new(&settings, true);
        assert!(transformer.push(b"al").is_empty());
        assert_eq!(transformer.flush_pending(), b"al");
        assert_eq!(
            transformer.push(b"pha beta gamma "),
            b"pha beta \x1b[1mgam\x1b[22mma "
        );
    }

    #[test]
    fn timed_out_short_word_counts_after_later_letters_make_it_eligible() {
        let settings = Settings {
            every: 2,
            min_word_length: 3,
            ..Settings::default()
        };
        let mut transformer = Transformer::new(&settings, true);
        assert!(transformer.push(b"ab").is_empty());
        assert_eq!(transformer.flush_pending(), b"ab");
        assert_eq!(
            transformer.push(b"c delta epsilon zeta "),
            b"c delta \x1b[1mepsi\x1b[22mlon zeta "
        );
    }

    #[test]
    fn malformed_bytes_delimit_a_timed_out_word_for_frequency() {
        let settings = Settings {
            every: 3,
            ..Settings::default()
        };
        let mut transformer = Transformer::new(&settings, true);
        assert!(transformer.push(b"al").is_empty());
        assert_eq!(transformer.flush_pending(), b"al");
        assert_eq!(
            transformer.push(b"\xffpha beta gamma delta "),
            b"\xffpha beta \x1b[1mgam\x1b[22mma delta "
        );
    }

    #[test]
    fn malformed_bytes_are_preserved_while_surrounding_words_transform() {
        let input = b"reading\xff!";
        assert_eq!(
            super::transform(input, &Settings::default(), true),
            b"\x1b[1mread\x1b[22ming\xff!"
        );
    }

    #[test]
    fn unknown_control_keeps_following_text_in_passthrough_until_reset() {
        let input = b"\x1b[2Jreading \x1b[0mreading!";
        assert_eq!(
            super::transform(input, &Settings::default(), true),
            b"\x1b[2Jreading \x1b[0m\x1b[1mread\x1b[22ming!"
        );
    }

    #[test]
    fn single_character_escape_does_not_consume_the_next_escape_sequence() {
        let input = b"\x1bc\x1b[0mreading!";
        assert_eq!(
            super::transform(input, &Settings::default(), true),
            b"\x1bc\x1b[0m\x1b[1mread\x1b[22ming!"
        );
    }

    #[test]
    fn apostrophes_are_internal_only_between_latin_letters() {
        assert_eq!(
            super::transform(b"can't foo' bar", &Settings::default(), true),
            b"\x1b[1mca\x1b[22mn't \x1b[1mfo\x1b[22mo' \x1b[1mba\x1b[22mr"
        );
    }

    #[test]
    fn contractions_stay_one_word_across_push_boundaries() {
        assert_eq!(
            super::transform(b"we're", &Settings::default(), true),
            b"\x1b[1mwe\x1b[22m're"
        );

        let mut transformer = Transformer::new(&Settings::default(), true);
        assert!(transformer.push(b"we").is_empty());
        assert!(transformer.push(b"'").is_empty());
        let mut output = transformer.push(b"re l'");
        output.extend(transformer.push(b"am"));
        output.extend(transformer.push(b"our!"));
        output.extend(transformer.finish());
        assert_eq!(output, b"\x1b[1mwe\x1b[22m're \x1b[1ml'am\x1b[22mour!");
    }

    #[test]
    fn chunked_sgr_inside_word_keeps_the_emphasis_prefix_literal() {
        let mut transformer = Transformer::new(&Settings::default(), true);
        let mut output = transformer.push(b"rea\x1b[3");
        output.extend(transformer.push(b"1"));
        output.extend(transformer.push(b"m"));
        output.extend(transformer.push(b"ding!"));
        output.extend(transformer.finish());
        assert_eq!(output, b"\x1b[1mrea\x1b[31md\x1b[22ming!");
    }

    #[test]
    fn chunked_osc8_hyperlink_keeps_its_payload_and_emphasizes_inside_link() {
        let mut transformer = Transformer::new(&Settings::default(), true);
        let mut output = transformer.push(b"\x1b]8;;https://example");
        output.extend(transformer.push(b".test\x07read"));
        output.extend(transformer.push(b"ing\x1b]8;;\x07!"));
        output.extend(transformer.finish());
        assert_eq!(
            output,
            b"\x1b]8;;https://example.test\x07\x1b[1mread\x1b[22ming\x1b]8;;\x07!"
        );
    }

    #[test]
    fn decomposed_marks_count_once_and_unicode_scripts_are_emphasized() {
        let input = "cafe\u{301} Καλημέρα 123 😀".as_bytes();
        assert_eq!(
            super::transform(input, &Settings::default(), true),
            "\x1b[1mca\x1b[22mfe\u{301} \x1b[1mΚαλη\x1b[22mμέρα 123 😀".as_bytes()
        );
    }

    #[test]
    fn unicode_words_preserve_text_and_keep_emoji_graphemes_unstyled() {
        let settings = Settings {
            min_word_length: 1,
            ..Settings::default()
        };
        let samples = [
            "café",
            "cafe\u{301}",
            "niño",
            "don't",
            "hyphen-ated",
            "你好世界",
            "日本語",
            "مرحبا",
            "Привет",
            "ภาษาไทย",
        ];

        for text in samples {
            let output = super::transform(text.as_bytes(), &settings, true);
            assert_eq!(strip_emphasis(&output), text.as_bytes(), "{text}");
            assert!(
                output.windows(4).any(|window| window == b"\x1b[1m"),
                "{text}"
            );
        }

        let family = "👨‍👩‍👧‍👦";
        assert_eq!(
            super::transform(family.as_bytes(), &settings, true),
            family.as_bytes()
        );
    }

    #[test]
    fn alphabetic_combining_marks_do_not_turn_punctuation_into_a_letter() {
        let settings = Settings {
            every: 6,
            min_word_length: 1,
            fraction: 0.75,
            ..Settings::default()
        };
        let punctuation = b"\0/\xd8\x94";
        let expected = super::transform(punctuation, &settings, true);
        assert_eq!(expected, punctuation);

        for width in 1..=punctuation.len() {
            let mut renderer = Transformer::new(&settings, true);
            let mut actual = Vec::new();
            for chunk in punctuation.chunks(width) {
                actual.extend(renderer.push(chunk));
            }
            actual.extend(renderer.finish());
            assert_eq!(actual, expected, "punctuation width={width}");
        }

        let arabic_base = "ب\u{0614}".as_bytes();
        let expected = super::transform(arabic_base, &settings, true);
        assert_ne!(expected, arabic_base);
        let mut renderer = Transformer::new(&settings, true);
        let mut actual = Vec::new();
        for byte in arabic_base {
            actual.extend(renderer.push(std::slice::from_ref(byte)));
        }
        actual.extend(renderer.finish());
        assert_eq!(actual, expected);

        let arabic_prepend = "\u{0600}ب".as_bytes();
        let expected = super::transform(arabic_prepend, &settings, true);
        assert_ne!(expected, arabic_prepend);
        let mut renderer = Transformer::new(&settings, true);
        let mut actual = Vec::new();
        for byte in arabic_prepend {
            actual.extend(renderer.push(std::slice::from_ref(byte)));
        }
        actual.extend(renderer.finish());
        assert_eq!(actual, expected);
    }

    #[test]
    fn marked_midnumlet_connector_stays_chunk_invariant() {
        let input = "𐁐.\u{11d3a}A".as_bytes();
        let settings = Settings::default();
        let expected = super::transform(input, &settings, true);

        for width in 1..=input.len() {
            let mut transformer = Transformer::new(&settings, true);
            let mut actual = Vec::new();
            for chunk in input.chunks(width) {
                actual.extend(transformer.push(chunk));
            }
            actual.extend(transformer.finish());
            assert_eq!(actual, expected, "width={width}");
        }
    }

    #[test]
    fn unicode_chunk_boundaries_do_not_change_emphasis_or_bytes() {
        let input = "café cafe\u{301} don't hyphen-ated 你好世界 日本語 مرحبا Привет ภาษาไทย 👨‍👩‍👧‍👦";
        let bytes = input.as_bytes();
        let settings = Settings {
            min_word_length: 1,
            ..Settings::default()
        };
        let expected = super::transform(bytes, &settings, true);

        for split in 0..=bytes.len() {
            let mut transformer = Transformer::new(&settings, true);
            let mut output = transformer.push(&bytes[..split]);
            output.extend(transformer.push(&bytes[split..]));
            output.extend(transformer.finish());
            assert_eq!(output, expected, "split at byte {split}");
        }

        let mut transformer = Transformer::new(&settings, true);
        let mut output = Vec::new();
        for byte in bytes {
            output.extend(transformer.push(std::slice::from_ref(byte)));
        }
        output.extend(transformer.finish());
        assert_eq!(output, expected);
    }

    #[test]
    fn icu_word_with_underscore_and_combining_marks_is_chunk_invariant() {
        let input = b"~~\xde\xaa_\xcd\xaa\xde\xaa_\xcd\xaa";
        let settings = Settings {
            min_word_length: 1,
            ..Settings::default()
        };
        let expected = super::transform(input, &settings, true);
        assert_eq!(strip_emphasis(&expected), input);

        for split in 0..=input.len() {
            let mut transformer = Transformer::new(&settings, true);
            let mut output = transformer.push(&input[..split]);
            output.extend(transformer.push(&input[split..]));
            output.extend(transformer.finish());
            assert_eq!(output, expected, "split at byte {split}");
        }

        let mut transformer = Transformer::new(&settings, true);
        let mut output = Vec::new();
        for byte in input {
            output.extend(transformer.push(std::slice::from_ref(byte)));
        }
        output.extend(transformer.finish());
        assert_eq!(output, expected, "one-byte chunks");
    }

    #[test]
    fn midletter_connectors_are_held_until_the_right_word_boundary() {
        let input = "alpha:βeta!".as_bytes();
        let settings = Settings {
            min_word_length: 1,
            ..Settings::default()
        };
        let expected = super::transform(input, &settings, true);

        for split in 0..=input.len() {
            let mut transformer = Transformer::new(&settings, true);
            let mut output = transformer.push(&input[..split]);
            output.extend(transformer.push(&input[split..]));
            output.extend(transformer.finish());
            assert_eq!(output, expected, "split at byte {split}");
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn unicode_word_connector_streams_match_whole_input(
            leading in proptest::collection::vec(prop_oneof![Just('~'), Just('_')], 1..3),
            prefix in proptest::collection::vec(prop_oneof![Just('a'), Just('β'), Just('\u{07aa}')], 1..5),
            connector in prop_oneof![Just('_'), Just(':'), Just('\u{00b7}'), Just('~')],
            suffix in proptest::collection::vec(prop_oneof![Just('b'), Just('γ'), Just('\u{07ab}')], 1..5),
        ) {
            let text: String = leading
                .into_iter()
                .chain(prefix)
                .chain(std::iter::once(connector))
                .chain(suffix)
                .chain(std::iter::once('!'))
                .collect();
            let input = text.as_bytes();
            let settings = Settings {
                min_word_length: 1,
                ..Settings::default()
            };
            let expected = super::transform(input, &settings, true);

            for split in 0..=input.len() {
                let mut transformer = Transformer::new(&settings, true);
                let mut output = transformer.push(&input[..split]);
                output.extend(transformer.push(&input[split..]));
                output.extend(transformer.finish());
                prop_assert_eq!(
                    output,
                    expected.clone(),
                    "split={}, text={:?}",
                    split,
                    text
                );
            }
        }
    }

    #[test]
    fn configured_emphasis_styles_restore_the_input_state_and_are_idempotent() {
        let cases = [
            (
                Emphasis::Bold,
                b"\x1b[1m".as_slice(),
                b"\x1b[22m".as_slice(),
            ),
            (
                Emphasis::Underline,
                b"\x1b[4m".as_slice(),
                b"\x1b[24m".as_slice(),
            ),
            (
                Emphasis::Reverse,
                b"\x1b[7m".as_slice(),
                b"\x1b[27m".as_slice(),
            ),
        ];

        for (emphasis, start, end) in cases {
            let settings = Settings {
                emphasis,
                ..Settings::default()
            };
            let mut expected = start.to_vec();
            expected.extend_from_slice(b"read");
            expected.extend_from_slice(end);
            expected.extend_from_slice(b"ing!");
            let first = super::transform(b"reading!", &settings, true);
            assert_eq!(first, expected);
            assert_eq!(super::transform(&first, &settings, true), first);

            let input_with_existing_style = match emphasis {
                Emphasis::Bold => b"\x1b[1mreading\x1b[22m next".as_slice(),
                Emphasis::Underline => b"\x1b[4mreading\x1b[24m next".as_slice(),
                Emphasis::Reverse => b"\x1b[7mreading\x1b[27m next".as_slice(),
            };
            let styled = super::transform(input_with_existing_style, &settings, true);
            let prefix_length = input_with_existing_style.len() - b"next".len();
            let mut expected_styled = input_with_existing_style[..prefix_length].to_vec();
            expected_styled.extend_from_slice(start);
            expected_styled.extend_from_slice(b"ne");
            expected_styled.extend_from_slice(end);
            expected_styled.extend_from_slice(b"xt");
            assert_eq!(styled, expected_styled);
            assert_eq!(super::transform(&styled, &settings, true), styled);
        }
    }

    #[test]
    fn colon_form_underline_reset_allows_emphasis_to_start() {
        let settings = Settings {
            emphasis: Emphasis::Underline,
            ..Settings::default()
        };
        assert_eq!(
            super::transform(b"\x1b[4:0mreading!", &settings, true),
            b"\x1b[4:0m\x1b[4mread\x1b[24ming!"
        );
        let first = super::transform(b"\x1b[4:0mreading!", &settings, true);
        assert_eq!(super::transform(&first, &settings, true), first);
    }

    #[test]
    fn underline_variants_are_restored_after_the_emphasized_prefix() {
        let settings = Settings {
            emphasis: Emphasis::Underline,
            ..Settings::default()
        };
        for (source_style, restored_style) in [
            (b"\x1b[4:2m".as_slice(), b"\x1b[4:2m".as_slice()),
            (b"\x1b[4:3m".as_slice(), b"\x1b[4:3m".as_slice()),
            (b"\x1b[21m".as_slice(), b"\x1b[21m".as_slice()),
        ] {
            let mut input = b"reading".to_vec();
            input.splice(2..2, source_style.iter().copied());
            input.push(b'!');

            let mut expected = b"\x1b[4mre".to_vec();
            expected.extend_from_slice(source_style);
            expected.extend_from_slice(b"ad");
            expected.extend_from_slice(b"\x1b[24m");
            expected.extend_from_slice(restored_style);
            expected.extend_from_slice(b"ing!");
            let first = super::transform(&input, &settings, true);
            assert_eq!(first, expected);
            assert_eq!(super::transform(&first, &settings, true), first);
        }
    }

    #[test]
    fn length_rules_choose_fraction_by_letter_grapheme_count() {
        let settings = Settings {
            length_rules: vec![
                LengthRule {
                    max_length: 3,
                    fraction: 1.0,
                },
                LengthRule {
                    max_length: 5,
                    fraction: 0.25,
                },
            ],
            ..Settings::default()
        };
        assert_eq!(
            super::transform(b"cat birds elephant", &settings, true),
            b"\x1b[1mcat\x1b[22m \x1b[1mbi\x1b[22mrds \x1b[1melep\x1b[22mhant"
        );
    }

    #[test]
    fn short_words_do_not_advance_every_n_selection() {
        let settings = Settings {
            every: 2,
            ..Settings::default()
        };
        assert_eq!(
            super::transform(b"a abc def ghi", &settings, true),
            b"a \x1b[1mab\x1b[22mc def \x1b[1mgh\x1b[22mi"
        );
    }

    #[test]
    fn osc8_hyperlinks_and_truncated_controls_are_preserved() {
        let input = b"\x1b]8;;https://example.test\x07reading\x1b]8;;\x07";
        assert_eq!(
            super::transform(input, &Settings::default(), true),
            b"\x1b]8;;https://example.test\x07\x1b[1mread\x1b[22ming\x1b]8;;\x07"
        );

        assert_eq!(
            super::transform(b"reading\x1b[31", &Settings::default(), true),
            b"\x1b[1mread\x1b[22ming\x1b[31"
        );
    }

    #[test]
    fn c1_string_terminators_resume_osc_and_dcs_parsing() {
        let cases: &[(&[u8], &[u8])] = &[
            (
                b"\x1b]0;title\x9creading!",
                b"\x1b]0;title\x9c\x1b[1mread\x1b[22ming!",
            ),
            (
                b"\x1bPpayload\x9c\x1b[0mreading!",
                b"\x1bPpayload\x9c\x1b[0m\x1b[1mread\x1b[22ming!",
            ),
            (
                b"\x1b]0;title\xc2\x9creading!",
                b"\x1b]0;title\xc2\x9c\x1b[1mread\x1b[22ming!",
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(
                super::transform(input, &Settings::default(), true),
                *expected
            );
            for width in [1, 2, 3, 7, input.len()] {
                let mut renderer = Transformer::new(&Settings::default(), true);
                let mut actual = Vec::new();
                for chunk in input.chunks(width) {
                    actual.extend(renderer.push(chunk));
                }
                actual.extend(renderer.finish());
                assert_eq!(actual, *expected, "width={width}, input={input:?}");
            }
        }

        let input = b"\x1b]0;title\xe2\x82\x9crest\x9creading!";
        let expected = b"\x1b]0;title\xe2\x82\x9crest\x9c\x1b[1mread\x1b[22ming!";
        assert_eq!(
            super::transform(input, &Settings::default(), true),
            expected
        );
        let mut renderer = Transformer::new(&Settings::default(), true);
        let mut actual = renderer.push(b"\x1b]0;title\xe2\x82");
        actual.extend(renderer.flush_pending());
        actual.extend(renderer.push(b"\x9crest\x9creading!"));
        actual.extend(renderer.finish());
        assert_eq!(actual, expected);
    }

    #[test]
    fn c1_string_terminators_resume_after_control_capacity_is_exceeded() {
        let mut osc = b"\x1b]0;".to_vec();
        osc.extend(std::iter::repeat_n(b'x', MAX_PENDING + 17));
        osc.extend_from_slice(b"\xc2\x9c\x1b[0mreading!");

        let mut dcs = b"\x1bP".to_vec();
        dcs.extend(std::iter::repeat_n(b'x', MAX_PENDING + 17));
        dcs.extend_from_slice(b"\x9c\x1b[0mreading!");

        for input in [&osc, &dcs] {
            let expected = super::transform(input, &Settings::default(), true);
            let mut renderer = Transformer::new(&Settings::default(), true);
            let mut actual = Vec::new();
            let encoded_c1_st = input.starts_with(b"\x1b]0;");
            let terminator_len = if encoded_c1_st { 2 } else { 1 };
            let prefix_end = input.len() - terminator_len - b"\x1b[0mreading!".len();
            for chunk in input[..prefix_end].chunks(4093) {
                actual.extend(renderer.push(chunk));
            }
            if encoded_c1_st {
                actual.extend(renderer.push(b"\xc2"));
                actual.extend(renderer.push(b"\x9c\x1b[0mreading!"));
            } else {
                actual.extend(renderer.push(b"\x9c\x1b[0mreading!"));
            }
            actual.extend(renderer.finish());
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn malformed_escape_intermediate_releases_following_bytes() {
        let input = b"\x1b(\0\x1b[0mreading!";
        let expected = b"\x1b(\0\x1b[0m\x1b[1mread\x1b[22ming!";
        assert_eq!(
            super::transform(input, &Settings::default(), true),
            expected
        );
        for width in [1, 2, 3, 7, input.len()] {
            let mut renderer = Transformer::new(&Settings::default(), true);
            let mut actual = Vec::new();
            for chunk in input.chunks(width) {
                actual.extend(renderer.push(chunk));
            }
            actual.extend(renderer.finish());
            assert_eq!(actual, expected, "width={width}");
        }

        let mut renderer = Transformer::new(&Settings::default(), true);
        assert!(renderer.push(b"\x1b(").is_empty());
        let mut actual = renderer.push(b"\0\x1b[0mreading!");
        actual.extend(renderer.finish());
        assert_eq!(actual, expected);
    }

    #[test]
    fn c1_sgr_controls_are_preserved_and_restore_input_style() {
        let cases: &[(&[u8], &[u8])] = &[
            (
                b"\x9b31mreading\x9b0m",
                b"\x9b31m\x1b[1mread\x1b[22ming\x9b0m",
            ),
            (
                b"\xc2\x9b31mreading\xc2\x9b0m",
                b"\xc2\x9b31m\x1b[1mread\x1b[22ming\xc2\x9b0m",
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(
                super::transform(input, &Settings::default(), true),
                *expected
            );
            for width in [1, 2, 3, 7, input.len()] {
                let mut renderer = Transformer::new(&Settings::default(), true);
                let mut actual = Vec::new();
                for chunk in input.chunks(width) {
                    actual.extend(renderer.push(chunk));
                }
                actual.extend(renderer.finish());
                assert_eq!(actual, *expected, "width={width}, input={input:?}");
            }
        }

        let dim_input = b"\x9b31;2mreading\x9b22m";
        let dim_expected = b"\x9b31;2m\x1b[1mread\x1b[22m\x1b[2ming\x9b22m";
        assert_eq!(
            super::transform(dim_input, &Settings::default(), true),
            dim_expected
        );

        let ordinary_continuation = b"\xc3\x9breading!";
        let output = super::transform(ordinary_continuation, &Settings::default(), true);
        assert!(output.windows(2).any(|window| window == b"\xc3\x9b"));
        assert_eq!(strip_emphasis(&output), ordinary_continuation);

        for (prefix, suffix, expected) in [
            (&b"\x9b31"[..], &b"mreading\x9b0m"[..], cases[0].1),
            (&b"\xc2\x9b31"[..], &b"mreading\xc2\x9b0m"[..], cases[1].1),
        ] {
            let mut renderer = Transformer::new(&Settings::default(), true);
            let mut streamed = renderer.push(prefix);
            streamed.extend(renderer.flush_pending());
            streamed.extend(renderer.push(suffix));
            streamed.extend(renderer.finish());
            assert_eq!(streamed, expected);
        }
    }

    #[test]
    fn c1_osc8_is_metadata_and_preserves_hyperlink_payload() {
        let input = b"\x9d8;;https://example.test\x9creading\x9d8;;\x9c";
        let expected = b"\x9d8;;https://example.test\x9c\x1b[1mread\x1b[22ming\x9d8;;\x9c";
        assert_eq!(
            super::transform(input, &Settings::default(), true),
            expected
        );
        for width in [1, 2, 3, 7, input.len()] {
            let mut renderer = Transformer::new(&Settings::default(), true);
            let mut actual = Vec::new();
            for chunk in input.chunks(width) {
                actual.extend(renderer.push(chunk));
            }
            actual.extend(renderer.finish());
            assert_eq!(actual, expected, "width={width}");
        }

        let mut renderer = Transformer::new(&Settings::default(), true);
        let mut streamed = renderer.push(b"\x9d8;;https://example.test");
        streamed.extend(renderer.flush_pending());
        streamed.extend(renderer.push(b"\x9creading\x9d8;;\x9c"));
        streamed.extend(renderer.finish());
        assert_eq!(streamed, expected);

        let encoded_input = b"\xc2\x9d8;;https://example.test\xc2\x9creading\xc2\x9d8;;\xc2\x9c";
        let encoded_expected =
            b"\xc2\x9d8;;https://example.test\xc2\x9c\x1b[1mread\x1b[22ming\xc2\x9d8;;\xc2\x9c";
        assert_eq!(
            super::transform(encoded_input, &Settings::default(), true),
            encoded_expected
        );
        for width in [1, 2, 3, 7, encoded_input.len()] {
            let mut renderer = Transformer::new(&Settings::default(), true);
            let mut actual = Vec::new();
            for chunk in encoded_input.chunks(width) {
                actual.extend(renderer.push(chunk));
            }
            actual.extend(renderer.finish());
            assert_eq!(actual, encoded_expected, "width={width}");
        }
    }

    #[test]
    fn oversized_c1_csi_drains_through_its_final_byte() {
        let mut prefix = b"\x9b".to_vec();
        prefix.extend(std::iter::repeat_n(b'1', MAX_PENDING + 17));
        let mut input = prefix.clone();
        input.extend_from_slice(b"m\x9b0mreading!");
        let expected = super::transform(&input, &Settings::default(), true);

        let mut renderer = Transformer::new(&Settings::default(), true);
        let mut actual = Vec::new();
        for chunk in prefix.chunks(4093) {
            actual.extend(renderer.push(chunk));
        }
        actual.extend(renderer.push(b"m\x9b0mreading!"));
        actual.extend(renderer.finish());
        assert_eq!(actual, expected);
    }

    #[test]
    fn incomplete_escape_after_timeout_is_drained_before_resuming() {
        let mut transformer = Transformer::new(&Settings::default(), true);
        assert!(transformer.push(b"\x1b").is_empty());
        assert_eq!(transformer.flush_pending(), b"\x1b");
        assert_eq!(
            transformer.push(b"[31mreading!"),
            b"[31m\x1b[1mread\x1b[22ming!"
        );
    }

    #[test]
    fn timed_out_sgr_is_parsed_before_resuming_emphasis() {
        let mut transformer = Transformer::new(&Settings::default(), true);
        assert!(transformer.push(b"\x1b[31").is_empty());
        assert_eq!(transformer.flush_pending(), b"\x1b[31");
        assert_eq!(transformer.push(b"mreading!"), b"m\x1b[1mread\x1b[22ming!");
    }

    #[test]
    fn repeated_one_byte_chunks_preserve_a_long_word() {
        let mut input = vec![b'a'; 4096];
        input.push(b' ');
        let mut expected = b"\x1b[1m".to_vec();
        expected.extend(std::iter::repeat_n(b'a', 2048));
        expected.extend_from_slice(b"\x1b[22m");
        expected.extend(std::iter::repeat_n(b'a', 2048));
        expected.push(b' ');

        let mut transformer = Transformer::new(&Settings::default(), true);
        let mut output = Vec::new();
        for byte in &input {
            output.extend(transformer.push(std::slice::from_ref(byte)));
        }
        output.extend(transformer.finish());
        assert_eq!(output, expected);
    }
    #[test]
    fn all_underline_styles_survive_temporary_bold_emphasis() {
        for code in ["4", "21", "4:1", "4:2", "4:3", "4:4", "4:5", "4:9"] {
            let source = format!("\x1b[{code}mreading\x1b[0m");
            let settings = crate::settings::Settings::new();
            let output = super::transform(source.as_bytes(), &settings, true);
            assert_eq!(strip_emphasis(&output), source.as_bytes(), "{code}");
            assert!(output.windows(4).any(|part| part == b"read"));
        }
    }

    #[test]
    fn invalid_and_extended_sgr_parameters_never_corrupt_following_text() {
        for parameters in [
            "?",
            "999999",
            "38",
            "38;3",
            "38;5",
            "38;2;0",
            "4:",
            "4:999999",
            ";;",
            "38:2::1:2:3",
            "48;5;10",
            "58;2;1;2;3",
        ] {
            let source = format!("\x1b[{parameters}mreading\x1b[0m next");
            let output = super::transform(source.as_bytes(), &Settings::new(), true);
            let mut filter = crate::document::ControlFilter::plain_text();
            let mut visible = filter.push(&output);
            visible.extend(filter.finish());
            assert_eq!(visible, b"reading next", "{parameters}");
            let mut renderer = super::Transformer::new(&Settings::new(), true);
            let mut chunked = Vec::new();
            for byte in source.as_bytes() {
                chunked.extend(renderer.push(std::slice::from_ref(byte)));
            }
            chunked.extend(renderer.finish());
            assert_eq!(chunked, output, "{parameters}");
        }
    }

    #[test]
    fn timed_out_and_oversized_string_controls_preserve_bytes_and_resume_at_terminators() {
        for prefix in [b"\x1b]".as_slice(), b"\x1bP", b"\x1bX", b"\x1b^", b"\x1b_"] {
            let mut renderer = super::Transformer::new(&Settings::new(), true);
            let mut source = prefix.to_vec();
            source.extend(std::iter::repeat_n(b'x', super::MAX_PENDING + 5));
            source.extend_from_slice(b"\x1b\\\x1b[0mreading");
            let expected = super::transform(&source, &Settings::new(), true);
            let mut output = Vec::new();
            for chunk in source.chunks(127) {
                output.extend(renderer.push(chunk));
            }
            output.extend(renderer.finish());
            assert_eq!(output, expected, "{prefix:?}");
            assert!(output.starts_with(prefix));
            let mut renderer = super::Transformer::new(&Settings::new(), true);
            let mut output = renderer.push(prefix);
            output.extend(renderer.flush_pending());
            output.extend(renderer.push(b"payload\x1b"));
            output.extend(renderer.push(b"\\\x1b[0mreading"));
            output.extend(renderer.finish());
            let mut original = prefix.to_vec();
            original.extend_from_slice(b"payload\x1b\\\x1b[0mreading");
            assert_eq!(strip_emphasis(&output), original);
        }
    }

    #[test]
    fn bytewise_incomplete_control_scanning_is_linear() {
        let mut osc = b"\x1b]0;".to_vec();
        osc.extend(std::iter::repeat_n(b'x', 2048));
        let mut dcs = b"\x1bP".to_vec();
        dcs.extend(std::iter::repeat_n(b'x', 2048));
        let mut csi = b"\x1b[".to_vec();
        csi.extend(std::iter::repeat_n(b'1', 2048));

        for input in [&osc, &dcs, &csi] {
            assert_bytewise_scan_work_is_linear(input, &Settings::default());
        }
    }

    #[test]
    fn bytewise_unicode_prefix_scanning_is_linear_and_frequency_stable() {
        let settings = Settings {
            every: 2,
            min_word_length: 1,
            ..Settings::default()
        };
        let han = format!("{} alpha beta gamma", "界".repeat(512));
        assert_bytewise_scan_work_is_linear(han.as_bytes(), &settings);

        let combining = format!("a{} alpha beta gamma", "\u{301}".repeat(512));
        assert_bytewise_scan_work_is_linear(combining.as_bytes(), &settings);
    }

    #[test]
    fn malformed_utf8_after_long_combining_runs_is_chunk_invariant_and_linear() {
        let settings = Settings {
            every: 7,
            min_word_length: 1,
            fraction: 1.0,
            ..Settings::default()
        };
        let mut input = Vec::with_capacity(70_007);
        input.extend_from_slice(b"\0a");
        for _ in 0..1_327 {
            input.extend_from_slice(b"\xcc\x81");
        }
        input.push(0xcc);
        for _ in 0..22_210 {
            input.extend_from_slice(b"\xcc\x81");
        }
        input.push(0xcc);
        for _ in 0..11_462 {
            input.extend_from_slice(b"\xcc\x81");
        }
        input.extend_from_slice(b" next");
        assert_eq!(input.len(), 70_007);

        assert_bytewise_scan_work_is_linear(&input, &settings);
        let expected = super::transform(&input, &settings, true);
        assert!(
            expected.starts_with(b"\0\x1b[1ma"),
            "the first word must be resolved before the malformed lead run"
        );
        for width in [2, 3, 7, 8192, input.len()] {
            let mut renderer = Transformer::new(&settings, true);
            let mut actual = Vec::new();
            for chunk in input.chunks(width) {
                actual.extend(renderer.push(chunk));
            }
            actual.extend(renderer.finish());
            assert_eq!(actual, expected, "width={width}");
        }
    }

    #[test]
    fn repeated_malformed_utf8_leads_are_chunk_invariant_and_linear() {
        let settings = Settings {
            every: 7,
            min_word_length: 1,
            fraction: 1.0,
            ..Settings::default()
        };
        let mut input = vec![b'a'];
        input.extend(std::iter::repeat_n(0xcc, 32_768));

        assert_bytewise_scan_work_is_linear(&input, &settings);
    }

    #[test]
    fn raw_continuation_runs_are_emitted_without_resegmenting_the_suffix() {
        let settings = Settings {
            every: 7,
            min_word_length: 1,
            fraction: 1.0,
            ..Settings::default()
        };
        let mut input = vec![0, b'a'];
        input.extend(std::iter::repeat_n(0xcc, 9_875));
        input.extend(std::iter::repeat_n(0x8b, 52));
        input.extend(std::iter::repeat_n(0xcc, 22_893));
        assert_eq!(input.len(), 32_822);

        assert_bytewise_scan_work_is_linear(&input, &settings);
        let expected = super::transform(&input, &settings, true);
        assert!(expected.starts_with(b"\0\x1b[1ma"));
        for width in [2, 3, 7, 127, input.len()] {
            let mut renderer = Transformer::new(&settings, true);
            let mut actual = Vec::new();
            for chunk in input.chunks(width) {
                actual.extend(renderer.push(chunk));
            }
            actual.extend(renderer.finish());
            assert_eq!(actual, expected, "width={width}");
        }
    }

    #[test]
    fn neighboring_raw_c1_bytes_do_not_split_valid_utf8_scalars_across_chunks() {
        let settings = Settings {
            min_word_length: 1,
            fraction: 1.0,
            ..Settings::default()
        };
        let inputs: [&[u8]; 2] = [
            b"a\xcc\x8b\x8a\x8b\x8c\xcc\x8b next",
            b"a\xc2\x8b\xcc\x8b\x8a\x8b\x8c next",
        ];
        for input in inputs {
            let expected = super::transform(input, &settings, true);
            for width in [1, 2, 3, 7, input.len()] {
                let mut renderer = Transformer::new(&settings, true);
                let mut actual = Vec::new();
                for chunk in input.chunks(width) {
                    actual.extend(renderer.push(chunk));
                }
                actual.extend(renderer.finish());
                assert_eq!(actual, expected, "width={width}, input={input:?}");
            }
        }
    }

    #[test]
    fn oversized_combining_cluster_keeps_following_frequency_chunk_invariant() {
        let settings = Settings {
            every: 2,
            min_word_length: 1,
            ..Settings::default()
        };
        let input = format!("a{} beta gamma delta", "\u{301}".repeat(MAX_PENDING / 2));
        let expected = super::transform(input.as_bytes(), &settings, true);
        let mut renderer = Transformer::new(&settings, true);
        let mut actual = Vec::new();
        for byte in input.as_bytes() {
            actual.extend(renderer.push(std::slice::from_ref(byte)));
        }
        actual.extend(renderer.finish());
        assert_eq!(actual, expected);
    }

    #[test]
    fn malformed_utf8_before_thai_is_chunk_invariant() {
        let input = [
            0x02, 0xa0, 0xe0, 0xb8, 0xb6, 0xe0, 0xb8, 0xa9, 0xe0, 0xb8, 0xb2, 0xe0, 0xb9, 0x84,
            0xe0, 0xb8, 0x91, 0x3c, 0xe2, 0x80, 0x8d, 0xf0, 0x9f, 0xa0, 0xe5, 0xa5, 0xbd, 0xe4,
            0x38, 0x96, 0xe7, 0x95, 0x8c, 0x20, 0xe6, 0x97, 0xa5, 0xe6, 0x68, 0xac, 0xe8, 0xaa,
            0x9e, 0x20, 0xe0, 0xb8, 0xa0, 0xe0, 0xb8, 0xb2, 0xe0, 0xb8, 0xa9, 0xe0, 0xb8, 0xb2,
            0xe0, 0xb9, 0x84, 0xe0, 0xb8, 0x91, 0x3c, 0xe2, 0x80, 0x8d, 0xf0, 0x9f, 0x91, 0xa6,
        ];
        let settings = Settings {
            emphasis: Emphasis::Reverse,
            ..Settings::default()
        };
        let expected = super::transform(&input, &settings, true);
        for width in [1, 2, 3, 7, input.len()] {
            let mut renderer = Transformer::new(&settings, true);
            let mut actual = Vec::new();
            for chunk in input.chunks(width) {
                actual.extend(renderer.push(chunk));
            }
            actual.extend(renderer.finish());
            assert_eq!(actual, expected, "width={width}");
        }
    }

    #[test]
    fn oversized_mixed_script_runs_do_not_resegment_the_suffix_on_every_read() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        #[derive(Debug)]
        struct CountedSegmenter {
            inner: crate::segmentation::UnicodeSegmenter,
            calls: Arc<AtomicUsize>,
        }
        impl WordSegmenter for CountedSegmenter {
            fn word_ranges(&self, text: &str) -> Vec<Range<usize>> {
                self.calls.fetch_add(1, Ordering::Relaxed);
                self.inner.word_ranges(text)
            }
            fn terminal_trailing_word_start(&self, text: &str) -> Option<usize> {
                self.inner.terminal_trailing_word_start(text)
            }
        }
        // Minimized structure of the Linux sanitizer timeout (70,005 bytes).
        let source = format!("{}ฟ{} next", "a".repeat(10_920), "a".repeat(59_077));
        for every in [1, 2] {
            let settings = Settings {
                every,
                ..Settings::new()
            };
            let expected = super::transform(source.as_bytes(), &settings, true);
            for width in [1, 35, 8192] {
                let calls = Arc::new(AtomicUsize::new(0));
                let segmenter = CountedSegmenter {
                    inner: crate::segmentation::UnicodeSegmenter::new(),
                    calls: Arc::clone(&calls),
                };
                let mut renderer =
                    super::Transformer::new_with_segmenter(&settings, true, Box::new(segmenter));
                let mut output = Vec::new();
                for chunk in source.as_bytes().chunks(width) {
                    output.extend(renderer.push(chunk));
                    assert!(
                        calls.load(Ordering::Relaxed) <= 32,
                        "segmentation work must be bounded independently of read size"
                    );
                }
                output.extend(renderer.finish());
                assert_eq!(output, expected, "every={every}, width={width}");
            }
        }
    }
}

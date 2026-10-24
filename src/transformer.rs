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
use crate::segmentation::{FixationEngine, UnicodeSegmenter, WordSegmenter, is_word_connector};
use crate::settings::{Emphasis, Settings};

const MAX_PENDING: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ControlKind {
    Sgr,
    Metadata,
    Other,
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
    Osc { previous_escape: bool },
    String { previous_escape: bool },
    Escape { intermediate: bool },
}

#[derive(Debug)]
struct DrainingControl {
    bytes: Vec<u8>,
    mode: DrainMode,
    tracking: bool,
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
    draining_control: Option<DrainingControl>,
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
            draining_control: None,
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
            self.pending.extend_from_slice(appended);
            remaining = &remaining[count..];

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

            if !self.pending_word || !chunk_can_extend_word(appended) {
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
            return Vec::new();
        }

        let mut output = Vec::new();
        if self.draining_control.is_some() {
            // The partial escape was already written by flush_pending. There are no
            // buffered bytes to recover, and its terminal state is uncertain.
            self.draining_control = None;
            self.style.known = false;
        }

        if self.passthrough_word {
            let (tokens, _) = scan_tokens(&self.pending, /* eof */ false);
            let (visible, _) = visible_text(&tokens);
            self.advance_passthrough_text(&visible, true);
            output.append(&mut self.pending);
            self.passthrough_word = false;
            self.pending_word = false;
            self.passthrough_context.clear();
            self.passthrough_collapsed_letters = 0;
            return output;
        }

        if !self.pending.is_empty() {
            let pending = mem::take(&mut self.pending);
            let (tokens, _) = scan_tokens(&pending, /* eof */ true);
            output.extend(self.render(&pending, &tokens));
        }
        self.pending_word = false;
        output
    }

    #[must_use]
    fn process_available(&mut self) -> Vec<u8> {
        if self.pending.is_empty() {
            return Vec::new();
        }

        let (tokens, parse) = scan_tokens(&self.pending, /* eof */ false);
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
            return Vec::new();
        }
        let source = self.pending[..cut].to_vec();
        let output = self.render(&source, &tokens[..safe_token_count]);
        self.pending.drain(..cut);
        output
    }

    #[must_use]
    fn fallback_pending(&mut self) -> Vec<u8> {
        if self.pending.is_empty() {
            return Vec::new();
        }

        let source = mem::take(&mut self.pending);
        let (tokens, parse) = scan_tokens(&source, /* eof */ false);
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
            if tail.first() == Some(&0x1b) {
                let drain = DrainingControl::new(tail);
                if !drain.tracking {
                    self.style.known = false;
                }
                self.draining_control = Some(drain);
            }
        }

        source
    }

    #[must_use]
    fn release_passthrough_pending(&mut self) -> Vec<u8> {
        if self.pending.is_empty() {
            return Vec::new();
        }

        let source = mem::take(&mut self.pending);
        let (tokens, parse) = scan_tokens(&source, /* eof */ false);
        self.update_style_from_tokens(&source[..parse.complete_len], &tokens);
        let mut output = source[..parse.complete_len].to_vec();
        if let Some(start) = parse.incomplete_at {
            let tail = &source[start..];
            if tail.first() == Some(&0x1b) {
                let drain = DrainingControl::new(tail);
                if !drain.tracking {
                    self.style.known = false;
                }
                self.draining_control = Some(drain);
            }
            output.extend_from_slice(tail);
        }
        output
    }

    fn advance_passthrough_text(&mut self, appended: &str, final_boundary: bool) {
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
        self.passthrough_collapsed_letters = self
            .passthrough_collapsed_letters
            .saturating_add(letter_grapheme_count(&self.passthrough_context));
        self.passthrough_context.clear();
    }

    #[must_use]
    fn passthrough_until_delimiter(&mut self) -> (Vec<u8>, bool) {
        let (tokens, parse) = scan_tokens(&self.pending, /* eof */ false);
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
            self.passthrough_word = false;
            return (raw, true);
        }

        let cut = parse.complete_len;
        let source = self.pending[..cut].to_vec();
        let (visible, _) = visible_text(&tokens);
        self.advance_passthrough_text(&visible, false);
        self.update_style_from_tokens(&source, &tokens);
        let raw: Vec<u8> = self.pending.drain(..cut).collect();
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
    let mut tokens = Vec::new();
    let mut offset = 0;
    while offset < source.len() {
        if source[offset] == 0x1b {
            match parse_escape(source, offset) {
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
        b'[' => {
            let mut index = start + 2;
            while let Some(byte) = source.get(index).copied() {
                if (0x40..=0x7e).contains(&byte) {
                    let end = index + 1;
                    let kind = if end - start <= MAX_PENDING
                        && byte == b'm'
                        && sgr_params(&source[start..end]).is_some()
                    {
                        ControlKind::Sgr
                    } else if is_presentation_neutral_control(&source[start..end]) {
                        ControlKind::Metadata
                    } else {
                        ControlKind::Other
                    };
                    return Some((end, kind));
                }
                index += 1;
                if index - start > MAX_PENDING {
                    // Keep scanning to the terminator, but the sequence itself is
                    // always passthrough because its state cannot be reconstructed.
                }
            }
            None
        }
        b']' => {
            let mut index = start + 2;
            let mut terminator = None;
            while index < source.len() {
                if source[index] == 0x07 {
                    terminator = Some(index + 1);
                    break;
                }
                if source[index] == 0x1b && source.get(index + 1) == Some(&b'\\') {
                    terminator = Some(index + 2);
                    break;
                }
                index += 1;
            }
            let end = terminator?;
            let payload_end = if source[end - 1] == 0x07 {
                end - 1
            } else {
                end - 2
            };
            let payload = &source[start + 2..payload_end];
            let kind = if is_safe_osc(payload) {
                ControlKind::Metadata
            } else {
                ControlKind::Other
            };
            Some((end, kind))
        }
        b'P' | b'^' | b'_' | b'X' => {
            let mut index = start + 2;
            while index + 1 < source.len() {
                if source[index] == 0x1b && source[index + 1] == b'\\' {
                    return Some((index + 2, ControlKind::Other));
                }
                index += 1;
            }
            None
        }
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
    matches!(sequence, b"\x1b[?2004h" | b"\x1b[?2004l" | b"\x1b(B")
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
    if sequence.len() < 3 || sequence[0] != 0x1b || sequence[1] != b'[' || *sequence.last()? != b'm'
    {
        return None;
    }
    let parameter_bytes = &sequence[2..sequence.len() - 1];
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
    if value.chars().any(char::is_alphabetic) {
        return GraphemeKind::Letter;
    }
    if value.chars().any(char::is_numeric) {
        return GraphemeKind::Number;
    }
    GraphemeKind::Other
}

#[must_use]
fn trailing_candidate_start(units: &[Grapheme]) -> Option<usize> {
    let last = units.len().checked_sub(1)?;
    let is_word_unit = |kind| matches!(kind, GraphemeKind::Letter | GraphemeKind::Number);
    let is_connector = |kind| matches!(kind, GraphemeKind::Apostrophe | GraphemeKind::Connector);
    if !is_word_unit(units[last].kind) && !is_connector(units[last].kind) {
        return None;
    }

    // Keep the whole contiguous possible word, including a trailing or
    // leading connector run: ICU can resolve those connectors only after a
    // neighboring word character arrives (for example, ExtendNumLet).
    let mut cursor = last;
    while cursor > 0
        && (is_word_unit(units[cursor - 1].kind) || is_connector(units[cursor - 1].kind))
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
        .filter(|grapheme| grapheme.chars().any(char::is_alphabetic))
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

impl DrainingControl {
    #[must_use]
    fn new(bytes: &[u8]) -> Self {
        let mode = if bytes == b"\x1b" {
            DrainMode::EscapeIntro
        } else if bytes.starts_with(b"\x1b[") {
            DrainMode::Csi
        } else if bytes.starts_with(b"\x1b]") {
            DrainMode::Osc {
                previous_escape: bytes.last() == Some(&0x1b),
            }
        } else if bytes.starts_with(b"\x1bP")
            || bytes.starts_with(b"\x1b^")
            || bytes.starts_with(b"\x1b_")
            || bytes.starts_with(b"\x1bX")
        {
            DrainMode::String {
                previous_escape: bytes.last() == Some(&0x1b),
            }
        } else {
            DrainMode::Escape {
                intermediate: bytes
                    .iter()
                    .skip(1)
                    .any(|byte| (0x20..=0x2f).contains(byte)),
            }
        };
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
                };
                false
            }
            b'P' | b'^' | b'_' | b'X' => {
                *mode = DrainMode::String {
                    previous_escape: false,
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
        DrainMode::Osc { previous_escape } => {
            let complete = (*previous_escape && byte == b'\\') || byte == 0x07;
            *previous_escape = byte == 0x1b;
            complete
        }
        DrainMode::String { previous_escape } => {
            let complete = *previous_escape && byte == b'\\';
            *previous_escape = byte == 0x1b;
            complete
        }
        DrainMode::Escape { intermediate } => {
            if *intermediate {
                (0x30..=0x7e).contains(&byte)
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
}

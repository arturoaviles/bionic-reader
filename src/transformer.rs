//! Incremental terminal text transformation for bionic reading.
//!
//! [`Transformer`] buffers incomplete words and terminal controls across input
//! chunks. Use [`transform`] when the full input is already available.

use std::collections::BTreeMap;
use std::mem;
use std::str;

use unicode_script::Script;

use crate::settings::Settings;

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
struct StyleState {
    bold: bool,
    dim: bool,
    known: bool,
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
                    self.known = true;
                }
                1 if self.known => self.bold = true,
                2 if self.known => self.dim = true,
                22 => {
                    self.bold = false;
                    self.dim = false;
                    self.known = true;
                }
                _ => {}
            }
            index += 1;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GraphemeKind {
    LatinLetter,
    Apostrophe,
    Mark,
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

#[derive(Clone, Copy, Debug)]
struct EmphasisSettings {
    fraction: f64,
    every: usize,
    min_word_length: usize,
}

/// Transforms terminal output while preserving stream boundaries and controls.
///
/// Original text bytes and terminal controls retain their order; only emphasis
/// escape sequences are inserted. Disabled transformation returns the original bytes.
///
/// A transformer retains incomplete words and terminal sequences between calls
/// to [`push`](Self::push). Call [`finish`](Self::finish) when the stream ends so
/// any remaining complete word can be emitted. Settings supplied to [`new`](Self::new)
/// must pass [`Settings::validate`] before the transformer is used.
#[derive(Debug)]
#[must_use = "use the transformer to process terminal output"]
pub struct Transformer {
    settings: EmphasisSettings,
    enabled: bool,
    pending: Vec<u8>,
    pending_word: bool,
    style: StyleState,
    eligible_seen: usize,
    passthrough_word: bool,
    passthrough_word_letters: usize,
    passthrough_apostrophe: bool,
    draining_control: Option<DrainingControl>,
}

impl Transformer {
    /// Creates a transformer with a snapshot of the supplied emphasis settings.
    ///
    /// `enabled` controls whether this transformer adds bionic reading emphasis.
    /// Later configuration changes do not affect this stream. Session exclusions
    /// and the style policy are resolved by the caller and are not retained here.
    /// Settings are not validated here; call [`Settings::validate`] before use.
    pub fn new(settings: &Settings, enabled: bool) -> Self {
        Self {
            settings: EmphasisSettings {
                fraction: settings.fraction,
                every: settings.every,
                min_word_length: settings.min_word_length,
            },
            enabled,
            pending: Vec::new(),
            pending_word: false,
            style: StyleState::initial(),
            eligible_seen: 0,
            passthrough_word: false,
            passthrough_word_letters: 0,
            passthrough_apostrophe: false,
            draining_control: None,
        }
    }

    /// Processes an input chunk and returns bytes that are ready to emit.
    ///
    /// Incomplete words and terminal sequences remain buffered until more input
    /// arrives or the stream is flushed or finished.
    ///
    /// # Panics
    ///
    /// Panics if transformation is enabled, the configured `every` value is zero,
    /// and the input contains an eligible word. Call [`Settings::validate`] before
    /// using this transformer. Malformed UTF-8 and incomplete terminal sequences
    /// do not cause a panic.
    #[must_use = "write or buffer the processed output"]
    pub fn push(&mut self, input: &[u8]) -> Vec<u8> {
        if !self.enabled {
            return input.to_vec();
        }

        let mut output = Vec::new();
        let mut remaining = input;

        if self.draining_control.is_some() {
            let (bytes, rest) = self.drain_control(remaining);
            output.extend_from_slice(bytes);
            remaining = rest;
            if remaining.is_empty() {
                return output;
            }
        }

        if self.passthrough_word {
            self.pending.extend_from_slice(remaining);
            let (raw, resume) = self.passthrough_until_delimiter();
            output.extend(raw);
            if resume {
                output.extend(self.process_available());
            }
            return output;
        }

        if self.pending_word {
            self.pending.extend_from_slice(remaining);
            if !chunk_can_extend_word(remaining) || self.pending.len() > MAX_PENDING {
                output.extend(self.process_available());
            }
        } else {
            self.pending.extend_from_slice(remaining);
            output.extend(self.process_available());
        }

        if self.pending.len() > MAX_PENDING {
            output.extend(self.fallback_pending());
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
    /// # Panics
    ///
    /// Panics if transformation is enabled, the configured `every` value is zero,
    /// and an eligible word is selected. Call [`Settings::validate`] before using
    /// this transformer. Malformed UTF-8 and incomplete terminal sequences do not
    /// cause a panic.
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
            output.append(&mut self.pending);
            self.passthrough_word = false;
            self.pending_word = false;
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
        let candidate = trailing_candidate_start(&graphemes);
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
        if let Some(candidate) = trailing_candidate_start(&graphemes) {
            self.passthrough_word = true;
            self.passthrough_word_letters = graphemes
                .iter()
                .filter(|unit| unit.start_token >= candidate)
                .filter(|unit| unit.kind == GraphemeKind::LatinLetter)
                .count();
            self.passthrough_apostrophe = graphemes
                .last()
                .is_some_and(|unit| unit.kind == GraphemeKind::Apostrophe);
        } else {
            self.passthrough_word = false;
            self.passthrough_word_letters = 0;
            self.passthrough_apostrophe = false;
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
    fn passthrough_until_delimiter(&mut self) -> (Vec<u8>, bool) {
        let (tokens, parse) = scan_tokens(&self.pending, /* eof */ false);
        let units = graphemes(&tokens);
        let mut letters = 0_usize;
        let delimiter_index = units.iter().position(|unit| match unit.kind {
            GraphemeKind::LatinLetter => {
                letters = letters.saturating_add(1);
                self.passthrough_apostrophe = false;
                false
            }
            GraphemeKind::Apostrophe if !self.passthrough_apostrophe => {
                self.passthrough_apostrophe = true;
                false
            }
            GraphemeKind::Mark if !self.passthrough_apostrophe => false,
            _ => true,
        });
        self.passthrough_word_letters = self.passthrough_word_letters.saturating_add(letters);

        if let Some(delimiter_index) = delimiter_index {
            if self.passthrough_word_letters >= self.settings.min_word_length {
                self.eligible_seen = self.eligible_seen.saturating_add(1);
            }
            self.passthrough_word_letters = 0;
            self.passthrough_apostrophe = false;
            let delimiter = &units[delimiter_index];
            let cut = tokens[delimiter.end_token - 1].end;
            let source = self.pending[..cut].to_vec();
            self.update_style_from_tokens(&source, &tokens[..delimiter.end_token]);
            let raw: Vec<u8> = self.pending.drain(..cut).collect();
            self.passthrough_word = false;
            return (raw, true);
        }

        let cut = parse.complete_len;
        let source = self.pending[..cut].to_vec();
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
        let starts = select_emphasis(tokens, &self.settings, &mut self.eligible_seen);
        render_ansi(source, tokens, &starts, &mut self.style)
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

// Word selection is independent of ANSI emission and style restoration.
#[must_use]
fn select_emphasis(
    tokens: &[Token],
    settings: &EmphasisSettings,
    eligible_seen: &mut usize,
) -> BTreeMap<usize, usize> {
    let units = graphemes(tokens);
    let mut starts: BTreeMap<usize, usize> = BTreeMap::new();
    let mut cursor = 0;
    while cursor < units.len() {
        if units[cursor].kind != GraphemeKind::LatinLetter {
            cursor += 1;
            continue;
        }

        let first = cursor;
        let mut letters = vec![cursor];
        cursor += 1;
        while cursor < units.len() {
            if units[cursor].kind == GraphemeKind::LatinLetter {
                letters.push(cursor);
                cursor += 1;
            } else if units[cursor].kind == GraphemeKind::Apostrophe
                && cursor + 1 < units.len()
                && units[cursor + 1].kind == GraphemeKind::LatinLetter
            {
                cursor += 1;
                letters.push(cursor);
                cursor += 1;
            } else {
                break;
            }
        }

        if letters.len() < settings.min_word_length {
            continue;
        }

        *eligible_seen = eligible_seen.saturating_add(1);
        let selected = (*eligible_seen - 1) % settings.every == 0;
        if !selected || settings.fraction <= 0.0 {
            continue;
        }

        let raw_start = tokens[units[first].start_token].start;
        let last_letter = *letters.last().expect("word has a letter");
        let raw_end = tokens[units[last_letter].end_token - 1].end;
        let unsafe_control = tokens[units[first].start_token..units[last_letter].end_token]
            .iter()
            .any(|token| matches!(token.kind, TokenKind::Control(ControlKind::Other)));
        if raw_end.saturating_sub(raw_start) > MAX_PENDING || unsafe_control {
            continue;
        }

        let emphasis_length = (settings.fraction * letters.len() as f64).ceil() as usize;
        if emphasis_length == 0 {
            continue;
        }
        let prefix_end = units[letters[emphasis_length.min(letters.len()) - 1]].end_token;
        starts.insert(units[first].start_token, prefix_end);
    }

    starts
}

#[must_use]
fn render_ansi(
    source: &[u8],
    tokens: &[Token],
    starts: &BTreeMap<usize, usize>,
    style: &mut StyleState,
) -> Vec<u8> {
    let mut output = Vec::with_capacity(source.len() + tokens.len() * 8);
    let mut emphasis_end = None;
    let mut emphasizing = false;
    let indexed_tokens = tokens.iter().enumerate();
    for (index, token) in indexed_tokens {
        if let Some(event) = starts.get(&index).filter(|_| style.known) {
            output.extend_from_slice(b"\x1b[1m");
            emphasizing = true;
            emphasis_end = Some(*event);
        }

        output.extend_from_slice(&source[token.start..token.end]);
        if let TokenKind::Control(ControlKind::Sgr) = token.kind {
            let resets_intensity = sgr_resets_intensity(&source[token.start..token.end]);
            style.apply_sgr(&source[token.start..token.end]);
            if emphasizing && resets_intensity {
                output.extend_from_slice(b"\x1b[1m");
            }
        } else if matches!(token.kind, TokenKind::Control(ControlKind::Other)) {
            style.known = false;
        }

        if emphasizing && emphasis_end == Some(index + 1) {
            restore_bold_dim(&mut output, *style);
            emphasizing = false;
            emphasis_end = None;
        }
    }
    output
}

/// Transforms a complete byte slice using bionic reading emphasis.
///
/// Original text bytes and terminal controls are preserved in order, with only
/// emphasis escape sequences inserted.
/// When `enabled` is false, the input is returned unchanged.
///
/// # Panics
///
/// Panics if `enabled` is true, `settings.every` is zero, and the input contains
/// an eligible word.
/// Call [`Settings::validate`] before transforming input. Malformed UTF-8 and
/// incomplete terminal sequences do not cause a panic; those bytes are preserved.
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

        match str::from_utf8(&source[offset..offset + width]) {
            Ok(text) => {
                let character = text.chars().next().expect("one valid UTF-8 character");
                tokens.push(Token {
                    start: offset,
                    end: offset + width,
                    kind: TokenKind::Text(character),
                });
                offset += width;
            }
            Err(_) => {
                tokens.push(Token {
                    start: offset,
                    end: offset + 1,
                    kind: TokenKind::Raw,
                });
                offset += 1;
            }
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
        }]);
    }
    let mut params = Vec::new();
    let semicolon_separated_parameters = parameter_bytes.split(|byte| *byte == b';');
    for part in semicolon_separated_parameters {
        let code_part = part.split(|byte| *byte == b':').next().unwrap_or_default();
        if code_part.is_empty() {
            params.push(SgrParam {
                code: 0,
                colon_form: false,
            });
        } else {
            let text = str::from_utf8(code_part).ok()?;
            let code: u16 = text.parse().ok()?;
            params.push(SgrParam {
                code,
                colon_form: part.contains(&b':'),
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
fn sgr_resets_intensity(sequence: &[u8]) -> bool {
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
        if !parameter.colon_form && matches!(parameter.code, 0 | 22) {
            return true;
        }
        index += 1;
    }
    false
}

#[must_use]
fn graphemes(tokens: &[Token]) -> Vec<Grapheme> {
    use unicode_segmentation::UnicodeSegmentation;

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
    use unicode_script::UnicodeScript;

    if matches!(value, "'" | "’" | "ʼ") {
        return GraphemeKind::Apostrophe;
    }
    let Some(first) = value.chars().next() else {
        return GraphemeKind::Other;
    };
    if first.is_alphabetic() && first.script() == Script::Latin {
        if value
            .chars()
            .skip(1)
            .all(|character| !character.is_alphabetic() && character.script() == Script::Inherited)
        {
            return GraphemeKind::LatinLetter;
        }
        return GraphemeKind::Other;
    }
    if value
        .chars()
        .all(|character| !character.is_alphabetic() && character.script() == Script::Inherited)
    {
        GraphemeKind::Mark
    } else {
        GraphemeKind::Other
    }
}

#[must_use]
fn trailing_candidate_start(units: &[Grapheme]) -> Option<usize> {
    let last = units.len().checked_sub(1)?;
    let mut cursor = match units[last].kind {
        GraphemeKind::LatinLetter => last,
        GraphemeKind::Apostrophe
            if last > 0 && units[last - 1].kind == GraphemeKind::LatinLetter =>
        {
            last - 1
        }
        _ => return None,
    };

    loop {
        while cursor > 0 && units[cursor - 1].kind == GraphemeKind::LatinLetter {
            cursor -= 1;
        }
        if cursor >= 2
            && units[cursor - 1].kind == GraphemeKind::Apostrophe
            && units[cursor - 2].kind == GraphemeKind::LatinLetter
        {
            cursor -= 2;
        } else {
            break;
        }
    }
    Some(units[cursor].start_token)
}

#[must_use]
fn chunk_can_extend_word(input: &[u8]) -> bool {
    if input
        .iter()
        .all(|byte| byte.is_ascii_alphabetic() || *byte == b'\'')
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
            GraphemeKind::LatinLetter | GraphemeKind::Apostrophe | GraphemeKind::Mark
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
            0x30..=0x7e => true,
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
    use super::{MAX_PENDING, Transformer};
    use crate::settings::Settings;

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
    fn decomposed_latin_marks_count_once_and_other_scripts_stay_unchanged() {
        let input = "cafe\u{301} Καλημέρα 123 😀".as_bytes();
        assert_eq!(
            super::transform(input, &Settings::default(), true),
            "\x1b[1mca\x1b[22mfe\u{301} Καλημέρα 123 😀".as_bytes()
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
}

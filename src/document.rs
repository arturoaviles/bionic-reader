//! Document-format conversion and terminal-control filtering.
//!
//! [`render`] converts one document between plain text, Markdown, ANSI, and safe HTML.
//! [`render_stream`] keeps terminal, plain-text, and Markdown streams incremental. Markdown is
//! parsed one bounded block at a time; a block larger than 1 MiB is passed through unstyled (or
//! escaped as literal text for Markdown and HTML) along with the remainder of that stream.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::ops::Range;
use std::str;

use pulldown_cmark::{BrokenLink, Event, Options, Parser, RefDefs, Tag};

use crate::Error;
use crate::segmentation::{FixationEngine, UnicodeSegmenter, WordSegmenter};
use crate::settings::{ControlPolicy, InputFormat, OutputFormat, Settings};
use crate::transformer::{self, Transformer};

const MAX_CONTROL_BYTES: usize = 8 * 1024;
const MAX_MARKDOWN_SLICE_BYTES: usize = 8 * 1024 * 1024;
const MAX_MARKDOWN_BLOCK_BYTES: usize = 1024 * 1024;
const MAX_MARKDOWN_REFERENCE_BYTES: usize = 1024 * 1024;
const MAX_TEXT_WORD_BYTES: usize = 64 * 1024;
const STREAM_CHUNK_BYTES: usize = 16 * 1024;

#[cfg(test)]
thread_local! {
    static CONTROL_SCAN_STEPS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SequenceKind {
    Csi,
    Osc,
    String,
    Escape,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Utf8Status {
    Complete { length: usize, c1_control: bool },
    Incomplete,
    Invalid,
}

/// Filters terminal control sequences across arbitrary byte-stream boundaries.
///
/// The safe policy retains complete SGR styling sequences and OSC 8 hyperlinks with `http`,
/// `https`, or `mailto` destinations. Other terminal controls, disallowed C0/C1 characters, and
/// incomplete escape sequences at end of stream are discarded. Ordinary bytes, including invalid
/// UTF-8 bytes, are copied unchanged. The preserve policy returns every input byte unchanged.
#[derive(Debug)]
#[must_use = "use the filter to process terminal bytes"]
pub struct ControlFilter {
    policy: ControlPolicy,
    retain_safe_controls: bool,
    pending: Vec<u8>,
    draining: Option<SequenceKind>,
    scan: Option<ControlScan>,
}

#[derive(Clone, Copy, Debug)]
struct ControlScan {
    start: usize,
    next: usize,
}

fn validate_format_combination(settings: &Settings) -> Result<(), Error> {
    if settings.input_format == InputFormat::Markdown
        && settings.format == OutputFormat::Markdown
        && !settings.skip_code
    {
        return Err(Error::invalid_input(
            "Markdown-to-Markdown output cannot safely emphasize code; set skip_code = true or choose ANSI, plain, or HTML output".to_owned(),
        ));
    }
    Ok(())
}

impl ControlFilter {
    /// Creates an incremental control filter for the supplied policy.
    pub fn new(policy: ControlPolicy) -> Self {
        Self {
            policy,
            retain_safe_controls: true,
            pending: Vec::new(),
            draining: None,
            scan: None,
        }
    }

    /// Creates a safe filter that removes every terminal control, including SGR styling and
    /// otherwise-safe OSC 8 hyperlinks.
    ///
    /// Tabs, line breaks, carriage returns, ordinary bytes, and invalid UTF-8 bytes are retained.
    pub fn plain_text() -> Self {
        Self {
            policy: ControlPolicy::Safe,
            retain_safe_controls: false,
            pending: Vec::new(),
            draining: None,
            scan: None,
        }
    }

    /// Processes a byte chunk, retaining an incomplete escape sequence for the next call.
    #[must_use = "write or buffer the filtered bytes"]
    pub fn push(&mut self, input: &[u8]) -> Vec<u8> {
        if self.policy == ControlPolicy::Preserve {
            return input.to_vec();
        }
        self.pending.extend_from_slice(input);
        self.process(false)
    }

    /// Finishes the stream and drops any incomplete escape sequence.
    #[must_use = "write the filtered bytes"]
    pub fn finish(&mut self) -> Vec<u8> {
        if self.policy == ControlPolicy::Preserve {
            self.pending.clear();
            return Vec::new();
        }
        let output = self.process(true);
        self.pending.clear();
        self.draining = None;
        self.scan = None;
        output
    }

    fn process(&mut self, eof: bool) -> Vec<u8> {
        let mut output = Vec::with_capacity(self.pending.len());
        let mut offset = 0;

        if self.resume_draining(&mut offset) {
            return output;
        }

        while let Some(byte) = self.pending.get(offset).copied() {
            if self.process_byte(byte, &mut offset, eof, &mut output) {
                break;
            }
        }

        if offset > 0 {
            self.pending.drain(..offset);
            if let Some(scan) = &mut self.scan {
                if scan.start >= offset {
                    scan.start -= offset;
                    scan.next -= offset;
                } else {
                    self.scan = None;
                }
            }
        }
        output
    }

    fn resume_draining(&mut self, offset: &mut usize) -> bool {
        while let Some(kind) = self.draining {
            if let Some(length) = drain_sequence(&self.pending[*offset..], kind) {
                *offset += length;
                self.draining = None;
                self.scan = None;
            } else {
                // A string terminator or incomplete UTF-8 scalar can straddle
                // calls after an oversized control enters draining mode.
                // Keep only that suffix so completed payload scalars are not
                // reinterpreted as controls on a later push.
                let suffix_length = if matches!(kind, SequenceKind::Osc | SequenceKind::String) {
                    string_drain_suffix_length(&self.pending)
                } else {
                    0
                };
                let suffix_start = self.pending.len() - suffix_length;
                let suffix = self.pending[suffix_start..].to_vec();
                self.pending.clear();
                self.pending.extend_from_slice(&suffix);
                self.scan = None;
                return true;
            }
        }
        false
    }

    fn process_byte(
        &mut self,
        byte: u8,
        offset: &mut usize,
        eof: bool,
        output: &mut Vec<u8>,
    ) -> bool {
        match byte {
            0x9b | 0x9d | 0x90 | 0x98 | 0x9e | 0x9f => {
                self.process_c1(byte, 1, offset, eof, output)
            }
            0x00..=0x08 | 0x0b..=0x0c | 0x0e..=0x1a | 0x1c..=0x1f | 0x7f | 0x80..=0x9f => {
                *offset += 1;
                false
            }
            b'\t' | b'\n' | b'\r' => {
                output.push(byte);
                *offset += 1;
                false
            }
            0x1b => self.process_escape(*offset, offset, eof, output),
            0xc2 => self.process_utf8_c2(*offset, offset, eof, output),
            value if value >= 0xc2 => self.process_utf8(value, *offset, offset, eof, output),
            _ => {
                output.push(byte);
                *offset += 1;
                false
            }
        }
    }

    fn process_escape(
        &mut self,
        start: usize,
        offset: &mut usize,
        eof: bool,
        output: &mut Vec<u8>,
    ) -> bool {
        if start + 1 >= self.pending.len() {
            if eof {
                *offset += 1;
            }
            return true;
        }

        let Some(introducer) = self.pending.get(start + 1).copied() else {
            if eof {
                *offset += 1;
            }
            return true;
        };
        let parsed = match introducer {
            b'[' => self.parse_csi(start),
            b']' => self.parse_string_control(start, SequenceKind::Osc),
            b'P' | b'X' | b'^' | b'_' => self.parse_string_control(start, SequenceKind::String),
            _ => self.parse_escape_sequence(start),
        };
        self.apply_control(start, parsed, eof, output, offset)
    }

    fn process_c1(
        &mut self,
        control: u8,
        prefix_length: usize,
        offset: &mut usize,
        eof: bool,
        output: &mut Vec<u8>,
    ) -> bool {
        if let Some(kind) = c1_sequence_kind(control) {
            let start = *offset;
            let parsed = self.parse_c1_control(start, prefix_length, kind);
            return self.apply_control(start, parsed, eof, output, offset);
        }
        *offset += prefix_length;
        false
    }

    fn process_utf8_c2(
        &mut self,
        start: usize,
        offset: &mut usize,
        eof: bool,
        output: &mut Vec<u8>,
    ) -> bool {
        if let Some(control) = self
            .pending
            .get(start + 1)
            .copied()
            .filter(|byte| (0x80..=0x9f).contains(byte))
        {
            return self.process_c1(control, 2, offset, eof, output);
        }
        self.process_utf8(b'\xc2', start, offset, eof, output)
    }

    fn process_utf8(
        &mut self,
        first: u8,
        start: usize,
        offset: &mut usize,
        eof: bool,
        output: &mut Vec<u8>,
    ) -> bool {
        match inspect_utf8_control(&self.pending[start..]) {
            Utf8Status::Complete { length, c1_control } => {
                if !c1_control {
                    output.extend_from_slice(&self.pending[start..start + length]);
                }
                *offset += length;
                false
            }
            Utf8Status::Incomplete if !eof => true,
            Utf8Status::Incomplete | Utf8Status::Invalid => {
                output.push(first);
                *offset += 1;
                false
            }
        }
    }

    fn parse_csi(&self, start: usize) -> ParsedControl {
        let body_start = start + 2;
        let scan_start = self.scan_start(start, body_start);
        let rest = &self.pending[scan_start..];
        Self::record_scan_steps(rest.len());
        let Some(relative_end) = rest.iter().position(|byte| (0x40..=0x7e).contains(byte)) else {
            return ParsedControl::Incomplete {
                kind: SequenceKind::Csi,
                next_scan: self.pending.len(),
            };
        };
        let end = scan_start + relative_end + 1;
        let bytes = &self.pending[start..end];
        let safe = self.retain_safe_controls && bytes.last() == Some(&b'm') && valid_sgr(bytes);
        ParsedControl::Complete { end, safe }
    }

    fn parse_string_control(&self, start: usize, kind: SequenceKind) -> ParsedControl {
        self.parse_string_control_at(start, start + 2, kind)
    }

    fn parse_c1_control(
        &self,
        start: usize,
        prefix_length: usize,
        kind: SequenceKind,
    ) -> ParsedControl {
        if kind == SequenceKind::Csi {
            let body_start = start + prefix_length;
            let scan_start = self.scan_start(start, body_start);
            let rest = &self.pending[scan_start..];
            Self::record_scan_steps(rest.len());
            let Some(relative_end) = rest.iter().position(|byte| (0x40..=0x7e).contains(byte))
            else {
                return ParsedControl::Incomplete {
                    kind: SequenceKind::Csi,
                    next_scan: self.pending.len(),
                };
            };
            let end = scan_start + relative_end + 1;
            let safe = self.retain_safe_controls
                && self.pending[end - 1] == b'm'
                && valid_sgr_parameters(&self.pending[body_start..end - 1]);
            return ParsedControl::Complete { end, safe };
        }
        self.parse_string_control_at(start, start + prefix_length, kind)
    }

    fn parse_string_control_at(
        &self,
        start: usize,
        body_start: usize,
        kind: SequenceKind,
    ) -> ParsedControl {
        let scan_start = self.scan_start(start, body_start);
        let mut offset = scan_start;
        let mut terminator_end = None;
        let mut terminator_start = None;
        while let Some(byte) = self.pending.get(offset).copied() {
            match byte {
                0x07 if kind == SequenceKind::Osc => {
                    terminator_end = Some(offset + 1);
                    terminator_start = Some(offset);
                    break;
                }
                0x1b if self.pending.get(offset + 1) == Some(&b'\\') => {
                    terminator_end = Some(offset + 2);
                    terminator_start = Some(offset);
                    break;
                }
                0x9c if is_c1_st_terminator(&self.pending, offset) => {
                    terminator_end = Some(offset + 1);
                    terminator_start = Some(offset);
                    break;
                }
                0xc2 if self.pending.get(offset + 1) == Some(&0x9c) => {
                    terminator_end = Some(offset + 2);
                    terminator_start = Some(offset);
                    break;
                }
                _ => offset += 1,
            }
        }
        let Some(end) = terminator_end else {
            Self::record_scan_steps(self.pending.len().saturating_sub(scan_start));
            return ParsedControl::Incomplete {
                kind,
                next_scan: self.pending.len().saturating_sub(3).max(body_start),
            };
        };
        Self::record_scan_steps(offset.saturating_sub(scan_start));
        let safe = kind == SequenceKind::Osc
            && self.retain_safe_controls
            && safe_osc8_body(&self.pending[body_start..terminator_start.unwrap_or(end)]);
        ParsedControl::Complete { end, safe }
    }

    fn apply_control(
        &mut self,
        start: usize,
        parsed: ParsedControl,
        eof: bool,
        output: &mut Vec<u8>,
        offset: &mut usize,
    ) -> bool {
        match parsed {
            ParsedControl::Complete { end, safe } => {
                self.scan = None;
                if self.retain_safe_controls && safe && end - start <= MAX_CONTROL_BYTES {
                    output.extend_from_slice(&self.pending[start..end]);
                }
                *offset = end;
                false
            }
            ParsedControl::Incomplete { kind, next_scan } if !eof => {
                let pending_length = self.pending.len() - start;
                if pending_length > MAX_CONTROL_BYTES {
                    self.draining = Some(kind);
                    self.scan = None;
                    let retained_suffix =
                        if matches!(kind, SequenceKind::Osc | SequenceKind::String) {
                            string_drain_suffix_length(&self.pending[start..])
                        } else {
                            0
                        };
                    *offset = self.pending.len() - retained_suffix;
                } else {
                    self.scan = Some(ControlScan {
                        start,
                        next: next_scan,
                    });
                }
                true
            }
            ParsedControl::Incomplete { .. } => {
                self.scan = None;
                *offset = self.pending.len();
                true
            }
        }
    }

    fn parse_escape_sequence(&self, start: usize) -> ParsedControl {
        let scan_start = self.scan_start(start, start + 1);
        let mut offset = scan_start;
        while offset < self.pending.len() && (0x20..=0x2f).contains(&self.pending[offset]) {
            offset += 1;
        }
        if offset >= self.pending.len() {
            Self::record_scan_steps(offset.saturating_sub(scan_start));
            return ParsedControl::Incomplete {
                kind: SequenceKind::Escape,
                next_scan: offset,
            };
        }
        Self::record_scan_steps(offset.saturating_sub(scan_start) + 1);
        if (0x30..=0x7e).contains(&self.pending[offset]) {
            ParsedControl::Complete {
                end: offset + 1,
                safe: false,
            }
        } else {
            ParsedControl::Complete {
                // Drop the ESC plus any intermediates, then let the invalid
                // byte be processed normally. This matches the streaming
                // overflow drain path and keeps chunked input invariant.
                end: offset,
                safe: false,
            }
        }
    }

    fn scan_start(&self, start: usize, initial: usize) -> usize {
        self.scan
            .filter(|scan| scan.start == start)
            .map_or(initial, |scan| scan.next.max(initial))
    }

    fn record_scan_steps(count: usize) {
        #[cfg(test)]
        CONTROL_SCAN_STEPS.with(|steps| steps.set(steps.get().saturating_add(count)));
        #[cfg(not(test))]
        let _ = count;
    }
}

#[derive(Clone, Copy, Debug)]
enum ParsedControl {
    Complete {
        end: usize,
        safe: bool,
    },
    Incomplete {
        kind: SequenceKind,
        next_scan: usize,
    },
}

/// Converts a document using the configured input and output formats.
///
/// ANSI and plain output preserve ordinary bytes, including invalid UTF-8. Markdown and HTML
/// output require UTF-8 and return an input error for invalid text. Markdown input is limited to
/// 8 MiB through this in-memory API; use [`render_stream`] for larger Markdown documents. Markdown
/// input to Markdown output rejects `skip_code = false`, because standard Markdown cannot safely
/// emphasize text inside code spans or blocks. HTML output escapes raw HTML and permits only safe
/// link destinations. `enabled` controls only added bionic emphasis; format conversion and control
/// filtering still apply when it is false.
///
/// # Errors
///
/// Returns an error when settings are invalid, markup output input is invalid UTF-8, or the
/// Markdown input document exceeds the in-memory parser limit, or requests unsupported code
/// emphasis in Markdown output.
///
/// # Panics
///
/// Does not panic for valid settings when using the built-in Unicode segmenter.
#[must_use = "write or inspect the rendered document"]
pub fn render(input: &[u8], settings: &Settings, enabled: bool) -> Result<Vec<u8>, Error> {
    let segmenter = UnicodeSegmenter::new();
    render_with_segmenter(input, settings, enabled, &segmenter)
}

/// Converts a document with a caller-supplied word segmenter.
///
/// The segmenter controls word boundaries for Markdown, HTML, and text-to-Markdown conversion.
/// ANSI terminal streams use the transformer's token renderer so existing terminal styling and
/// byte boundaries remain intact. Markdown input has an 8 MiB limit through this slice API; use
/// [`render_stream`] for larger documents. Markdown input to Markdown output rejects
/// `skip_code = false`, because standard Markdown cannot safely emphasize code spans or blocks.
///
/// # Errors
///
/// Returns the same errors as [`render`].
///
/// # Panics
///
/// Panics if `segmenter` returns out-of-bounds, unordered, overlapping, or non-grapheme ranges.
#[must_use = "write or inspect the rendered document"]
pub fn render_with_segmenter(
    input: &[u8],
    settings: &Settings,
    enabled: bool,
    segmenter: &dyn WordSegmenter,
) -> Result<Vec<u8>, Error> {
    settings.validate()?;
    validate_format_combination(settings)?;
    if settings.input_format == InputFormat::Markdown && input.len() > MAX_MARKDOWN_SLICE_BYTES {
        return Err(Error::invalid_input(format!(
            "Markdown slice exceeds the {MAX_MARKDOWN_SLICE_BYTES}-byte rendering limit; use render_stream for larger documents"
        )));
    }
    if settings.input_format == InputFormat::Markdown {
        if settings.format == OutputFormat::Plain {
            return Ok(strip_controls(input));
        }
        let source = strip_controls(input);
        let source = str::from_utf8(&source).map_err(|error| {
            Error::invalid_input_with_source("Markdown input must be valid UTF-8".to_owned(), error)
        })?;
        return Ok(render_markdown(source, settings, enabled, segmenter));
    }

    match settings.format {
        OutputFormat::Ansi => {
            let sanitized = apply_control_policy(input, effective_control_policy(settings));
            Ok(transformer::transform_with_segmenter(
                &sanitized, settings, enabled, segmenter,
            ))
        }
        OutputFormat::Plain => Ok(strip_controls(input)),
        OutputFormat::Markdown | OutputFormat::Html => {
            let visible = strip_controls(input);
            let visible = str::from_utf8(&visible).map_err(|error| {
                Error::invalid_input_with_source(
                    "text-to-document conversion requires valid UTF-8".to_owned(),
                    error,
                )
            })?;
            let mut engine = FixationEngine::new(settings);
            let spans = if enabled {
                engine.spans(visible, segmenter)
            } else {
                Vec::new()
            };
            if settings.format == OutputFormat::Markdown {
                Ok(render_markdown_text(visible, &spans))
            } else {
                Ok(render_html_text(visible, &spans, settings).into_bytes())
            }
        }
    }
}

/// Converts a stream while keeping at most a fixed number of input bytes in memory.
///
/// Plain text and terminal ANSI output are processed in chunks. Plain text to Markdown or HTML
/// retains at most one incomplete word; words longer than 64 KiB pass through without emphasis.
/// Markdown input is parsed one block at a time with a 1 MiB block buffer and has no total-size
/// limit. If a block exceeds that size, the remainder is passed through unstyled, or escaped as
/// literal Markdown or HTML text.
/// A temporary file is used to index at most 1 MiB of link-reference definitions before rendering;
/// if the bounded index cannot be complete, the stream is passed through using the same fallback.
/// Markdown input to Markdown output rejects `skip_code = false`, because standard Markdown cannot
/// safely emphasize text inside code spans or blocks.
///
/// # Errors
///
/// Returns an error for input or output I/O failures, invalid settings, or invalid UTF-8 in document
/// conversion.
pub fn render_stream(
    input: &mut dyn Read,
    output: &mut dyn Write,
    settings: &Settings,
    enabled: bool,
) -> Result<(), Error> {
    settings.validate()?;
    validate_format_combination(settings)?;
    if settings.input_format == InputFormat::Markdown {
        return render_stream_markdown(input, output, settings, enabled);
    }
    match settings.format {
        OutputFormat::Ansi | OutputFormat::Plain => {
            render_stream_terminal(input, output, settings, enabled)
        }
        OutputFormat::Markdown | OutputFormat::Html => {
            render_stream_text_markup(input, output, settings, enabled)
        }
    }
}

fn render_stream_markdown(
    input: &mut dyn Read,
    output: &mut dyn Write,
    settings: &Settings,
    enabled: bool,
) -> Result<(), Error> {
    if settings.format == OutputFormat::Plain {
        return render_stream_markdown_plain(input, output);
    }
    let mut spool = spool_markdown_input(input)?;
    let (references, references_complete) = collect_markdown_references(&mut spool)?;
    spool.seek(SeekFrom::Start(0)).map_err(|error| {
        Error::with_source("could not rewind Markdown spool file".to_owned(), error)
    })?;
    let segmenter = UnicodeSegmenter::new();
    let mut renderer = MarkdownBlockRenderer::new(settings, enabled, &segmenter, &references);
    if !references_complete {
        renderer.fallback = true;
    }
    render_spooled_markdown(&mut spool, output, &mut renderer)
}

fn spool_markdown_input(input: &mut dyn Read) -> Result<File, Error> {
    let mut spool = tempfile::tempfile().map_err(|error| {
        Error::with_source("could not create Markdown spool file".to_owned(), error)
    })?;
    let mut controls = ControlFilter::plain_text();
    let mut incomplete_utf8 = Vec::new();
    let mut chunk = [0_u8; STREAM_CHUNK_BYTES];
    loop {
        let count = input.read(&mut chunk).map_err(|error| {
            Error::with_source("could not read document input".to_owned(), error)
        })?;
        if count == 0 {
            break;
        }
        let filtered = controls.push(&chunk[..count]);
        let mut bytes = std::mem::take(&mut incomplete_utf8);
        bytes.extend_from_slice(&filtered);
        let valid_length = valid_utf8_prefix(&bytes)?;
        incomplete_utf8.extend_from_slice(&bytes[valid_length..]);
        spool.write_all(&bytes[..valid_length]).map_err(|error| {
            Error::with_source("could not spool Markdown input".to_owned(), error)
        })?;
    }
    let filtered = controls.finish();
    let mut bytes = std::mem::take(&mut incomplete_utf8);
    bytes.extend_from_slice(&filtered);
    let valid = str::from_utf8(&bytes).map_err(|error| {
        Error::invalid_input_with_source("Markdown input must be valid UTF-8".to_owned(), error)
    })?;
    spool
        .write_all(valid.as_bytes())
        .map_err(|error| Error::with_source("could not spool Markdown input".to_owned(), error))?;
    spool.flush().map_err(|error| {
        Error::with_source("could not flush Markdown spool file".to_owned(), error)
    })?;
    Ok(spool)
}

fn valid_utf8_prefix(bytes: &[u8]) -> Result<usize, Error> {
    match str::from_utf8(bytes) {
        Ok(_) => Ok(bytes.len()),
        Err(error) if error.error_len().is_none() => Ok(error.valid_up_to()),
        Err(error) => Err(Error::invalid_input_with_source(
            "Markdown input must be valid UTF-8".to_owned(),
            error,
        )),
    }
}

fn render_spooled_markdown(
    spool: &mut File,
    output: &mut dyn Write,
    renderer: &mut MarkdownBlockRenderer<'_>,
) -> Result<(), Error> {
    let mut incomplete_utf8 = Vec::new();
    let mut chunk = [0_u8; STREAM_CHUNK_BYTES];
    loop {
        let count = spool.read(&mut chunk).map_err(|error| {
            Error::with_source("could not read Markdown spool file".to_owned(), error)
        })?;
        if count == 0 {
            break;
        }
        let mut bytes = std::mem::take(&mut incomplete_utf8);
        bytes.extend_from_slice(&chunk[..count]);
        let valid_length = valid_utf8_prefix(&bytes)?;
        incomplete_utf8.extend_from_slice(&bytes[valid_length..]);
        let valid = str::from_utf8(&bytes[..valid_length]).map_err(|error| {
            Error::invalid_input_with_source("Markdown input must be valid UTF-8".to_owned(), error)
        })?;
        renderer.process(valid, output)?;
    }
    let tail = str::from_utf8(&incomplete_utf8).map_err(|error| {
        Error::invalid_input_with_source("Markdown input must be valid UTF-8".to_owned(), error)
    })?;
    renderer.process(tail, output)?;
    renderer.finish(output)
}

fn render_stream_markdown_plain(input: &mut dyn Read, output: &mut dyn Write) -> Result<(), Error> {
    let mut controls = ControlFilter::plain_text();
    let mut chunk = [0_u8; STREAM_CHUNK_BYTES];
    loop {
        let count = input.read(&mut chunk).map_err(|error| {
            Error::with_source("could not read document input".to_owned(), error)
        })?;
        if count == 0 {
            break;
        }
        let filtered = controls.push(&chunk[..count]);
        write_stream_chunk(output, &filtered)?;
    }
    let tail = controls.finish();
    write_stream_chunk(output, &tail)
}

fn collect_markdown_references(spool: &mut File) -> Result<(String, bool), Error> {
    spool.seek(SeekFrom::Start(0)).map_err(|error| {
        Error::with_source("could not rewind Markdown spool file".to_owned(), error)
    })?;
    let mut references = Vec::new();
    let mut references_complete = true;
    let mut fence = None;
    let mut line = Vec::new();
    let mut line_too_long = false;
    let mut chunk = [0_u8; STREAM_CHUNK_BYTES];
    loop {
        let count = spool.read(&mut chunk).map_err(|error| {
            Error::with_source("could not read Markdown spool file".to_owned(), error)
        })?;
        if count == 0 {
            break;
        }
        for byte in &chunk[..count] {
            if line.len() < MAX_MARKDOWN_BLOCK_BYTES {
                line.push(*byte);
            } else {
                line_too_long = true;
            }
            if *byte == b'\n' {
                collect_reference_line(
                    &line,
                    line_too_long,
                    &mut fence,
                    &mut references,
                    &mut references_complete,
                );
                line.clear();
                line_too_long = false;
            }
        }
    }
    if !line.is_empty() || line_too_long {
        collect_reference_line(
            &line,
            line_too_long,
            &mut fence,
            &mut references,
            &mut references_complete,
        );
    }
    spool.seek(SeekFrom::Start(0)).map_err(|error| {
        Error::with_source("could not rewind Markdown spool file".to_owned(), error)
    })?;
    let references = if references_complete {
        String::from_utf8(references).map_err(|error| {
            Error::invalid_input_with_source("Markdown input must be valid UTF-8".to_owned(), error)
        })?
    } else {
        String::new()
    };
    Ok((references, references_complete))
}

fn collect_reference_line(
    line: &[u8],
    line_too_long: bool,
    fence: &mut Option<FenceMarker>,
    references: &mut Vec<u8>,
    references_complete: &mut bool,
) {
    if line_too_long {
        if is_reference_definition_line(line) || could_change_fence(line, *fence) {
            *references_complete = false;
            references.clear();
        }
        return;
    }
    if fence.is_none() && is_reference_definition_line(line) {
        if references.len().saturating_add(line.len()) > MAX_MARKDOWN_REFERENCE_BYTES {
            *references_complete = false;
            references.clear();
        } else if *references_complete {
            references.extend_from_slice(line);
        }
    }
    update_fence(line, fence);
}

fn could_change_fence(line: &[u8], fence: Option<FenceMarker>) -> bool {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    let indentation = line.iter().take_while(|byte| **byte == b' ').count();
    if indentation > 3 || indentation == line.len() {
        return false;
    }
    let rest = &line[indentation..];
    if let Some(open) = fence {
        rest.iter()
            .take_while(|byte| **byte == open.character)
            .count()
            >= open.length
    } else {
        let Some(&character @ (b'`' | b'~')) = rest.first() else {
            return false;
        };
        rest.iter().take_while(|byte| **byte == character).count() >= 3
    }
}

fn is_reference_definition_line(line: &[u8]) -> bool {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    let indentation = line.iter().take_while(|byte| **byte == b' ').count();
    if indentation > 3 {
        return false;
    }
    let rest = &line[indentation..];
    let Some(label) = rest.strip_prefix(b"[") else {
        return false;
    };
    let Some(end) = label.iter().position(|byte| *byte == b']') else {
        return false;
    };
    end > 0 && label.get(end + 1) == Some(&b':')
}

struct MarkdownBlockRenderer<'a> {
    settings: &'a Settings,
    enabled: bool,
    segmenter: &'a dyn WordSegmenter,
    references: Parser<'a>,
    engine: FixationEngine,
    block: Vec<u8>,
    line: Vec<u8>,
    fence: Option<FenceMarker>,
    fallback: bool,
}

#[derive(Clone, Copy)]
struct FenceMarker {
    character: u8,
    length: usize,
}

impl<'a> MarkdownBlockRenderer<'a> {
    fn new(
        settings: &'a Settings,
        enabled: bool,
        segmenter: &'a dyn WordSegmenter,
        references: &'a str,
    ) -> Self {
        Self {
            settings,
            enabled,
            segmenter,
            references: Parser::new_ext(references, Options::empty()),
            engine: FixationEngine::new(settings),
            block: Vec::new(),
            line: Vec::new(),
            fence: None,
            fallback: false,
        }
    }

    fn process(&mut self, text: &str, output: &mut dyn Write) -> Result<(), Error> {
        if self.fallback {
            return self.write_fallback(text, output);
        }
        for (offset, character) in text.char_indices() {
            let mut encoded = [0_u8; 4];
            self.line
                .extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
            if character == '\n' {
                self.process_line(output)?;
            } else if self.block.len().saturating_add(self.line.len()) > MAX_MARKDOWN_BLOCK_BYTES {
                self.start_fallback(output)?;
            }
            if self.fallback {
                let remaining = &text[offset + character.len_utf8()..];
                return self.write_fallback(remaining, output);
            }
        }
        Ok(())
    }

    fn process_line(&mut self, output: &mut dyn Write) -> Result<(), Error> {
        let line = std::mem::take(&mut self.line);
        if self.fence.is_none() && is_blank_markdown_line(&line) {
            self.block.extend_from_slice(&line);
            return self.render_block(output);
        }
        update_fence(&line, &mut self.fence);
        self.block.extend_from_slice(&line);
        if self.block.len() > MAX_MARKDOWN_BLOCK_BYTES {
            self.start_fallback(output)?;
        }
        Ok(())
    }

    fn start_fallback(&mut self, output: &mut dyn Write) -> Result<(), Error> {
        self.fallback = true;
        let mut bytes = std::mem::take(&mut self.block);
        bytes.extend_from_slice(&self.line);
        self.line.clear();
        self.write_fallback_bytes(&bytes, output)
    }

    fn render_block(&mut self, output: &mut dyn Write) -> Result<(), Error> {
        if self.block.is_empty() {
            return Ok(());
        }
        let block_source = str::from_utf8(&self.block).map_err(|error| {
            Error::invalid_input_with_source("Markdown input must be valid UTF-8".to_owned(), error)
        })?;
        let rendered = render_markdown_with_engine(
            block_source,
            self.settings,
            self.enabled,
            self.segmenter,
            &mut self.engine,
            Some(self.references.reference_definitions()),
        );
        self.block.clear();
        write_stream_chunk(output, &rendered)
    }

    fn finish(&mut self, output: &mut dyn Write) -> Result<(), Error> {
        if !self.fallback {
            self.block.extend_from_slice(&self.line);
            self.line.clear();
            return self.render_block(output);
        }
        if !self.line.is_empty() {
            let line = std::mem::take(&mut self.line);
            self.write_fallback_bytes(&line, output)?;
        }
        Ok(())
    }

    fn write_fallback(&self, text: &str, output: &mut dyn Write) -> Result<(), Error> {
        self.write_fallback_bytes(text.as_bytes(), output)
    }

    fn write_fallback_bytes(&self, bytes: &[u8], output: &mut dyn Write) -> Result<(), Error> {
        match self.settings.format {
            OutputFormat::Markdown => {
                let text = str::from_utf8(bytes).map_err(|error| {
                    Error::invalid_input_with_source(
                        "Markdown input must be valid UTF-8".to_owned(),
                        error,
                    )
                })?;
                write_stream_chunk(output, &escape_markdown_text(text))
            }
            OutputFormat::Html => {
                let text = str::from_utf8(bytes).map_err(|error| {
                    Error::invalid_input_with_source(
                        "Markdown input must be valid UTF-8".to_owned(),
                        error,
                    )
                })?;
                write_stream_chunk(output, escape_html(text).as_bytes())
            }
            OutputFormat::Ansi | OutputFormat::Plain => write_stream_chunk(output, bytes),
        }
    }
}

fn is_blank_markdown_line(line: &[u8]) -> bool {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    line.iter().all(|byte| matches!(*byte, b' ' | b'\t'))
}

fn update_fence(line: &[u8], fence: &mut Option<FenceMarker>) {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    let indentation = line.iter().take_while(|byte| **byte == b' ').count();
    if indentation > 3 || indentation == line.len() {
        return;
    }
    let rest = &line[indentation..];
    if let Some(open) = *fence {
        let run = rest
            .iter()
            .take_while(|byte| **byte == open.character)
            .count();
        if run >= open.length && rest[run..].iter().all(|byte| matches!(*byte, b' ' | b'\t')) {
            *fence = None;
        }
        return;
    }
    let Some(&character @ (b'`' | b'~')) = rest.first() else {
        return;
    };
    let run = rest.iter().take_while(|byte| **byte == character).count();
    if run >= 3 && !(character == b'`' && rest[run..].contains(&b'`')) {
        *fence = Some(FenceMarker {
            character,
            length: run,
        });
    }
}

fn render_stream_text_markup(
    input: &mut dyn Read,
    output: &mut dyn Write,
    settings: &Settings,
    enabled: bool,
) -> Result<(), Error> {
    let mut controls = ControlFilter::plain_text();
    let mut incomplete_utf8 = Vec::new();
    let mut markup = TextMarkupState::new(settings, enabled);
    let mut chunk = [0_u8; STREAM_CHUNK_BYTES];

    loop {
        let count = input.read(&mut chunk).map_err(|error| {
            Error::with_source("could not read document input".to_owned(), error)
        })?;
        if count == 0 {
            break;
        }
        let filtered = controls.push(&chunk[..count]);
        let mut bytes = std::mem::take(&mut incomplete_utf8);
        bytes.extend_from_slice(&filtered);
        let valid_length = match str::from_utf8(&bytes) {
            Ok(_) => bytes.len(),
            Err(error) if error.error_len().is_none() => {
                let valid_length = error.valid_up_to();
                incomplete_utf8.extend_from_slice(&bytes[valid_length..]);
                valid_length
            }
            Err(error) => {
                return Err(Error::invalid_input_with_source(
                    "text-to-document conversion requires valid UTF-8".to_owned(),
                    error,
                ));
            }
        };
        let valid = str::from_utf8(&bytes[..valid_length]).map_err(|error| {
            Error::invalid_input_with_source(
                "text-to-document conversion requires valid UTF-8".to_owned(),
                error,
            )
        })?;
        markup.process_chunk(valid, output)?;
    }

    let filtered = controls.finish();
    let mut bytes = std::mem::take(&mut incomplete_utf8);
    bytes.extend_from_slice(&filtered);
    let valid = str::from_utf8(&bytes).map_err(|error| {
        Error::invalid_input_with_source(
            "text-to-document conversion requires valid UTF-8".to_owned(),
            error,
        )
    })?;
    markup.process_chunk(valid, output)?;

    let tail = markup.process_pending(true)?;
    write_stream_chunk(output, &tail)?;
    Ok(())
}

struct TextMarkupState<'a> {
    settings: &'a Settings,
    enabled: bool,
    pending: Vec<u8>,
    oversized_word_passthrough: bool,
    engine: FixationEngine,
    segmenter: UnicodeSegmenter,
}

impl<'a> TextMarkupState<'a> {
    fn new(settings: &'a Settings, enabled: bool) -> Self {
        Self {
            settings,
            enabled,
            pending: Vec::new(),
            oversized_word_passthrough: false,
            engine: FixationEngine::new(settings),
            segmenter: UnicodeSegmenter::new(),
        }
    }

    fn process_chunk(&mut self, text: &str, output: &mut dyn Write) -> Result<(), Error> {
        if text.is_empty() {
            return Ok(());
        }
        if !self.enabled {
            let bytes = render_unemphasized_markup(text, self.settings);
            return write_stream_chunk(output, &bytes);
        }

        let mut remaining = text;
        if self.oversized_word_passthrough {
            let word_ranges = self.segmenter.word_ranges(remaining);
            match word_ranges.first() {
                Some(word) if word.start == 0 => {
                    let end = word.end;
                    let raw = render_unemphasized_markup(&remaining[..end], self.settings);
                    write_stream_chunk(output, &raw)?;
                    self.oversized_word_passthrough = end == remaining.len();
                    remaining = &remaining[end..];
                    if remaining.is_empty() {
                        return Ok(());
                    }
                }
                _ => self.oversized_word_passthrough = false,
            }
        }

        self.pending.extend_from_slice(remaining.as_bytes());
        let rendered = self.process_pending(false)?;
        write_stream_chunk(output, &rendered)
    }

    fn process_pending(&mut self, eof: bool) -> Result<Vec<u8>, Error> {
        let mut output = Vec::new();
        loop {
            if self.pending.is_empty() {
                break;
            }
            let text = str::from_utf8(&self.pending).map_err(|error| {
                Error::invalid_input_with_source(
                    "text-to-document conversion requires valid UTF-8".to_owned(),
                    error,
                )
            })?;
            let word_ranges = self.segmenter.word_ranges(text);
            if let Some(long_word) = word_ranges
                .iter()
                .find(|range| range.end - range.start > MAX_TEXT_WORD_BYTES)
            {
                let before = &text[..long_word.start];
                if !before.is_empty() {
                    let spans = self.engine.spans(before, &self.segmenter);
                    output.extend(render_markup_fragment(before, &spans, self.settings));
                }
                let word = &text[long_word.clone()];
                let _ = self.engine.spans(word, &self.segmenter);
                output.extend(render_unemphasized_markup(word, self.settings));
                let end = long_word.end;
                let text_length = text.len();
                self.pending.drain(..end);
                if self.pending.is_empty() || end == text_length {
                    self.oversized_word_passthrough = !eof;
                    break;
                }
                self.oversized_word_passthrough = false;
                continue;
            }

            let safe_length = safe_text_markup_length(text, &self.segmenter, eof);
            if safe_length == 0 {
                break;
            }
            let safe_text = &text[..safe_length];
            let spans = self.engine.spans(safe_text, &self.segmenter);
            output.extend(render_markup_fragment(safe_text, &spans, self.settings));
            self.pending.drain(..safe_length);
            if !eof {
                break;
            }
        }
        Ok(output)
    }
}

fn safe_text_markup_length(text: &str, segmenter: &dyn WordSegmenter, eof: bool) -> usize {
    if eof {
        return text.len();
    }
    segmenter.trailing_word_start(text).unwrap_or(text.len())
}

fn render_markup_fragment(text: &str, spans: &[Range<usize>], settings: &Settings) -> Vec<u8> {
    if settings.format == OutputFormat::Markdown {
        render_markdown_text(text, spans)
    } else {
        render_html_text(text, spans, settings).into_bytes()
    }
}

fn render_unemphasized_markup(text: &str, settings: &Settings) -> Vec<u8> {
    render_markup_fragment(text, &[], settings)
}

fn render_stream_terminal(
    input: &mut dyn Read,
    output: &mut dyn Write,
    settings: &Settings,
    enabled: bool,
) -> Result<(), Error> {
    let strip_all = settings.format == OutputFormat::Plain;
    let mut filter = ControlFilter {
        policy: if strip_all {
            ControlPolicy::Safe
        } else {
            effective_control_policy(settings)
        },
        retain_safe_controls: !strip_all,
        pending: Vec::new(),
        draining: None,
        scan: None,
    };
    let mut transformer = Transformer::new(settings, enabled && !strip_all);
    let mut chunk = [0_u8; STREAM_CHUNK_BYTES];
    loop {
        let count = input.read(&mut chunk).map_err(|error| {
            Error::with_source("could not read document input".to_owned(), error)
        })?;
        if count == 0 {
            break;
        }
        let filtered = filter.push(&chunk[..count]);
        let rendered = if strip_all {
            filtered
        } else {
            transformer.push(&filtered)
        };
        write_stream_chunk(output, &rendered)?;
    }
    let filtered = filter.finish();
    let tail = if strip_all {
        filtered
    } else {
        let mut tail = transformer.push(&filtered);
        tail.extend(transformer.finish());
        tail
    };
    write_stream_chunk(output, &tail)
}

fn effective_control_policy(settings: &Settings) -> ControlPolicy {
    if settings.input_format == InputFormat::Terminal && settings.format == OutputFormat::Ansi {
        ControlPolicy::Preserve
    } else {
        settings.controls
    }
}

fn write_stream_chunk(output: &mut dyn Write, bytes: &[u8]) -> Result<(), Error> {
    if bytes.is_empty() {
        return Ok(());
    }
    output.write_all(bytes).map_err(|error| {
        Error::with_source("could not write rendered document".to_owned(), error)
    })?;
    output
        .flush()
        .map_err(|error| Error::with_source("could not flush rendered document".to_owned(), error))
}

fn apply_control_policy(input: &[u8], policy: ControlPolicy) -> Vec<u8> {
    let mut filter = ControlFilter::new(policy);
    let mut output = filter.push(input);
    output.extend(filter.finish());
    output
}

fn strip_controls(input: &[u8]) -> Vec<u8> {
    let mut filter = ControlFilter::plain_text();
    let mut output = filter.push(input);
    output.extend(filter.finish());
    output
}

fn is_c1_st_terminator(input: &[u8], offset: usize) -> bool {
    if input.get(offset) != Some(&0x9c) {
        return false;
    }
    if offset > 0 && input[offset - 1] == 0xc2 {
        return true;
    }

    let earliest_start = offset.saturating_sub(3);
    for start in earliest_start..offset {
        let width = match input[start] {
            0xc2..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf4 => 4,
            _ => continue,
        };
        if start + width == offset + 1 && str::from_utf8(&input[start..=offset]).is_ok() {
            return false;
        }
    }
    true
}

fn string_drain_suffix_length(input: &[u8]) -> usize {
    let mut suffix_length = usize::from(input.last() == Some(&0x1b));
    for start in input.len().saturating_sub(3)..input.len() {
        if (0xc2..=0xf4).contains(&input[start])
            && str::from_utf8(&input[start..]).is_err_and(|error| error.error_len().is_none())
        {
            suffix_length = suffix_length.max(input.len() - start);
        }
    }
    suffix_length
}

fn drain_sequence(input: &[u8], kind: SequenceKind) -> Option<usize> {
    match kind {
        SequenceKind::Csi => input
            .iter()
            .position(|byte| (0x40..=0x7e).contains(byte))
            .map(|index| index + 1),
        SequenceKind::Osc => input
            .iter()
            .enumerate()
            .find_map(|(index, byte)| match *byte {
                0x07 => Some(index + 1),
                0x9c if is_c1_st_terminator(input, index) => Some(index + 1),
                0x1b if input.get(index + 1) == Some(&b'\\') => Some(index + 2),
                0xc2 if input.get(index + 1) == Some(&0x9c) => Some(index + 2),
                _ => None,
            }),
        SequenceKind::String => find_string_terminator(input),
        SequenceKind::Escape => input
            .iter()
            .position(|byte| !(0x20..=0x2f).contains(byte))
            .map(|index| {
                if (0x30..=0x7e).contains(&input[index]) {
                    index + 1
                } else {
                    index
                }
            }),
    }
}

fn valid_sgr(bytes: &[u8]) -> bool {
    bytes.len() >= 3 && bytes[0..2] == *b"\x1b[" && valid_sgr_parameters(&bytes[2..bytes.len() - 1])
}

fn valid_sgr_parameters(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .all(|byte| matches!(*byte, b'0'..=b'9' | b';' | b':'))
}

fn c1_sequence_kind(control: u8) -> Option<SequenceKind> {
    match control {
        0x9b => Some(SequenceKind::Csi),
        0x9d => Some(SequenceKind::Osc),
        0x90 | 0x98 | 0x9e | 0x9f => Some(SequenceKind::String),
        _ => None,
    }
}

fn find_string_terminator(input: &[u8]) -> Option<usize> {
    let mut offset = 0;
    while offset < input.len() {
        match input[offset] {
            0x9c if is_c1_st_terminator(input, offset) => return Some(offset + 1),
            0x1b if input.get(offset + 1) == Some(&b'\\') => return Some(offset + 2),
            0xc2 if input.get(offset + 1) == Some(&0x9c) => return Some(offset + 2),
            _ => offset += 1,
        }
    }
    None
}

fn inspect_utf8_control(input: &[u8]) -> Utf8Status {
    let Some(&first) = input.first() else {
        return Utf8Status::Incomplete;
    };
    let expected_length = match first {
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => return Utf8Status::Invalid,
    };
    if input.len() < expected_length {
        return match str::from_utf8(input) {
            Err(error) if error.error_len().is_none() => Utf8Status::Incomplete,
            _ => Utf8Status::Invalid,
        };
    }
    let Ok(character) = str::from_utf8(&input[..expected_length]) else {
        return Utf8Status::Invalid;
    };
    let c1_control = character
        .chars()
        .next()
        .is_some_and(|character| ('\u{80}'..='\u{9f}').contains(&character));
    Utf8Status::Complete {
        length: expected_length,
        c1_control,
    }
}

#[cfg(test)]
fn safe_osc8(bytes: &[u8]) -> bool {
    let Some(body) = bytes.strip_prefix(b"\x1b]") else {
        return false;
    };
    let body = body
        .strip_suffix(b"\x07")
        .or_else(|| body.strip_suffix(b"\x1b\\"));
    let Some(body) = body else {
        return false;
    };
    safe_osc8_body(body)
}

fn safe_osc8_body(body: &[u8]) -> bool {
    let Some(rest) = body.strip_prefix(b"8;") else {
        return false;
    };
    let Some(separator) = rest.iter().position(|byte| *byte == b';') else {
        return false;
    };
    let url = &rest[separator + 1..];
    if url.is_empty() {
        return true;
    }
    let Ok(url) = str::from_utf8(url) else {
        return false;
    };
    has_safe_scheme(url) && safe_url(url)
}

fn safe_url(url: &str) -> bool {
    if url.chars().any(|character| {
        character.is_control()
            || character.is_whitespace()
            || matches!(character, '\\' | '\'' | '"' | '<' | '>')
    }) {
        return false;
    }

    let Some(colon) = url.find(':') else {
        return true;
    };
    let first_path_delimiter = url.find(['/', '?', '#']).unwrap_or(url.len());
    if colon >= first_path_delimiter {
        return true;
    }
    matches!(
        &url[..colon],
        scheme if scheme.eq_ignore_ascii_case("http")
            || scheme.eq_ignore_ascii_case("https")
            || scheme.eq_ignore_ascii_case("mailto")
    )
}

fn has_safe_scheme(url: &str) -> bool {
    let Some((scheme, _)) = url.split_once(':') else {
        return false;
    };
    scheme.eq_ignore_ascii_case("http")
        || scheme.eq_ignore_ascii_case("https")
        || scheme.eq_ignore_ascii_case("mailto")
}

fn render_markdown(
    source: &str,
    settings: &Settings,
    enabled: bool,
    segmenter: &dyn WordSegmenter,
) -> Vec<u8> {
    let mut engine = FixationEngine::new(settings);
    render_markdown_with_engine(source, settings, enabled, segmenter, &mut engine, None)
}

fn render_markdown_with_engine(
    source: &str,
    settings: &Settings,
    enabled: bool,
    segmenter: &dyn WordSegmenter,
    engine: &mut FixationEngine,
    references: Option<&RefDefs<'_>>,
) -> Vec<u8> {
    match settings.format {
        OutputFormat::Markdown | OutputFormat::Ansi => {
            render_markdown_source(source, settings, enabled, segmenter, engine, references)
        }
        OutputFormat::Plain => source.as_bytes().to_vec(),
        OutputFormat::Html => {
            render_markdown_html(source, settings, enabled, segmenter, engine, references)
                .into_bytes()
        }
    }
}

fn markdown_parser<'a>(
    source: &'a str,
    references: Option<&'a RefDefs<'a>>,
) -> impl Iterator<Item = (Event<'a>, Range<usize>)> {
    let callback = move |link: BrokenLink<'a>| {
        references
            .and_then(|definitions| definitions.get(&link.reference))
            .map(|definition| {
                (
                    definition.dest.clone(),
                    definition.title.clone().unwrap_or_else(|| "".into()),
                )
            })
    };
    Parser::new_with_broken_link_callback(source, Options::empty(), Some(callback))
        .into_offset_iter()
}

#[expect(
    clippy::too_many_lines,
    reason = "keep parser nesting state and offset-based source edits visible in one pass"
)]
fn render_markdown_source(
    source: &str,
    settings: &Settings,
    enabled: bool,
    segmenter: &dyn WordSegmenter,
    engine: &mut FixationEngine,
    references: Option<&RefDefs<'_>>,
) -> Vec<u8> {
    let output_source = source;
    let events: Vec<(Event<'_>, Range<usize>)> = markdown_parser(source, references).collect();
    let unsafe_ranges = unsafe_markdown_link_ranges(&events, settings.format);
    let mut edits: Vec<(Range<usize>, Vec<u8>)> = unsafe_ranges
        .iter()
        .map(|range| (range.clone(), escape_markdown_text(&source[range.clone()])))
        .collect();
    let mut strong_depth = 0_usize;
    let mut code_depth = 0_usize;
    let mut image_depth = 0_usize;
    let mut strong_suffix_to_skip = vec![0; events.len()];
    for (event_index, (event, range)) in events.iter().enumerate() {
        match event {
            Event::Start(Tag::Strong) => strong_depth += 1,
            Event::End(pulldown_cmark::TagEnd::Strong) => {
                strong_depth = strong_depth.saturating_sub(1);
            }
            Event::Start(Tag::Image { .. }) => image_depth += 1,
            Event::End(pulldown_cmark::TagEnd::Image) => {
                image_depth = image_depth.saturating_sub(1);
            }
            Event::Start(Tag::CodeBlock(_)) => code_depth += 1,
            Event::End(pulldown_cmark::TagEnd::CodeBlock) => {
                code_depth = code_depth.saturating_sub(1);
            }
            Event::Code(_)
                if settings.format == OutputFormat::Ansi && !settings.skip_code && enabled =>
            {
                let raw = &source[range.clone()];
                let delimiter = raw.bytes().take_while(|byte| *byte == b'`').count();
                let inner = &raw[delimiter..raw.len() - delimiter];
                let spans = engine.spans(inner, segmenter);
                append_emphasis_edits(source, range.start + delimiter, spans, settings, &mut edits);
            }
            Event::Text(text)
                if enabled
                    && strong_depth > 0
                    && image_depth == 0
                    && (code_depth == 0
                        || settings.format == OutputFormat::Ansi && !settings.skip_code)
                    && !unsafe_ranges
                        .iter()
                        .any(|blocked| range.start < blocked.end && range.end > blocked.start) =>
            {
                if let Some((suffix_event, suffix_len)) =
                    advance_markdown_strong_text(&events, event_index, text, segmenter, engine)
                {
                    strong_suffix_to_skip[suffix_event] = suffix_len;
                }
            }
            Event::Text(text)
                if strong_depth == 0
                    && image_depth == 0
                    && (settings.format == OutputFormat::Ansi && !settings.skip_code
                        || code_depth == 0)
                    && !unsafe_ranges
                        .iter()
                        .any(|blocked| range.start < blocked.end && range.end > blocked.start) =>
            {
                let skipped_suffix = if enabled {
                    strong_suffix_to_skip[event_index]
                } else {
                    0
                };
                let spans = if enabled
                    && !(code_depth > 0
                        && settings.format == OutputFormat::Ansi
                        && !settings.skip_code)
                {
                    selected_markdown_text_spans(
                        &events,
                        event_index,
                        text,
                        segmenter,
                        engine,
                        skipped_suffix,
                    )
                } else {
                    Vec::new()
                };
                if range.start <= range.end && range.end <= output_source.len() {
                    let source_text = &output_source[range.clone()];
                    if enabled
                        && code_depth > 0
                        && settings.format == OutputFormat::Ansi
                        && !settings.skip_code
                    {
                        let raw_spans = engine.spans(source_text, segmenter);
                        append_emphasis_edits(source, range.start, raw_spans, settings, &mut edits);
                    } else if let Some(mapped) = map_markdown_text(source_text, text) {
                        let raw_spans = spans
                            .iter()
                            .filter_map(|span| map_markdown_span(&mapped, span))
                            .collect::<Vec<_>>();
                        append_emphasis_edits(source, range.start, raw_spans, settings, &mut edits);
                    }
                }
            }
            Event::Html(_) | Event::InlineHtml(_) if settings.format == OutputFormat::Markdown => {
                if range.start <= range.end
                    && range.end <= output_source.len()
                    && !unsafe_ranges
                        .iter()
                        .any(|blocked| range.start < blocked.end && range.end > blocked.start)
                {
                    let replacement = escape_markdown_text(&output_source[range.clone()]);
                    edits.push((range.clone(), replacement));
                }
            }
            _ => {}
        }
    }
    edits.sort_by(|left, right| right.0.start.cmp(&left.0.start));
    let mut output = output_source.as_bytes().to_vec();
    for (range, replacement) in edits {
        output.splice(range, replacement);
    }
    output
}

fn unsafe_markdown_link_ranges(
    events: &[(Event<'_>, Range<usize>)],
    format: OutputFormat,
) -> Vec<Range<usize>> {
    merge_ranges(if format == OutputFormat::Markdown {
        events
            .iter()
            .filter_map(|(event, range)| match event {
                Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. })
                    if !safe_url(dest_url) =>
                {
                    Some(range.clone())
                }
                _ => None,
            })
            .collect()
    } else {
        Vec::new()
    })
}

fn markdown_fixation_spans(
    events: &[(Event<'_>, Range<usize>)],
    event_index: usize,
    text: &str,
    segmenter: &dyn WordSegmenter,
    engine: &mut FixationEngine,
) -> Vec<Range<usize>> {
    let spans = engine.spans(text, segmenter);
    let blocked = markdown_strong_adjacencies(events, event_index, text);
    spans
        .into_iter()
        .filter(|span| {
            !blocked
                .iter()
                .any(|range| span.start < range.end && span.end > range.start)
        })
        .collect()
}

fn selected_markdown_text_spans(
    events: &[(Event<'_>, Range<usize>)],
    event_index: usize,
    text: &str,
    segmenter: &dyn WordSegmenter,
    engine: &mut FixationEngine,
    skipped_prefix: usize,
) -> Vec<Range<usize>> {
    if skipped_prefix == 0 {
        return markdown_fixation_spans(events, event_index, text, segmenter, engine);
    }

    let blocked = markdown_strong_adjacencies(events, event_index, text);
    engine
        .spans(&text[skipped_prefix..], segmenter)
        .into_iter()
        .map(|span| span.start + skipped_prefix..span.end + skipped_prefix)
        .filter(|span| {
            !blocked
                .iter()
                .any(|range| span.start < range.end && span.end > range.start)
        })
        .collect()
}

fn advance_markdown_strong_text(
    events: &[(Event<'_>, Range<usize>)],
    event_index: usize,
    text: &str,
    segmenter: &dyn WordSegmenter,
    engine: &mut FixationEngine,
) -> Option<(usize, usize)> {
    let continuation = strong_word_continuation(events, event_index, text, segmenter);
    if let Some((suffix_event, suffix_len)) = continuation {
        let Event::Text(suffix) = &events[suffix_event].0 else {
            unreachable!("continuation event was validated as Markdown text");
        };
        let mut combined = String::with_capacity(text.len() + suffix_len);
        combined.push_str(text);
        combined.push_str(&suffix[..suffix_len]);
        let _ = engine.spans(&combined, segmenter);
    } else {
        let _ = engine.spans(text, segmenter);
    }
    continuation
}

fn append_emphasis_edits(
    source: &str,
    offset: usize,
    spans: Vec<Range<usize>>,
    settings: &Settings,
    edits: &mut Vec<(Range<usize>, Vec<u8>)>,
) {
    for span in merge_ranges(spans) {
        let start = offset + span.start;
        let end = offset + span.end;
        if settings.format == OutputFormat::Ansi {
            let rendered = render_ansi_text(
                &source[start..end],
                std::slice::from_ref(&(0..end - start)),
                settings,
            );
            edits.push((start..end, rendered));
        } else {
            edits.push((end..end, b"**".to_vec()));
            edits.push((start..start, b"**".to_vec()));
        }
    }
}

/// Maps parser-visible character boundaries back to the raw Markdown event source.
///
/// Pulldown-cmark intentionally excludes escape backslashes from text event ranges and decodes
/// entities in the event text. Mapping spans instead of replacing event text preserves the
/// document's original escaping and entity spelling.
fn map_markdown_text(source: &str, visible: &str) -> Option<Vec<(Range<usize>, Range<usize>)>> {
    let mut mappings = Vec::new();
    let mut raw_offset = 0;
    let mut visible_offset = 0;

    while raw_offset < source.len() && visible_offset < visible.len() {
        if source.as_bytes()[raw_offset] == b'&' {
            if let Some(relative_end) = source[raw_offset..].find(';').filter(|end| *end <= 64) {
                let raw_end = raw_offset + relative_end + 1;
                let entity = &source[raw_offset..raw_end];
                if let Some(decoded) = decode_markdown_entity(entity) {
                    if visible[visible_offset..].starts_with(&decoded) {
                        let mut decoded_offset = 0;
                        for character in decoded.chars() {
                            let next = decoded_offset + character.len_utf8();
                            mappings.push((
                                visible_offset + decoded_offset..visible_offset + next,
                                raw_offset..raw_end,
                            ));
                            decoded_offset = next;
                        }
                        visible_offset += decoded.len();
                        raw_offset = raw_end;
                        continue;
                    }
                }
            }
        }

        if source.as_bytes()[raw_offset] == b'\\'
            && source[raw_offset + 1..]
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_punctuation())
        {
            raw_offset += 1;
        }

        let raw_character = source[raw_offset..].chars().next()?;
        let visible_character = visible[visible_offset..].chars().next()?;
        if raw_character != visible_character {
            return None;
        }
        let raw_end = raw_offset + raw_character.len_utf8();
        let visible_end = visible_offset + visible_character.len_utf8();
        mappings.push((visible_offset..visible_end, raw_offset..raw_end));
        raw_offset = raw_end;
        visible_offset = visible_end;
    }

    (raw_offset == source.len() && visible_offset == visible.len()).then_some(mappings)
}

fn decode_markdown_entity(entity: &str) -> Option<String> {
    let mut decoded = String::new();
    for event in Parser::new_ext(entity, Options::empty()) {
        if let Event::Text(text) = event {
            decoded.push_str(&text);
        }
    }
    (!decoded.is_empty() && decoded != entity).then_some(decoded)
}

fn map_markdown_span(
    mappings: &[(Range<usize>, Range<usize>)],
    span: &Range<usize>,
) -> Option<Range<usize>> {
    let first = mappings
        .iter()
        .find(|(visible, _)| visible.start == span.start)?;
    let last = mappings
        .iter()
        .find(|(visible, _)| visible.end == span.end)?;
    (first.1.start <= last.1.end).then_some(first.1.start..last.1.end)
}

fn merge_ranges(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.sort_by_key(|range| range.start);
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        if let Some(last) = merged.last_mut() {
            if range.start <= last.end {
                last.end = last.end.max(range.end);
                continue;
            }
        }
        merged.push(range);
    }
    merged
}

fn markdown_strong_adjacencies(
    events: &[(Event<'_>, Range<usize>)],
    event_index: usize,
    text: &str,
) -> Vec<Range<usize>> {
    let mut blocked = Vec::new();
    if event_index > 0
        && matches!(
            &events[event_index - 1].0,
            Event::End(pulldown_cmark::TagEnd::Strong)
        )
    {
        if let Some(prefix) = prefix_word(text) {
            blocked.push(prefix);
        }
    }
    if events
        .get(event_index + 1)
        .is_some_and(|(event, _)| matches!(event, Event::Start(Tag::Strong)))
    {
        if let Some(suffix) = suffix_word(text) {
            blocked.push(suffix);
        }
    }
    blocked
}

fn strong_word_continuation(
    events: &[(Event<'_>, Range<usize>)],
    event_index: usize,
    strong_text: &str,
    segmenter: &dyn WordSegmenter,
) -> Option<(usize, usize)> {
    if !events
        .get(event_index + 1)
        .is_some_and(|(event, _)| matches!(event, Event::End(pulldown_cmark::TagEnd::Strong)))
    {
        return None;
    }

    let suffix_event = event_index + 2;
    let Event::Text(suffix) = &events.get(suffix_event)?.0 else {
        return None;
    };
    let boundary = strong_text.len();
    let mut combined = String::with_capacity(boundary.saturating_add(suffix.len()));
    combined.push_str(strong_text);
    combined.push_str(suffix);
    let suffix_len = segmenter
        .word_ranges(&combined)
        .into_iter()
        .find(|range| range.start < boundary && range.end > boundary)?
        .end
        .saturating_sub(boundary);

    (suffix_len > 0).then_some((suffix_event, suffix_len))
}

fn prefix_word(text: &str) -> Option<Range<usize>> {
    let mut end = 0;
    for (offset, character) in text.char_indices() {
        if !is_word_character(character) {
            break;
        }
        end = offset + character.len_utf8();
    }
    (end > 0).then_some(0..end)
}

fn suffix_word(text: &str) -> Option<Range<usize>> {
    let mut start = text.len();
    for (offset, character) in text.char_indices().rev() {
        if !is_word_character(character) {
            break;
        }
        start = offset;
    }
    (start < text.len()).then_some(start..text.len())
}

fn is_word_character(character: char) -> bool {
    character.is_alphanumeric() || character.is_alphabetic()
}

fn render_markdown_html(
    source: &str,
    settings: &Settings,
    enabled: bool,
    segmenter: &dyn WordSegmenter,
    engine: &mut FixationEngine,
    references: Option<&RefDefs<'_>>,
) -> String {
    let events: Vec<_> = markdown_parser(source, references).collect();
    let mut strong_depth = 0_usize;
    let mut code_depth = 0_usize;
    let mut image_depth = 0_usize;
    let mut strong_suffix_to_skip = vec![0; events.len()];
    let rendered = events.iter().enumerate().map(|(index, (event, _))| {
        match event {
            Event::Start(tag) => {
                match tag {
                    Tag::Strong => strong_depth += 1,
                    Tag::CodeBlock(_) => code_depth += 1,
                    Tag::Image { .. } => image_depth += 1,
                    _ => {}
                }
                let mut tag = tag.clone();
                if let Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. } = &mut tag {
                    if !safe_url(dest_url) {
                        *dest_url = "".into();
                    }
                }
                Event::Start(tag)
            }
            Event::End(tag) => {
                match tag {
                    pulldown_cmark::TagEnd::Strong => strong_depth = strong_depth.saturating_sub(1),
                    pulldown_cmark::TagEnd::CodeBlock => code_depth = code_depth.saturating_sub(1),
                    pulldown_cmark::TagEnd::Image => image_depth = image_depth.saturating_sub(1),
                    _ => {}
                }
                event.clone()
            }
            Event::Text(text)
                if enabled
                    && strong_depth > 0
                    && image_depth == 0
                    && (code_depth == 0 || !settings.skip_code) =>
            {
                if let Some((suffix_event, suffix_len)) =
                    advance_markdown_strong_text(&events, index, text, segmenter, engine)
                {
                    strong_suffix_to_skip[suffix_event] = suffix_len;
                }
                event.clone()
            }
            Event::Text(text)
                if enabled
                    && image_depth == 0
                    && strong_depth == 0
                    && (code_depth == 0 || !settings.skip_code) =>
            {
                let skipped_suffix = strong_suffix_to_skip[index];
                let spans: Vec<Range<usize>> = if skipped_suffix > 0 {
                    engine
                        .spans(&text[skipped_suffix..], segmenter)
                        .into_iter()
                        .map(|span| span.start + skipped_suffix..span.end + skipped_suffix)
                        .collect()
                } else {
                    let blocked = markdown_strong_adjacencies(&events, index, text);
                    engine
                        .spans(text, segmenter)
                        .into_iter()
                        .filter(|span| {
                            !blocked
                                .iter()
                                .any(|range| span.start < range.end && span.end > range.start)
                        })
                        .collect()
                };
                Event::InlineHtml(render_html_text(text, &spans, settings).into())
            }
            Event::Code(text) if enabled && image_depth == 0 && !settings.skip_code => {
                let spans = engine.spans(text, segmenter);
                Event::InlineHtml(
                    format!("<code>{}</code>", render_html_text(text, &spans, settings)).into(),
                )
            }
            // Raw document HTML is data. Only markup generated above reaches the renderer as HTML.
            Event::Html(text) | Event::InlineHtml(text) => Event::Text(text.clone()),
            _ => event.clone(),
        }
    });
    let mut output = String::new();
    pulldown_cmark::html::push_html(&mut output, rendered);
    output
}

fn render_markdown_text(text: &str, spans: &[Range<usize>]) -> Vec<u8> {
    let mut output = Vec::with_capacity(text.len().saturating_add(spans.len().saturating_mul(4)));
    let mut offset = 0;
    for span in spans {
        if span.start < offset || span.end > text.len() || span.start > span.end {
            continue;
        }
        push_markdown_escaped(&mut output, &text[offset..span.start]);
        output.extend_from_slice(b"**");
        push_markdown_escaped(&mut output, &text[span.clone()]);
        output.extend_from_slice(b"**");
        offset = span.end;
    }
    push_markdown_escaped(&mut output, &text[offset..]);
    output
}

fn push_markdown_escaped(output: &mut Vec<u8>, text: &str) {
    for character in text.chars() {
        if character.is_ascii_punctuation() {
            output.push(b'\\');
        }
        let mut buffer = [0_u8; 4];
        output.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
    }
}

fn escape_markdown_text(text: &str) -> Vec<u8> {
    let mut output = Vec::with_capacity(text.len().saturating_mul(2));
    push_markdown_escaped(&mut output, text);
    output
}

fn render_html_text(text: &str, spans: &[Range<usize>], settings: &Settings) -> String {
    let mut output =
        String::with_capacity(text.len().saturating_add(spans.len().saturating_mul(20)));
    push_html_text(&mut output, text, spans, settings);
    output
}

fn push_html_text(output: &mut String, text: &str, spans: &[Range<usize>], settings: &Settings) {
    let (open, close) = html_emphasis(settings);
    let mut offset = 0;
    for span in spans {
        if span.start < offset || span.end > text.len() || span.start > span.end {
            continue;
        }
        escape_html_into(output, &text[offset..span.start]);
        output.push_str(open);
        escape_html_into(output, &text[span.clone()]);
        output.push_str(close);
        offset = span.end;
    }
    escape_html_into(output, &text[offset..]);
}

fn html_emphasis(settings: &Settings) -> (&'static str, &'static str) {
    match settings.emphasis {
        crate::settings::Emphasis::Bold => ("<strong>", "</strong>"),
        crate::settings::Emphasis::Underline => ("<u>", "</u>"),
        crate::settings::Emphasis::Reverse => ("<span class=\"bionic-reverse\">", "</span>"),
    }
}

fn render_ansi_text(text: &str, spans: &[Range<usize>], settings: &Settings) -> Vec<u8> {
    let (open, close) = ansi_emphasis(settings);
    let mut output = Vec::with_capacity(text.len().saturating_add(spans.len().saturating_mul(12)));
    let mut offset = 0;
    for span in spans {
        if span.start < offset || span.end > text.len() || span.start > span.end {
            continue;
        }
        output.extend_from_slice(&text.as_bytes()[offset..span.start]);
        output.extend_from_slice(open);
        output.extend_from_slice(&text.as_bytes()[span.clone()]);
        output.extend_from_slice(close);
        offset = span.end;
    }
    output.extend_from_slice(&text.as_bytes()[offset..]);
    output
}

fn ansi_emphasis(settings: &Settings) -> (&'static [u8], &'static [u8]) {
    match settings.emphasis {
        crate::settings::Emphasis::Bold => (b"\x1b[1m", b"\x1b[22m"),
        crate::settings::Emphasis::Underline => (b"\x1b[4m", b"\x1b[24m"),
        crate::settings::Emphasis::Reverse => (b"\x1b[7m", b"\x1b[27m"),
    }
}

fn escape_html(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    escape_html_into(&mut output, text);
    output
}

fn escape_html_into(output: &mut String, text: &str) {
    for character in text.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#39;"),
            '\0' => output.push_str("&#xfffd;"),
            _ => output.push(character),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ControlFilter, safe_osc8};
    use crate::settings::ControlPolicy;

    #[test]
    fn filters_unsafe_controls_across_chunks() {
        let mut filter = ControlFilter::new(ControlPolicy::Safe);
        let mut output = filter.push(b"a\x1b]52;c;secret");
        output.extend(filter.push(b"\x07b\x1b[2Jc\x1b[31mred"));
        output.extend(filter.finish());
        assert_eq!(output, b"abc\x1b[31mred");
    }

    #[test]
    fn retains_only_sgr_and_safe_osc8() {
        let mut filter = ControlFilter::new(ControlPolicy::Safe);
        let input = b"\x1b[1mword\x1b]8;;https://example.test\x1b\\link\x1b]8;;\x1b\\";
        let mut output = filter.push(input);
        output.extend(filter.finish());
        assert_eq!(output, input);
        assert!(safe_osc8(b"\x1b]8;;https://example.test\x07"));
        assert!(!safe_osc8(b"\x1b]8;;javascript:alert(1)\x07"));
    }

    #[test]
    fn preserves_invalid_utf8_bytes() {
        let mut filter = ControlFilter::new(ControlPolicy::Safe);
        let mut output = filter.push(&[b'a', 0xff, 0x1b, b'[', b'2', b'J', b'b']);
        output.extend(filter.finish());
        assert_eq!(output, [b'a', 0xff, b'b']);
    }

    #[test]
    fn drops_incomplete_control_at_finish() {
        let mut filter = ControlFilter::new(ControlPolicy::Safe);
        let mut output = filter.push(b"text\x1b[31");
        output.extend(filter.finish());
        assert_eq!(output, b"text");
    }

    #[test]
    fn bytewise_control_scanning_does_not_recheck_the_entire_prefix() {
        let mut osc = b"\x1b]8;;https://example.test/".to_vec();
        osc.extend(std::iter::repeat_n(b'x', 4000));
        osc.push(0x07);

        let mut csi = b"\x1b[".to_vec();
        csi.extend(std::iter::repeat_n(b'1', 4000));
        csi.push(b'm');

        for input in [&osc, &csi] {
            super::CONTROL_SCAN_STEPS.with(|steps| steps.set(0));
            let mut filter = ControlFilter::new(ControlPolicy::Safe);
            let mut output = Vec::new();
            for byte in input {
                output.extend(filter.push(std::slice::from_ref(byte)));
            }
            output.extend(filter.finish());
            assert_eq!(output, *input);
            let scan_steps = super::CONTROL_SCAN_STEPS.with(std::cell::Cell::get);
            assert!(
                scan_steps <= input.len().saturating_mul(5),
                "scanned {} positions for {} input bytes",
                scan_steps,
                input.len()
            );
        }
    }
}

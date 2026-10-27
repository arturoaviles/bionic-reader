//! Native incremental document processing, including temporary-file Markdown spooling.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::ops::Range;
use std::str;

use pulldown_cmark::{Options, Parser};

use super::{
    ControlFilter, effective_control_policy, escape_html, escape_markdown_text, render_html_text,
    render_markdown_text, render_markdown_with_engine, validate_format_combination,
};
use crate::Error;
use crate::segmentation::{FixationEngine, UnicodeSegmenter, WordSegmenter};
use crate::settings::{ControlPolicy, InputFormat, OutputFormat, Settings};
use crate::transformer::Transformer;

const MAX_MARKDOWN_BLOCK_BYTES: usize = 1024 * 1024;
const MAX_MARKDOWN_REFERENCE_BYTES: usize = 1024 * 1024;
const MAX_TEXT_WORD_BYTES: usize = 64 * 1024;
const STREAM_CHUNK_BYTES: usize = 16 * 1024;

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

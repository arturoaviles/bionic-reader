//! Interactive reading of a document in the controlling terminal.
//!
//! The document is spooled to a temporary file and indexed by line start, so
//! the reader does not retain the whole document in memory. Terminal controls
//! from document contents are removed before rendering.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::ops::Range;
use std::os::fd::AsRawFd;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use pulldown_cmark::{Event, LinkType, Options, Parser, Tag};
use unicode_segmentation::{GraphemeCursor, GraphemeIncomplete, UnicodeSegmentation};
use unicode_width::UnicodeWidthStr;

use crate::Error;
use crate::document::ControlFilter;
use crate::settings::{Emphasis, InputFormat, Profile, Settings};
use crate::transformer::Transformer;

const INPUT_BUFFER_LIMIT: usize = 4096;
const SEARCH_QUERY_LIMIT: usize = 4096;
const IO_BUFFER_SIZE: usize = 32 * 1024;
const MAX_MARKDOWN_BLOCK_BYTES: usize = 1024 * 1024;
const TAB_STOP: usize = 8;

/// Runs the interactive reader using the supplied input stream as the document.
///
/// Input is spooled to a temporary file before the controlling terminal is
/// opened. This allows a pipeline to supply the document through standard input
/// while keys and screen output use `/dev/tty`.
///
/// # Errors
///
/// Returns an error when settings are invalid, the input cannot be spooled, or
/// the controlling terminal cannot be opened or used.
pub fn run(input: &mut dyn Read, settings: Settings) -> Result<(), Error> {
    settings.validate()?;
    if is_dumb_terminal(std::env::var("TERM").ok().as_deref()) {
        return Err(Error::new(
            "interactive reading needs a capable terminal (TERM=dumb); use `bionic-reader preview --print` for plain output".to_owned(),
        ));
    }
    let file = spool_document(input)?;
    let mut document = Document::new(
        file,
        settings.input_format == InputFormat::Markdown,
        settings.skip_code,
    )?;
    let signals = SignalWatcher::new().map_err(|error| {
        Error::with_source("could not watch terminal signals".to_owned(), error)
    })?;
    let mut terminal = Terminal::open()?;
    terminal.enter_screen()?;

    let mut reader = Reader::new(settings);
    reader.event_loop(&mut terminal, &mut document, &signals)
}

fn is_dumb_terminal(term: Option<&str>) -> bool {
    term == Some("dumb")
}

fn spool_document(input: &mut dyn Read) -> Result<File, Error> {
    let mut spool = tempfile::tempfile()
        .map_err(|error| Error::with_source("could not create document spool".to_owned(), error))?;
    let mut plain_text = PlainTextFilter::default();
    let mut buffer = vec![0_u8; IO_BUFFER_SIZE];

    loop {
        let count = input
            .read(&mut buffer)
            .map_err(|error| Error::with_source("could not read document".to_owned(), error))?;
        if count == 0 {
            break;
        }
        let safe = plain_text.push(&buffer[..count]);
        spool
            .write_all(&safe)
            .map_err(|error| Error::with_source("could not spool document".to_owned(), error))?;
    }

    let tail = plain_text.finish();
    spool
        .write_all(&tail)
        .map_err(|error| Error::with_source("could not finish document spool".to_owned(), error))?;
    spool
        .seek(SeekFrom::Start(0))
        .map_err(|error| Error::with_source("could not rewind document spool".to_owned(), error))?;
    Ok(spool)
}

#[derive(Debug)]
struct PlainTextFilter {
    controls: ControlFilter,
    utf8_pending: Vec<u8>,
    utf8_expected: usize,
    pending_cr: bool,
}

impl Default for PlainTextFilter {
    fn default() -> Self {
        Self {
            controls: ControlFilter::plain_text(),
            utf8_pending: Vec::new(),
            utf8_expected: 0,
            pending_cr: false,
        }
    }
}

impl PlainTextFilter {
    fn push(&mut self, input: &[u8]) -> Vec<u8> {
        let filtered = self.controls.push(input);
        let mut output = Vec::with_capacity(filtered.len());
        for byte in filtered {
            self.push_text_byte(byte, &mut output);
        }
        output
    }

    fn finish(&mut self) -> Vec<u8> {
        let tail = self.controls.finish();
        let mut output = Vec::new();
        for byte in tail {
            self.push_text_byte(byte, &mut output);
        }
        if !self.utf8_pending.is_empty() {
            output.extend_from_slice("\u{fffd}".as_bytes());
        }
        self.utf8_pending.clear();
        self.utf8_expected = 0;
        if self.pending_cr {
            output.extend_from_slice(b"^M");
            self.pending_cr = false;
        }
        output
    }

    fn push_text_byte(&mut self, byte: u8, output: &mut Vec<u8>) {
        if self.pending_cr {
            self.pending_cr = false;
            if byte != b'\n' {
                output.extend_from_slice(b"^M");
            }
        }
        if !self.utf8_pending.is_empty() {
            if (0x80..=0xbf).contains(&byte) {
                self.utf8_pending.push(byte);
                if self.utf8_pending.len() == self.utf8_expected {
                    if let Ok(character) = std::str::from_utf8(&self.utf8_pending) {
                        if !character.chars().all(char::is_control) {
                            output.extend_from_slice(&self.utf8_pending);
                        }
                    } else {
                        output.extend_from_slice("\u{fffd}".as_bytes());
                    }
                    self.utf8_pending.clear();
                    self.utf8_expected = 0;
                }
                return;
            }
            output.extend_from_slice("\u{fffd}".as_bytes());
            self.utf8_pending.clear();
            self.utf8_expected = 0;
            self.push_text_byte(byte, output);
            return;
        }
        match byte {
            b'\n' | b'\t' => output.push(byte),
            b'\r' => self.pending_cr = true,
            0x00..=0x1f | 0x7f | 0x80..=0x9f => {}
            0xc2..=0xdf => {
                self.utf8_pending.push(byte);
                self.utf8_expected = 2;
            }
            0xe0..=0xef => {
                self.utf8_pending.push(byte);
                self.utf8_expected = 3;
            }
            0xf0..=0xf4 => {
                self.utf8_pending.push(byte);
                self.utf8_expected = 4;
            }
            0xa0..=0xc1 | 0xf5..=0xff => output.extend_from_slice("\u{fffd}".as_bytes()),
            _ => output.push(byte),
        }
    }
}

#[derive(Debug)]
struct Document {
    file: File,
    line_starts: Vec<u64>,
    protected_ranges: Vec<Range<u64>>,
    length: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Position {
    line: usize,
    offset: u64,
}

#[derive(Debug)]
struct DisplayRow {
    text: String,
    protected_ranges: Vec<Range<usize>>,
    next: Option<Position>,
}

impl Document {
    fn new(mut file: File, markdown: bool, skip_code: bool) -> Result<Self, Error> {
        let length = file
            .metadata()
            .map_err(|error| {
                Error::with_source("could not inspect document spool".to_owned(), error)
            })?
            .len();
        let mut line_starts = vec![0_u64];
        let mut offset = 0_u64;
        let mut buffer = vec![0_u8; IO_BUFFER_SIZE];
        loop {
            let count = file.read(&mut buffer).map_err(|error| {
                Error::with_source("could not index document".to_owned(), error)
            })?;
            if count == 0 {
                break;
            }
            for (index, byte) in buffer[..count].iter().enumerate() {
                if *byte == b'\n' {
                    let after_newline = offset
                        .checked_add(u64::try_from(index).expect("buffer index fits in u64"))
                        .and_then(|value| value.checked_add(1))
                        .expect("spooled document length fits in u64");
                    line_starts.push(after_newline);
                }
            }
            offset = offset
                .checked_add(u64::try_from(count).expect("buffer length fits in u64"))
                .expect("spooled document length fits in u64");
        }
        file.seek(SeekFrom::Start(0)).map_err(|error| {
            Error::with_source("could not rewind document index".to_owned(), error)
        })?;
        let protected_ranges = if markdown {
            index_markdown_protected_ranges(&mut file, skip_code)?
        } else {
            Vec::new()
        };
        file.seek(SeekFrom::Start(0)).map_err(|error| {
            Error::with_source("could not rewind document index".to_owned(), error)
        })?;
        Ok(Self {
            file,
            line_starts,
            protected_ranges,
            length,
        })
    }

    fn start() -> Position {
        Position { line: 0, offset: 0 }
    }

    fn line_range(&self, line: usize) -> (u64, u64) {
        let start = self.line_starts[line];
        let raw_end = self
            .line_starts
            .get(line + 1)
            .map_or(self.length, |next_start| next_start.saturating_sub(1));
        (start, raw_end.max(start))
    }

    fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    fn row(&mut self, position: Position, columns: usize) -> Result<DisplayRow, Error> {
        let line = position.line.min(self.line_starts.len().saturating_sub(1));
        let (line_start, line_end) = self.line_range(line);
        let offset = position.offset.clamp(line_start, line_end);
        if offset >= line_end {
            return Ok(DisplayRow {
                text: String::new(),
                protected_ranges: Vec::new(),
                next: self.next_line(line),
            });
        }

        let file = self
            .file
            .try_clone()
            .map_err(|error| Error::with_source("could not read document row".to_owned(), error))?;
        let mut graphemes = GraphemeReader::new(file, offset, line_end).map_err(|error| {
            Error::with_source("could not seek to document row".to_owned(), error)
        })?;
        let mut text = String::new();
        let mut protected_ranges: Vec<Range<usize>> = Vec::new();
        let mut cells = 0_usize;
        let width = columns.max(1);
        let mut next_offset = offset;

        while let Some(grapheme) = graphemes
            .next_grapheme()
            .map_err(|error| Error::with_source("could not read document text".to_owned(), error))?
        {
            if !text.is_empty()
                && text.len().saturating_add(grapheme.text.len())
                    > MAX_VIEW_GRAPHEME_BYTES.saturating_mul(width)
            {
                next_offset = grapheme.start;
                break;
            }
            let is_tab = grapheme.text == "\t";
            let grapheme_width = if is_tab {
                TAB_STOP - (cells % TAB_STOP)
            } else {
                UnicodeWidthStr::width(grapheme.text.as_str())
            };
            if cells > 0 && cells.saturating_add(grapheme_width) > width {
                next_offset = grapheme.start;
                break;
            }
            let is_protected = self.is_protected(grapheme.start);
            let text_start = text.len();
            if is_tab {
                let spaces = grapheme_width.min(width.saturating_sub(cells));
                text.extend(std::iter::repeat_n(' ', spaces));
                cells = cells.saturating_add(spaces);
            } else if grapheme_width > width {
                text.push('\u{fffd}');
                cells = cells.saturating_add(1);
            } else {
                text.push_str(&grapheme.text);
                cells = cells.saturating_add(grapheme_width);
            }
            if is_protected && text_start < text.len() {
                if let Some(last) = protected_ranges.last_mut() {
                    if last.end == text_start {
                        last.end = text.len();
                    } else {
                        protected_ranges.push(text_start..text.len());
                    }
                } else {
                    protected_ranges.push(text_start..text.len());
                }
            }
            next_offset = grapheme.end;
        }

        let next = if next_offset < line_end {
            Some(Position {
                line,
                offset: next_offset,
            })
        } else {
            self.next_line(line)
        };
        Ok(DisplayRow {
            text,
            protected_ranges,
            next,
        })
    }

    fn is_protected(&self, offset: u64) -> bool {
        let index = self
            .protected_ranges
            .partition_point(|range| range.end <= offset);
        self.protected_ranges
            .get(index)
            .is_some_and(|range| range.start <= offset && offset < range.end)
    }

    fn next_line(&self, line: usize) -> Option<Position> {
        let next_line = line.checked_add(1)?;
        let offset = *self.line_starts.get(next_line)?;
        Some(Position {
            line: next_line,
            offset,
        })
    }

    fn move_down(
        &mut self,
        mut position: Position,
        rows: usize,
        columns: usize,
    ) -> Result<Position, Error> {
        for _ in 0..rows {
            let Some(next) = self.row(position, columns)?.next else {
                break;
            };
            position = next;
        }
        Ok(position)
    }

    fn move_up(&mut self, position: Position, columns: usize) -> Result<Position, Error> {
        self.move_up_one(position, columns)
    }

    fn move_up_rows(
        &mut self,
        mut position: Position,
        rows: usize,
        columns: usize,
    ) -> Result<Position, Error> {
        for _ in 0..rows {
            position = self.move_up_one(position, columns)?;
        }
        Ok(position)
    }

    fn move_up_one(&mut self, position: Position, columns: usize) -> Result<Position, Error> {
        let line_start = self.line_starts[position.line];
        if position.offset == line_start {
            if position.line == 0 {
                return Ok(position);
            }
            return self.last_row_of_line(position.line - 1, columns);
        }

        let mut candidate = Position {
            line: position.line,
            offset: line_start,
        };
        let mut previous = candidate;
        loop {
            let row = self.row(candidate, columns)?;
            let Some(next) = row.next else {
                return Ok(previous);
            };
            if next.line != position.line || next.offset >= position.offset {
                return Ok(candidate);
            }
            previous = candidate;
            candidate = next;
        }
    }

    fn last_row_of_line(&mut self, line: usize, columns: usize) -> Result<Position, Error> {
        let mut position = Position {
            line,
            offset: self.line_starts[line],
        };
        let mut last = position;
        loop {
            let row = self.row(position, columns)?;
            let Some(next) = row.next else {
                return Ok(last);
            };
            if next.line != line {
                return Ok(last);
            }
            last = next;
            position = next;
        }
    }

    fn last_position(&mut self, columns: usize) -> Result<Position, Error> {
        self.last_row_of_line(self.line_starts.len().saturating_sub(1), columns)
    }

    fn position_for_offset(&mut self, offset: u64, columns: usize) -> Result<Position, Error> {
        let insertion = self.line_starts.partition_point(|start| *start <= offset);
        let line = insertion
            .saturating_sub(1)
            .min(self.line_starts.len().saturating_sub(1));
        let mut position = Position {
            line,
            offset: self.line_starts[line],
        };
        loop {
            let row = self.row(position, columns)?;
            let Some(next) = row.next else {
                return Ok(position);
            };
            if next.line != line || next.offset > offset {
                return Ok(position);
            }
            position = next;
        }
    }

    fn find_forward(&mut self, query: &[u8], start: u64) -> Result<Option<u64>, Error> {
        if query.is_empty() || start >= self.length {
            return Ok(None);
        }
        search_forward(&mut self.file, query, start, self.length)
            .map_err(|error| Error::with_source("could not search document".to_owned(), error))
    }

    fn find_last_before(&mut self, query: &[u8], before: u64) -> Result<Option<u64>, Error> {
        if query.is_empty() || before == 0 {
            return Ok(None);
        }
        search_last_before(&mut self.file, query, before.min(self.length))
            .map_err(|error| Error::with_source("could not search document".to_owned(), error))
    }
}

#[derive(Clone, Copy, Debug)]
struct FenceMarker {
    character: u8,
    length: usize,
}

fn index_markdown_protected_ranges(
    file: &mut File,
    protect_code: bool,
) -> Result<Vec<Range<u64>>, Error> {
    file.seek(SeekFrom::Start(0)).map_err(|error| {
        Error::with_source("could not rewind Markdown document".to_owned(), error)
    })?;
    let mut input = BufReader::with_capacity(
        IO_BUFFER_SIZE,
        file.try_clone().map_err(|error| {
            Error::with_source("could not index Markdown document".to_owned(), error)
        })?,
    );
    let mut ranges = Vec::new();
    let mut block = Vec::new();
    let mut line = Vec::new();
    let mut block_start = 0_u64;
    let mut offset = 0_u64;
    let mut fence = None;
    let mut block_oversized = false;
    let mut line_oversized = false;
    let mut buffer = vec![0_u8; IO_BUFFER_SIZE];

    loop {
        let count = input.read(&mut buffer).map_err(|error| {
            Error::with_source("could not read Markdown document".to_owned(), error)
        })?;
        if count == 0 {
            break;
        }
        for byte in &buffer[..count] {
            if !line_oversized {
                if line.len() < MAX_MARKDOWN_BLOCK_BYTES {
                    line.push(*byte);
                } else {
                    line_oversized = true;
                }
            }
            offset = offset.saturating_add(1);
            if *byte != b'\n' {
                continue;
            }

            append_markdown_line(&line, line_oversized, &mut block, &mut block_oversized);
            let blank = !line_oversized && markdown_blank_line(&line);
            update_markdown_fence(&line, &mut fence);
            if fence.is_none() && blank {
                finish_markdown_block(
                    &block,
                    block_oversized,
                    block_start,
                    offset,
                    protect_code,
                    &mut ranges,
                )?;
                block.clear();
                block_oversized = false;
                block_start = offset;
            }
            line.clear();
            line_oversized = false;
        }
    }

    if !line.is_empty() || line_oversized {
        append_markdown_line(&line, line_oversized, &mut block, &mut block_oversized);
        update_markdown_fence(&line, &mut fence);
    }
    if block_start < offset {
        finish_markdown_block(
            &block,
            block_oversized,
            block_start,
            offset,
            protect_code,
            &mut ranges,
        )?;
    }
    Ok(merge_byte_ranges(ranges))
}

fn append_markdown_line(
    line: &[u8],
    line_oversized: bool,
    block: &mut Vec<u8>,
    block_oversized: &mut bool,
) {
    if *block_oversized {
        return;
    }
    if line_oversized || block.len().saturating_add(line.len()) > MAX_MARKDOWN_BLOCK_BYTES {
        *block_oversized = true;
        block.clear();
    } else {
        block.extend_from_slice(line);
    }
}

fn finish_markdown_block(
    block: &[u8],
    oversized: bool,
    start: u64,
    end: u64,
    protect_code: bool,
    ranges: &mut Vec<Range<u64>>,
) -> Result<(), Error> {
    if start >= end {
        return Ok(());
    }
    if oversized {
        ranges.push(start..end);
        return Ok(());
    }
    let source = std::str::from_utf8(block).map_err(|error| {
        Error::invalid_input_with_source("Markdown input must be valid UTF-8".to_owned(), error)
    })?;
    let mut local_ranges = Vec::new();
    for (event, range) in Parser::new_ext(source, Options::empty()).into_offset_iter() {
        match event {
            Event::Code(_) | Event::Start(Tag::CodeBlock(_)) if protect_code => {
                local_ranges.push(range);
            }
            Event::Start(Tag::Link { link_type, .. } | Tag::Image { link_type, .. }) => {
                match link_type {
                    LinkType::Inline => {
                        if let Some(destination) = markdown_link_destination(source, range) {
                            local_ranges.push(destination);
                        }
                    }
                    LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut => {}
                    _ => local_ranges.push(range),
                }
            }
            _ => {}
        }
    }
    local_ranges.extend(markdown_reference_destinations(source));
    for range in merge_usize_ranges(local_ranges) {
        ranges.push(
            start.saturating_add(u64::try_from(range.start).expect("Markdown offset fits in u64"))
                ..start
                    .saturating_add(u64::try_from(range.end).expect("Markdown offset fits in u64")),
        );
    }
    Ok(())
}

fn markdown_link_destination(source: &str, range: Range<usize>) -> Option<Range<usize>> {
    let bytes = source.as_bytes();
    let mut cursor = range.start;
    let mut bracket_depth = 0_usize;
    let mut parenthesis = None;
    while cursor < range.end {
        match bytes[cursor] {
            b'\\' => cursor = cursor.saturating_add(2),
            b'[' => {
                bracket_depth = bracket_depth.saturating_add(1);
                cursor += 1;
            }
            b']' => {
                bracket_depth = bracket_depth.saturating_sub(1);
                if bracket_depth == 0 && bytes.get(cursor + 1) == Some(&b'(') {
                    parenthesis = Some(cursor + 2);
                    break;
                }
                cursor += 1;
            }
            _ => cursor += 1,
        }
    }
    let mut start = parenthesis?;
    while start < range.end && bytes[start].is_ascii_whitespace() {
        start += 1;
    }
    if bytes.get(start) == Some(&b'<') {
        let content_start = start + 1;
        let mut end = content_start;
        while end < range.end {
            if bytes[end] == b'\\' {
                end = end.saturating_add(2);
            } else if bytes[end] == b'>' {
                return (content_start < end).then_some(content_start..end);
            } else {
                end += 1;
            }
        }
        return None;
    }

    let content_start = start;
    let mut nested_parentheses = 0_usize;
    while start < range.end {
        match bytes[start] {
            b'\\' => start = start.saturating_add(2),
            b'(' => {
                nested_parentheses = nested_parentheses.saturating_add(1);
                start += 1;
            }
            b')' if nested_parentheses == 0 => break,
            b')' => {
                nested_parentheses = nested_parentheses.saturating_sub(1);
                start += 1;
            }
            byte if byte.is_ascii_whitespace() && nested_parentheses == 0 => break,
            _ => start += 1,
        }
    }
    (content_start < start).then_some(content_start..start)
}

fn markdown_reference_destinations(source: &str) -> Vec<Range<usize>> {
    let parser = Parser::new_ext(source, Options::all());
    parser
        .reference_definitions()
        .iter()
        .map(|(_label, definition)| definition.span.clone())
        .collect()
}

fn markdown_blank_line(line: &[u8]) -> bool {
    line.iter()
        .all(|byte| matches!(*byte, b' ' | b'\t' | b'\r' | b'\n'))
}

fn update_markdown_fence(line: &[u8], fence: &mut Option<FenceMarker>) {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    let indent = line.iter().take_while(|byte| **byte == b' ').count();
    if indent > 3 || indent == line.len() {
        return;
    }
    let rest = &line[indent..];
    if let Some(open) = fence {
        let length = rest
            .iter()
            .take_while(|byte| **byte == open.character)
            .count();
        if length >= open.length
            && rest[length..]
                .iter()
                .all(|byte| matches!(*byte, b' ' | b'\t'))
        {
            *fence = None;
        }
    } else if matches!(rest.first().copied(), Some(b'`' | b'~')) {
        let character = rest[0];
        let length = rest.iter().take_while(|byte| **byte == character).count();
        if length >= 3 && (character != b'`' || !rest[length..].contains(&b'`')) {
            *fence = Some(FenceMarker { character, length });
        }
    }
}

fn merge_usize_ranges(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
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

fn merge_byte_ranges(mut ranges: Vec<Range<u64>>) -> Vec<Range<u64>> {
    ranges.sort_by_key(|range| range.start);
    let mut merged: Vec<Range<u64>> = Vec::with_capacity(ranges.len());
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

#[derive(Debug)]
struct Grapheme {
    text: String,
    start: u64,
    end: u64,
}

const MAX_VIEW_GRAPHEME_BYTES: usize = 4096;

#[derive(Debug)]
struct GraphemeReader {
    file: File,
    origin: u64,
    length: usize,
    cursor: GraphemeCursor,
    chunk: String,
    chunk_start: usize,
}

impl GraphemeReader {
    fn new(file: File, offset: u64, line_end: u64) -> io::Result<Self> {
        let length = usize::try_from(line_end.saturating_sub(offset))
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let mut reader = Self {
            file,
            origin: offset,
            length,
            cursor: GraphemeCursor::new(0, length, true),
            chunk: String::new(),
            chunk_start: 0,
        };
        reader.chunk = reader.read_chunk(0, length.min(IO_BUFFER_SIZE), false)?;
        Ok(reader)
    }

    fn read_chunk(&mut self, start: usize, end: usize, context: bool) -> io::Result<String> {
        self.file
            .seek(SeekFrom::Start(self.origin + start as u64))?;
        let mut bytes = vec![0; end - start];
        self.file.read_exact(&mut bytes)?;
        if context {
            let skip = bytes
                .iter()
                .take_while(|byte| (**byte & 0xc0) == 0x80)
                .count();
            bytes.drain(..skip);
        } else if end < self.length {
            // A file read may end inside a scalar. Retain that scalar for the next chunk.
            if let Err(error) = std::str::from_utf8(&bytes) {
                if error.error_len().is_none() {
                    bytes.truncate(error.valid_up_to());
                }
            }
        }
        String::from_utf8(bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    fn next_grapheme(&mut self) -> io::Result<Option<Grapheme>> {
        let start = self.cursor.cur_cursor();
        if start == self.length {
            return Ok(None);
        }
        let mut copied = start;
        let mut text = String::new();
        let mut oversized = false;
        loop {
            let boundary = self.cursor.next_boundary(&self.chunk, self.chunk_start);
            let end = match boundary {
                Ok(Some(end)) => end,
                Ok(None) => self.length,
                Err(GraphemeIncomplete::NextChunk) => self.chunk_start + self.chunk.len(),
                Err(GraphemeIncomplete::PreContext(end)) => {
                    let context_start = end.saturating_sub(IO_BUFFER_SIZE);
                    let context = self.read_chunk(context_start, end, true)?;
                    self.cursor.provide_context(&context, end - context.len());
                    continue;
                }
                Err(_) => return Err(io::Error::other("invalid document grapheme boundary")),
            };
            if !oversized {
                if text.len().saturating_add(end - copied) > MAX_VIEW_GRAPHEME_BYTES {
                    text.clear();
                    oversized = true;
                } else {
                    text.push_str(&self.chunk[copied - self.chunk_start..end - self.chunk_start]);
                }
            }
            copied = end;
            if boundary.is_ok() {
                return Ok(Some(Grapheme {
                    text: if oversized {
                        "\u{fffd}".to_owned()
                    } else {
                        text
                    },
                    start: self.origin + start as u64,
                    end: self.origin + end as u64,
                }));
            }
            self.chunk_start = end;
            self.chunk = self.read_chunk(end, self.length.min(end + IO_BUFFER_SIZE), false)?;
        }
    }
}

#[derive(Debug)]
struct Reader {
    settings: Settings,
    position: Position,
    search_query: String,
    search_match: Option<u64>,
    search_input: Option<String>,
    show_help: bool,
    show_settings: bool,
    keys: KeyBuffer,
}

impl Reader {
    fn new(settings: Settings) -> Self {
        Self {
            settings,
            position: Position { line: 0, offset: 0 },
            search_query: String::new(),
            search_match: None,
            search_input: None,
            show_help: false,
            show_settings: true,
            keys: KeyBuffer::default(),
        }
    }

    fn event_loop(
        &mut self,
        terminal: &mut Terminal,
        document: &mut Document,
        signals: &SignalWatcher,
    ) -> Result<(), Error> {
        let mut dimensions = terminal.dimensions();
        terminal.draw(document, self, dimensions)?;
        loop {
            if signals.quit.load(Ordering::Relaxed) {
                return Ok(());
            }
            let mut bytes = [0_u8; 256];
            match terminal.input.read(&mut bytes) {
                Ok(count) if count > 0 => self.keys.push(&bytes[..count]),
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => {
                    return Err(Error::with_source(
                        "could not read terminal input".to_owned(),
                        error,
                    ));
                }
            }

            let new_dimensions = terminal.dimensions();
            let resized = new_dimensions != dimensions;
            dimensions = new_dimensions;
            if resized {
                self.position =
                    document.position_for_offset(self.position.offset, dimensions.columns)?;
            }
            let mut changed = resized;
            while let Some(key) = self.keys.next_key(Instant::now()) {
                if self.handle_key(key, document, dimensions)? {
                    return Ok(());
                }
                changed = true;
            }
            if changed {
                terminal.draw(document, self, dimensions)?;
            }
        }
    }

    fn handle_key(
        &mut self,
        key: Key,
        document: &mut Document,
        dimensions: Dimensions,
    ) -> Result<bool, Error> {
        if let Some(query) = &mut self.search_input {
            match key {
                Key::Escape => self.search_input = None,
                Key::Enter => {
                    self.search_query = std::mem::take(query);
                    self.search_input = None;
                    self.search_match = None;
                    self.search_next(document, dimensions.columns, false)?;
                }
                Key::Backspace => {
                    query.pop();
                }
                Key::Character(character) if !character.is_control() => {
                    if query.len().saturating_add(character.len_utf8()) <= SEARCH_QUERY_LIMIT {
                        query.push(character);
                    }
                }
                _ => {}
            }
            return Ok(false);
        }

        match key {
            Key::Escape | Key::ControlC | Key::Character('q') => return Ok(true),
            Key::Up | Key::Character('k') => {
                self.position = document.move_up(self.position, dimensions.columns)?;
            }
            Key::Down | Key::Character('j') => {
                self.position = document.move_down(self.position, 1, dimensions.columns)?;
            }
            Key::PageUp | Key::Character('b') => {
                self.position = document.move_up_rows(
                    self.position,
                    dimensions.content_rows().max(1),
                    dimensions.columns,
                )?;
            }
            Key::PageDown | Key::Character(' ') => {
                self.position = document.move_down(
                    self.position,
                    dimensions.content_rows().max(1),
                    dimensions.columns,
                )?;
            }
            Key::Home | Key::Character('g') => self.position = Document::start(),
            Key::End | Key::Character('G') => {
                self.position = document.last_position(dimensions.columns)?;
            }
            Key::Character('/') => self.search_input = Some(String::new()),
            Key::Character('n') => self.search_next(document, dimensions.columns, false)?,
            Key::Character('N') => self.search_next(document, dimensions.columns, true)?,
            Key::Character('+') => {
                self.settings.fraction = (self.settings.fraction + 0.05).min(1.0);
                self.settings.profile = None;
            }
            Key::Character('-') => {
                self.settings.fraction = (self.settings.fraction - 0.05).max(0.0);
                self.settings.profile = None;
            }
            Key::Character('[') => {
                self.settings.every = self.settings.every.saturating_sub(1).max(1);
                self.settings.profile = None;
            }
            Key::Character(']') => {
                self.settings.every = self.settings.every.saturating_add(1);
                self.settings.profile = None;
            }
            Key::Character('m') => {
                self.settings.min_word_length =
                    self.settings.min_word_length.saturating_sub(1).max(1);
                self.settings.profile = None;
            }
            Key::Character('M') => {
                self.settings.min_word_length = self.settings.min_word_length.saturating_add(1);
                self.settings.profile = None;
            }
            Key::Character('p') => self.cycle_profile(),
            Key::Character('s') => self.show_settings = !self.show_settings,
            Key::Character('?') => self.show_help = !self.show_help,
            _ => {}
        }
        Ok(false)
    }

    fn cycle_profile(&mut self) {
        let next = match self.settings.profile.as_ref() {
            Some(Profile::Subtle) => Profile::Balanced,
            Some(Profile::Balanced) => Profile::Strong,
            Some(Profile::Strong) | None => Profile::Subtle,
        };
        self.settings.apply_profile(next);
    }

    fn search_next(
        &mut self,
        document: &mut Document,
        columns: usize,
        backwards: bool,
    ) -> Result<(), Error> {
        if self.search_query.is_empty() {
            return Ok(());
        }
        let query = self.search_query.as_bytes();
        let found = if backwards {
            let before = self.search_match.unwrap_or(self.position.offset);
            document
                .find_last_before(query, before)?
                .or(document.find_last_before(query, document.length.saturating_add(1))?)
        } else {
            let after = self
                .search_match
                .map_or(self.position.offset, |offset| offset.saturating_add(1));
            document
                .find_forward(query, after)?
                .or(document.find_forward(query, 0)?)
        };
        if let Some(found) = found {
            self.position = document.position_for_offset(found, columns)?;
            self.search_match = Some(found);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Dimensions {
    rows: usize,
    columns: usize,
}

impl Dimensions {
    fn content_rows(self) -> usize {
        self.rows.saturating_sub(1).max(1)
    }
}

#[derive(Debug)]
struct Terminal {
    input: File,
    output: File,
    original: libc::termios,
    screen_entered: bool,
}

impl Terminal {
    fn open() -> Result<Self, Error> {
        let tty = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")
            .map_err(|error| {
                Error::with_source("could not open controlling terminal".to_owned(), error)
            })?;
        let fd = tty.as_raw_fd();
        let mut original = std::mem::MaybeUninit::<libc::termios>::uninit();
        // SAFETY: `original` is writable and `fd` refers to the open controlling terminal.
        if unsafe { libc::tcgetattr(fd, original.as_mut_ptr()) } != 0 {
            return Err(Error::with_source(
                "could not read terminal settings".to_owned(),
                io::Error::last_os_error(),
            ));
        }
        // SAFETY: tcgetattr initialized `original` on success above.
        let original = unsafe { original.assume_init() };
        let mut raw = original;
        // SAFETY: `raw` is initialized and belongs to the active terminal.
        unsafe { libc::cfmakeraw(&mut raw) };
        raw.c_cc[libc::VMIN] = 0;
        raw.c_cc[libc::VTIME] = 1;
        // SAFETY: `raw` is initialized and `fd` remains open for this call.
        if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } != 0 {
            return Err(Error::with_source(
                "could not enable terminal input mode".to_owned(),
                io::Error::last_os_error(),
            ));
        }
        let output = tty.try_clone().map_err(|error| {
            // SAFETY: `original` is the termios value captured from this terminal.
            unsafe { libc::tcsetattr(fd, libc::TCSANOW, &original) };
            Error::with_source("could not open terminal output".to_owned(), error)
        })?;
        Ok(Self {
            input: tty,
            output,
            original,
            screen_entered: false,
        })
    }

    fn enter_screen(&mut self) -> Result<(), Error> {
        self.screen_entered = true;
        self.output
            .write_all(b"\x1b[?1049h\x1b[?25l\x1b[0m\x1b[2J\x1b[H")
            .and_then(|()| self.output.flush())
            .map_err(|error| Error::with_source("could not enter reader screen".to_owned(), error))
    }

    fn dimensions(&self) -> Dimensions {
        let mut size = std::mem::MaybeUninit::<libc::winsize>::zeroed();
        // SAFETY: `size` is writable and initialized by TIOCGWINSZ on success.
        let result =
            unsafe { libc::ioctl(self.output.as_raw_fd(), libc::TIOCGWINSZ, size.as_mut_ptr()) };
        if result == 0 {
            // SAFETY: ioctl succeeded and initialized the winsize value.
            let size = unsafe { size.assume_init() };
            Dimensions {
                rows: usize::from(size.ws_row.max(1)),
                columns: usize::from(size.ws_col.max(1)),
            }
        } else {
            Dimensions {
                rows: 24,
                columns: 80,
            }
        }
    }

    fn draw(
        &mut self,
        document: &mut Document,
        reader: &Reader,
        dimensions: Dimensions,
    ) -> Result<(), Error> {
        let frame = build_frame(document, reader, dimensions)?;
        self.output
            .write_all(&frame)
            .and_then(|()| self.output.flush())
            .map_err(|error| Error::with_source("could not render reader screen".to_owned(), error))
    }
}

fn build_frame(
    document: &mut Document,
    reader: &Reader,
    dimensions: Dimensions,
) -> Result<Vec<u8>, Error> {
    let mut frame = Vec::new();
    let content_rows = dimensions.content_rows();
    let mut position = Some(reader.position);
    for row_number in 0..content_rows {
        let row = if reader.show_help {
            help_row(row_number, dimensions.columns)
        } else if let Some(current) = position {
            let row = document.row(current, dimensions.columns)?;
            position = row.next;
            emphasize_row(&row.text, &row.protected_ranges, &reader.settings)
        } else {
            Vec::new()
        };
        frame.extend_from_slice(format!("\x1b[{};1H\x1b[2K", row_number + 1).as_bytes());
        frame.extend_from_slice(&row);
    }
    if dimensions.rows > 1 {
        let status_row = dimensions.rows;
        let status = status_text(document, reader);
        frame.extend_from_slice(format!("\x1b[{status_row};1H\x1b[2K").as_bytes());
        frame.extend_from_slice(truncate_cells(&status, dimensions.columns).as_bytes());
    }
    frame.extend_from_slice(b"\x1b[0m");
    Ok(frame)
}

impl Drop for Terminal {
    fn drop(&mut self) {
        if self.screen_entered {
            let _ = self.output.write_all(b"\x1b[0m\x1b[?25h\x1b[?1049l");
            let _ = self.output.flush();
        }
        // SAFETY: `original` was captured from this still-open terminal descriptor.
        unsafe {
            libc::tcsetattr(self.input.as_raw_fd(), libc::TCSAFLUSH, &self.original);
        }
    }
}

#[derive(Debug)]
struct SignalWatcher {
    quit: Arc<AtomicBool>,
    registrations: Vec<signal_hook::SigId>,
}

impl SignalWatcher {
    fn new() -> io::Result<Self> {
        let quit = Arc::new(AtomicBool::new(false));
        let mut watcher = Self {
            quit: Arc::clone(&quit),
            registrations: Vec::new(),
        };
        for signal in [
            signal_hook::consts::signal::SIGINT,
            signal_hook::consts::signal::SIGTERM,
            signal_hook::consts::signal::SIGHUP,
            signal_hook::consts::signal::SIGQUIT,
        ] {
            watcher
                .registrations
                .push(signal_hook::flag::register(signal, Arc::clone(&quit))?);
        }
        Ok(watcher)
    }
}

impl Drop for SignalWatcher {
    fn drop(&mut self) {
        for registration in &self.registrations {
            signal_hook::low_level::unregister(*registration);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Key {
    Character(char),
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
    Enter,
    Backspace,
    Escape,
    ControlC,
}

#[derive(Debug, Default)]
struct KeyBuffer {
    bytes: VecDeque<u8>,
    last_input: Option<Instant>,
}

impl KeyBuffer {
    fn push(&mut self, bytes: &[u8]) {
        let remaining = INPUT_BUFFER_LIMIT.saturating_sub(self.bytes.len());
        self.bytes.extend(bytes.iter().take(remaining).copied());
        self.last_input = Some(Instant::now());
    }

    fn next_key(&mut self, now: Instant) -> Option<Key> {
        let first = *self.bytes.front()?;
        match first {
            0x1b => self.escape_key(now),
            0x03 => {
                self.bytes.pop_front();
                Some(Key::ControlC)
            }
            b'\r' | b'\n' => {
                self.bytes.pop_front();
                Some(Key::Enter)
            }
            0x08 | 0x7f => {
                self.bytes.pop_front();
                Some(Key::Backspace)
            }
            0x00..=0x1f => {
                self.bytes.pop_front();
                None
            }
            0x20..=0x7e => {
                self.bytes.pop_front();
                Some(Key::Character(char::from(first)))
            }
            _ => self.utf8_key(now),
        }
    }

    fn escape_key(&mut self, now: Instant) -> Option<Key> {
        let timed_out = self
            .last_input
            .is_some_and(|last| now.duration_since(last) >= Duration::from_millis(40));
        if self.bytes.len() == 1 {
            if timed_out {
                self.bytes.pop_front();
                return Some(Key::Escape);
            }
            return None;
        }
        let second = self.bytes.get(1).copied()?;
        if second == b'[' {
            let final_index = (2..self.bytes.len()).find(|index| {
                self.bytes
                    .get(*index)
                    .is_some_and(|byte| (0x40..=0x7e).contains(byte))
            });
            let Some(final_index) = final_index else {
                if timed_out {
                    self.bytes.pop_front();
                    return Some(Key::Escape);
                }
                return None;
            };
            let sequence: Vec<u8> = self
                .bytes
                .iter()
                .take(final_index)
                .skip(2)
                .copied()
                .collect();
            let final_byte = self.bytes.get(final_index).copied().unwrap_or_default();
            for _ in 0..=final_index {
                self.bytes.pop_front();
            }
            return match (sequence.as_slice(), final_byte) {
                ([], b'A') => Some(Key::Up),
                ([], b'B') => Some(Key::Down),
                ([], b'C') => Some(Key::Right),
                ([], b'D') => Some(Key::Left),
                ([], b'H') | ([b'1' | b'7'], b'~') => Some(Key::Home),
                ([], b'F') | ([b'4' | b'8'], b'~') => Some(Key::End),
                ([b'5'], b'~') => Some(Key::PageUp),
                ([b'6'], b'~') => Some(Key::PageDown),
                _ => None,
            };
        }
        if second == b'O' && self.bytes.len() >= 3 {
            let third = self.bytes.get(2).copied()?;
            for _ in 0..3 {
                self.bytes.pop_front();
            }
            return match third {
                b'H' => Some(Key::Home),
                b'F' => Some(Key::End),
                _ => None,
            };
        }
        self.bytes.pop_front();
        Some(Key::Escape)
    }

    fn utf8_key(&mut self, now: Instant) -> Option<Key> {
        let first = *self.bytes.front()?;
        let length = match first {
            0xc2..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf4 => 4,
            _ => 1,
        };
        if self.bytes.len() < length {
            let timed_out = self
                .last_input
                .is_some_and(|last| now.duration_since(last) >= Duration::from_millis(150));
            if !timed_out {
                return None;
            }
            self.bytes.pop_front();
            return Some(Key::Character('\u{fffd}'));
        }
        let bytes: Vec<u8> = self.bytes.iter().take(length).copied().collect();
        let character = std::str::from_utf8(&bytes)
            .ok()
            .and_then(|text| text.chars().next())
            .unwrap_or('\u{fffd}');
        for _ in 0..length {
            self.bytes.pop_front();
        }
        Some(Key::Character(character))
    }
}

fn help_row(row: usize, columns: usize) -> Vec<u8> {
    let lines = [
        "Bionic Reader",
        "j/down or k/up: scroll one row     PageUp/PageDown or b/space: scroll a page",
        "g/Home: beginning                  G/End: end",
        "/: search                           n/N: next/previous match",
        "+/-: emphasis length                [ ]: emphasis frequency",
        "m/M: minimum emphasis length       p: cycle emphasis preset",
        "s: show or hide settings            ?: close help",
        "q, Esc, or Ctrl-C: quit",
    ];
    lines
        .get(row)
        .map_or_else(Vec::new, |line| truncate_cells(line, columns).into_bytes())
}

fn status_text(document: &Document, reader: &Reader) -> String {
    if let Some(query) = &reader.search_input {
        return format!("/{query}");
    }
    let line_number = reader.position.line.saturating_add(1);
    let mut status = format!("line {line_number}/{}", document.line_count());
    if reader.show_settings {
        let fraction_percent = reader.settings.fraction * 100.0;
        status.push_str(&format!(
            " | emphasis {fraction_percent:.0}% | every {} | minimum {} | attr {}",
            reader.settings.every,
            reader.settings.min_word_length,
            match reader.settings.emphasis {
                Emphasis::Bold => "bold",
                Emphasis::Underline => "underline",
                Emphasis::Reverse => "reverse",
            }
        ));
        if let Some(profile) = reader.settings.profile.as_ref() {
            status.push_str(" | preset ");
            status.push_str(match profile {
                Profile::Subtle => "subtle",
                Profile::Balanced => "balanced",
                Profile::Strong => "strong",
            });
        }
    }
    if !reader.search_query.is_empty() {
        status.push_str(" | /");
        status.push_str(&reader.search_query);
    }
    status.push_str(" | ? help  s settings  q quit");
    status
}

fn truncate_cells(value: &str, max_cells: usize) -> String {
    let mut output = String::new();
    let mut cells = 0_usize;
    for grapheme in value.graphemes(true) {
        let width = UnicodeWidthStr::width(grapheme);
        if cells.saturating_add(width) > max_cells {
            break;
        }
        output.push_str(grapheme);
        cells = cells.saturating_add(width);
    }
    output
}

fn emphasize_row(text: &str, protected: &[Range<usize>], settings: &Settings) -> Vec<u8> {
    if !settings.style.enabled(true) {
        return text.as_bytes().to_vec();
    }
    if protected.is_empty() {
        return emphasize_segment(text, settings);
    }
    let mut output = Vec::new();
    let mut offset = 0_usize;
    for range in protected {
        let start = range.start.max(offset).min(text.len());
        let end = range.end.min(text.len());
        if offset < start {
            output.extend(emphasize_segment(&text[offset..start], settings));
        }
        if start < end {
            output.extend_from_slice(text[start..end].as_bytes());
            offset = end;
        }
    }
    if offset < text.len() {
        output.extend(emphasize_segment(&text[offset..], settings));
    }
    output
}

fn emphasize_segment(text: &str, settings: &Settings) -> Vec<u8> {
    let mut transformer = Transformer::new(settings, true);
    let mut output = transformer.push(text.as_bytes());
    output.extend(transformer.finish());
    output
}

fn search_forward(file: &mut File, query: &[u8], start: u64, end: u64) -> io::Result<Option<u64>> {
    if query.is_empty() || start >= end {
        return Ok(None);
    }
    let mut reader = BufReader::with_capacity(IO_BUFFER_SIZE, file.try_clone()?);
    reader.seek(SeekFrom::Start(start))?;
    let mut current = start;
    let mut carry = Vec::new();
    let mut buffer = vec![0_u8; IO_BUFFER_SIZE];
    while current < end {
        let buffer_size = u64::try_from(IO_BUFFER_SIZE).expect("buffer size fits in u64");
        let request = usize::try_from((end - current).min(buffer_size))
            .expect("bounded search read fits in usize");
        let count = reader.read(&mut buffer[..request])?;
        if count == 0 {
            break;
        }
        let mut combined = carry;
        let carry_length = combined.len();
        combined.extend_from_slice(&buffer[..count]);
        let base = current.saturating_sub(u64::try_from(carry_length).expect("carry fits in u64"));
        if let Some(index) = combined
            .windows(query.len())
            .position(|window| window == query)
        {
            let found = base
                .checked_add(u64::try_from(index).expect("search index fits in u64"))
                .expect("document offset fits in u64");
            if found >= start
                && found.saturating_add(u64::try_from(query.len()).expect("query fits in u64"))
                    <= end
            {
                return Ok(Some(found));
            }
        }
        let keep = query.len().saturating_sub(1).min(combined.len());
        carry = combined[combined.len() - keep..].to_vec();
        current = current
            .checked_add(u64::try_from(count).expect("read count fits in u64"))
            .expect("document offset fits in u64");
    }
    Ok(None)
}

fn search_last_before(file: &mut File, query: &[u8], before: u64) -> io::Result<Option<u64>> {
    if query.is_empty() || before == 0 {
        return Ok(None);
    }
    let end = before.min(file.metadata()?.len());
    let query_length = u64::try_from(query.len()).expect("query length fits in u64");
    if query_length > end {
        return Ok(None);
    }

    let overlap = query.len().saturating_sub(1);
    let mut upper = end;
    let mut carry = Vec::new();
    while upper > 0 {
        let chunk_start = upper.saturating_sub(u64::try_from(IO_BUFFER_SIZE).expect("buffer fits"));
        let chunk_length = usize::try_from(upper - chunk_start)
            .expect("bounded backward search read fits in usize");
        file.seek(SeekFrom::Start(chunk_start))?;
        let mut chunk = vec![0_u8; chunk_length];
        file.read_exact(&mut chunk)?;

        let mut combined = chunk;
        let candidate_starts = combined.len();
        combined.extend_from_slice(&carry);
        if let Some(index) = combined
            .windows(query.len())
            .take(candidate_starts)
            .rposition(|window| window == query)
        {
            return Ok(Some(
                chunk_start
                    .checked_add(u64::try_from(index).expect("match offset fits in u64"))
                    .expect("document offset fits in u64"),
            ));
        }

        let keep = overlap.min(combined.len());
        carry = combined[..keep].to_vec();
        upper = chunk_start;
    }
    Ok(None)
}

#[cfg(test)]
mod unit_tests {
    use super::{
        Dimensions, Document, Key, KeyBuffer, PlainTextFilter, Position, Reader, SignalWatcher,
        Terminal, emphasize_row, help_row, is_dumb_terminal, spool_document, status_text,
        truncate_cells,
    };
    use crate::settings::{Emphasis, InputFormat, Profile, Settings, Style};
    use std::fs::File;
    use std::io::{self, Cursor, Read, Seek, SeekFrom};
    use std::time::{Duration, Instant};
    use unicode_width::UnicodeWidthStr;

    fn document(text: &[u8]) -> Document {
        let mut input = Cursor::new(text.to_vec());
        let file = spool_document(&mut input).expect("spool test document");
        Document::new(file, false, true).expect("index test document")
    }

    #[test]
    fn terminal_controls_and_control_bytes_are_removed_before_layout() {
        let mut filter = PlainTextFilter::default();
        let visible = filter
            .push(b"alpha\x1b[31mred\x1b[0m\x1b]8;;https://example.com\x07link\x1b]8;;\x07\n");
        assert_eq!(visible, b"alpharedlink\n");
    }

    #[test]
    fn spooling_strips_c1_presentation_sequences_without_visible_residue() {
        let mut input = Cursor::new(b"\x9b31mred\x9b0m \xc2\x9b31mtext\xc2\x9b0m".to_vec());
        let mut file = spool_document(&mut input).expect("spool C1 text");
        let mut visible = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut visible).expect("read filtered document");
        assert_eq!(visible, b"red text");
    }

    #[test]
    fn blockquote_and_list_reference_definitions_are_protected() {
        let source =
            "> [quote]: https://example.test/quote\n\n- [list]: https://example.test/list\n";
        let ranges = super::markdown_reference_destinations(source);
        for destination in ["https://example.test/quote", "https://example.test/list"] {
            let start = source
                .find(destination)
                .expect("reference destination exists");
            assert!(
                ranges.iter().any(|range| range.contains(&start)),
                "unprotected {destination}"
            );
        }
    }

    #[test]
    fn carriage_returns_are_normalized_or_rendered_as_visible_text() {
        let mut filter = PlainTextFilter::default();
        let mut visible = filter.push(b"first\r\nsecond\r");
        visible.extend(filter.finish());
        assert_eq!(visible, b"first\nsecond^M");
        assert!(!visible.contains(&b'\r'));
    }

    #[test]
    fn plain_text_filter_handles_split_malformed_and_incomplete_utf8() {
        let mut filter = PlainTextFilter::default();
        assert_eq!(filter.push(b"\xc3"), b"");
        assert_eq!(filter.push(b"\xa9"), "é".as_bytes());
        assert_eq!(filter.push(b"\xe0"), b"");
        assert_eq!(filter.push(b"\x80\x80x"), "�x".as_bytes());
        assert_eq!(filter.push(b"\xe2z"), "�z".as_bytes());
        assert_eq!(filter.push(b"\xf5\xff"), "��".as_bytes());
        assert_eq!(filter.push(b"\xe2\x82"), b"");
        assert_eq!(filter.finish(), "�".as_bytes());
    }

    #[test]
    fn spooling_and_document_search_preserve_io_errors_with_context() {
        struct FailedRead;

        impl Read for FailedRead {
            fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed source"))
            }
        }

        let error = spool_document(&mut FailedRead).expect_err("propagate source read errors");
        assert!(error.to_string().contains("could not read document"));
        assert_eq!(error.io_kind(), Some(io::ErrorKind::BrokenPipe));

        let directory = File::open(".").expect("open a directory as a file");
        let mut broken_document = Document {
            file: directory,
            line_starts: vec![0],
            protected_ranges: Vec::new(),
            length: 1,
        };
        let error = broken_document
            .find_forward(b"x", 0)
            .expect_err("report failed reads from the indexed source");
        assert!(error.to_string().contains("could not search document"));
        assert!(error.io_kind().is_some());
    }

    #[test]
    fn markdown_index_rejects_invalid_utf8_and_oversized_blocks_safely() {
        let mut invalid = tempfile::tempfile().expect("create a Markdown fixture");
        std::io::Write::write_all(&mut invalid, b"invalid \xff\n").expect("write invalid Markdown");
        invalid
            .seek(SeekFrom::Start(0))
            .expect("rewind Markdown fixture");
        let error = Document::new(invalid, true, true).expect_err("reject invalid Markdown UTF-8");
        assert!(error.is_invalid_input());
        assert!(
            error
                .to_string()
                .contains("Markdown input must be valid UTF-8")
        );

        let oversized_source = vec![b'x'; super::MAX_MARKDOWN_BLOCK_BYTES + 1];
        let mut oversized_input = Cursor::new(oversized_source);
        let file = spool_document(&mut oversized_input).expect("spool oversized Markdown block");
        let document = Document::new(file, true, false).expect("index oversized Markdown block");
        assert!(document.is_protected(0));
        assert!(document.is_protected(document.length - 1));
    }

    #[test]
    fn wrapped_rows_preserve_graphemes_and_cell_width() {
        let mut document = document("a👩‍💻e\u{301}界z".as_bytes());
        let first = document.row(Document::start(), 4).expect("read first row");
        assert_eq!(first.text, "a👩‍💻e\u{301}");
        let second = document
            .row(first.next.expect("next wrapped row"), 4)
            .expect("read next row");
        assert_eq!(second.text, "界z");
        assert_eq!(second.next, None);
    }

    #[test]
    fn wrapping_expands_tabs_replaces_wide_cells_and_clamps_positions() {
        let mut document = document("\t界x".as_bytes());
        let tab = document
            .row(Document::start(), 4)
            .expect("expand a leading tab");
        assert_eq!(tab.text, "    ");
        assert_eq!(tab.next, Some(Position { line: 0, offset: 1 }));

        let wide = document
            .row(tab.next.expect("tab is followed by a wide glyph"), 1)
            .expect("replace a glyph wider than the viewport");
        assert_eq!(wide.text, "�");
        assert_eq!(wide.next, Some(Position { line: 0, offset: 4 }));

        let tail = document
            .row(wide.next.expect("wide glyph is followed by text"), 0)
            .expect("treat a zero-column terminal as one cell wide");
        assert_eq!(tail.text, "x");
        assert_eq!(tail.next, None);

        let empty_at_end = document
            .row(
                Position {
                    line: 0,
                    offset: u64::MAX,
                },
                10,
            )
            .expect("clamp a stale offset to the line end");
        assert!(empty_at_end.text.is_empty());
        assert_eq!(empty_at_end.next, None);
    }

    #[test]
    fn rows_keep_zero_width_text_bounded_and_do_not_split_the_next_grapheme() {
        let source = "\u{200b}".repeat(super::MAX_VIEW_GRAPHEME_BYTES / 3 + 2);
        let mut document = document(source.as_bytes());
        let first = document
            .row(Document::start(), 1)
            .expect("read zero-width graphemes in a narrow viewport");
        assert!(first.text.len() <= super::MAX_VIEW_GRAPHEME_BYTES);
        let next = first
            .next
            .expect("the view buffer should stop at its byte bound");
        assert!(next.offset > 0);
        let second = document
            .row(next, 1)
            .expect("continue from a complete grapheme boundary");
        assert!(!second.text.is_empty());
    }

    #[test]
    fn row_viewport_tracks_protected_ranges_and_final_empty_rows() {
        let source = b"`protected` readable\n\n";
        let mut input = Cursor::new(source.to_vec());
        let file = spool_document(&mut input).expect("spool Markdown row fixture");
        let mut document = Document::new(file, true, true).expect("index Markdown row fixture");
        let row = document
            .row(Document::start(), 80)
            .expect("read a row with adjacent protected ranges");
        assert_eq!(row.text, "`protected` readable");
        assert_eq!(row.protected_ranges, vec![0..11]);
        assert_eq!(
            row.next,
            Some(Position {
                line: 1,
                offset: 21
            })
        );

        let blank = document
            .row(
                Position {
                    line: 1,
                    offset: 21,
                },
                80,
            )
            .expect("read an empty row");
        assert!(blank.text.is_empty());
        assert_eq!(
            blank.next,
            Some(Position {
                line: 2,
                offset: 22
            })
        );
        let final_row = document
            .row(
                Position {
                    line: usize::MAX,
                    offset: 0,
                },
                80,
            )
            .expect("clamp a stale line number to the final row");
        assert!(final_row.text.is_empty());
        assert_eq!(final_row.next, None);
    }

    #[test]
    fn grapheme_reader_preserves_a_multibyte_scalar_split_at_the_io_boundary() {
        let prefix = "x".repeat(super::IO_BUFFER_SIZE - 1);
        let source = format!("{prefix}é👩‍💻tail");
        let mut document = document(source.as_bytes());
        let row = document
            .row(Document::start(), super::IO_BUFFER_SIZE + 16)
            .expect("read UTF-8 and grapheme sequences spanning chunks");
        assert!(row.text.ends_with("é👩‍💻tail"));
    }

    #[test]
    fn grapheme_cursor_keeps_emoji_joiners_across_a_file_chunk_boundary() {
        let prefix = "x".repeat(super::IO_BUFFER_SIZE - "👩\u{200d}".len());
        let source = format!("{prefix}👩‍💻tail");
        let mut document = document(source.as_bytes());
        let row = document
            .row(Document::start(), super::IO_BUFFER_SIZE + 16)
            .expect("read an emoji grapheme spanning a file chunk");
        assert!(row.text.ends_with("👩‍💻tail"));
    }

    #[test]
    fn markdown_protects_inline_fenced_code_and_link_destinations() {
        let source = "Readable words `private code` [public label](https://example.org/inline)\n\n[ref]: <https://example.org/reference>\n\n```rust\nprivate words\n```\n";
        let mut input = Cursor::new(source.as_bytes().to_vec());
        let file = spool_document(&mut input).expect("spool Markdown fixture");
        let mut document = Document::new(file, true, true).expect("index Markdown fixture");
        let row = document
            .row(Document::start(), 120)
            .expect("read Markdown paragraph");
        let protected_text = row
            .protected_ranges
            .iter()
            .map(|range| &row.text[range.clone()])
            .collect::<Vec<_>>();
        assert!(protected_text.contains(&"`private code`"));
        assert!(protected_text.contains(&"https://example.org/inline"));
        assert!(!protected_text.contains(&"public label"));
        let styled = emphasize_row(
            &row.text,
            &row.protected_ranges,
            &Settings {
                input_format: InputFormat::Markdown,
                style: Style::Always,
                ..Settings::new()
            },
        );
        for protected in ["`private code`", "https://example.org/inline"] {
            assert!(
                styled
                    .windows(protected.len())
                    .any(|window| window == protected.as_bytes()),
                "the reader should leave {protected:?} unstyled"
            );
        }

        let reference_url = source
            .find("https://example.org/reference")
            .map(|offset| u64::try_from(offset).expect("reference offset fits in u64"))
            .expect("reference destination exists");
        assert!(document.is_protected(reference_url));
        let code_line = document.line_starts[5];
        assert!(document.is_protected(code_line));
        let fenced_row = document
            .row(
                Position {
                    line: 5,
                    offset: code_line,
                },
                120,
            )
            .expect("read fenced Markdown code");
        let styled_code = emphasize_row(
            &fenced_row.text,
            &fenced_row.protected_ranges,
            &Settings {
                input_format: InputFormat::Markdown,
                style: Style::Always,
                ..Settings::new()
            },
        );
        assert!(
            styled_code
                .windows(b"private words".len())
                .any(|window| window == b"private words")
        );
    }

    #[test]
    fn markdown_link_destinations_stay_protected_when_code_emphasis_is_enabled() {
        let source = "`code words` [label](https://example.org/path)";
        let mut input = Cursor::new(source.as_bytes().to_vec());
        let file = spool_document(&mut input).expect("spool Markdown fixture");
        let document = Document::new(file, true, false).expect("index Markdown fixture");
        let url = source
            .find("https://example.org/path")
            .map(|offset| u64::try_from(offset).expect("inline destination offset fits in u64"))
            .expect("inline destination exists");
        let code = source
            .find("code words")
            .map(|offset| u64::try_from(offset).expect("inline code offset fits in u64"))
            .expect("inline code exists");
        assert!(document.is_protected(url));
        assert!(!document.is_protected(code));
    }

    #[test]
    fn help_rows_fit_narrow_terminal_widths() {
        for columns in 1..=12 {
            let row = help_row(1, columns);
            let text = std::str::from_utf8(&row).expect("help remains valid UTF-8");
            assert!(UnicodeWidthStr::width(text) <= columns);
        }
    }

    #[test]
    fn long_unbroken_line_wraps_without_reading_the_whole_line() {
        let source = vec![b'x'; 100_000];
        let mut document = document(&source);
        let first = document
            .row(Document::start(), 7)
            .expect("read first wrapped row");
        assert_eq!(first.text, "xxxxxxx");
        assert_eq!(first.next, Some(Position { line: 0, offset: 7 }));
    }

    #[test]
    fn navigation_uses_wrapped_rows_and_handles_empty_final_line() {
        let mut document = document(b"abcdefgh\nlast\n");
        let first = Document::start();
        let second = document
            .move_down(first, 1, 4)
            .expect("move to second visual row");
        assert_eq!(second, Position { line: 0, offset: 4 });
        assert_eq!(document.move_up(second, 4).expect("move up"), first);
        let end = document.last_position(4).expect("find final visual row");
        assert_eq!(
            end,
            Position {
                line: 2,
                offset: 14
            }
        );
    }

    #[test]
    fn navigation_clamps_at_document_edges_and_maps_offsets_to_visual_rows() {
        let mut document = document(b"abcdef\nxy\n");
        let start = Document::start();
        assert_eq!(document.move_up(start, 3).expect("stay at start"), start);
        let wrapped = Position { line: 0, offset: 3 };
        assert_eq!(
            document
                .move_up(wrapped, 3)
                .expect("move to first wrapped row"),
            start
        );
        assert_eq!(
            document
                .move_up(Position { line: 1, offset: 7 }, 3)
                .expect("move from the second line to the preceding line"),
            Position { line: 0, offset: 3 }
        );
        assert_eq!(
            document
                .move_down(Position { line: 1, offset: 7 }, 3, 3)
                .expect("stop at the last document row"),
            Position {
                line: 2,
                offset: 10
            }
        );
        assert_eq!(
            document
                .position_for_offset(4, 3)
                .expect("map a byte offset into a wrapped row"),
            Position { line: 0, offset: 3 }
        );
        assert_eq!(
            document
                .position_for_offset(100, 3)
                .expect("clamp an offset beyond the document"),
            Position {
                line: 2,
                offset: 10
            }
        );
    }

    #[test]
    fn input_decoder_handles_arrows_utf8_and_escape_timeout() {
        let now = Instant::now();
        let mut keys = KeyBuffer::default();
        keys.push(b"\x1b[A");
        assert_eq!(keys.next_key(now), Some(Key::Up));
        keys.push("é".as_bytes());
        assert_eq!(keys.next_key(now), Some(Key::Character('é')));
        keys.push(b"\x1b");
        assert_eq!(keys.next_key(now), None);
        assert_eq!(
            keys.next_key(now + Duration::from_millis(50)),
            Some(Key::Escape)
        );
    }

    #[test]
    fn reader_settings_start_with_the_supplied_values() {
        let settings = Settings {
            every: 3,
            ..Settings::new()
        };
        let reader = Reader::new(settings);
        assert_eq!(reader.settings.every, 3);
        assert_eq!(
            Dimensions {
                rows: 24,
                columns: 80
            }
            .content_rows(),
            23
        );
    }

    #[test]
    fn dumb_terminal_is_rejected_with_an_actionable_reader_error() {
        assert!(is_dumb_terminal(Some("dumb")));
        assert!(!is_dumb_terminal(Some("xterm-256color")));
        assert!(!is_dumb_terminal(None));
    }

    #[test]
    fn streaming_search_matches_across_chunk_boundaries_in_both_directions() {
        let mut text = vec![b'x'; super::IO_BUFFER_SIZE - 3];
        text.extend_from_slice(b"needle and needle");
        let mut document = document(&text);
        let first = u64::try_from(super::IO_BUFFER_SIZE - 3).expect("test offset fits");
        assert_eq!(
            document.find_forward(b"needle", 0).expect("search forward"),
            Some(first)
        );
        assert_eq!(
            document
                .find_last_before(b"needle", document.length)
                .expect("search backward"),
            Some(first + 11)
        );
    }

    #[test]
    fn search_handles_empty_queries_bounds_and_matches_longer_than_a_chunk() {
        let mut small = document(b"abc abc");
        assert_eq!(
            small.find_forward(b"", 0).expect("empty forward query"),
            None
        );
        assert_eq!(
            small
                .find_forward(b"abc", small.length)
                .expect("search at EOF"),
            None
        );
        assert_eq!(
            small.find_last_before(b"", 4).expect("empty reverse query"),
            None
        );
        assert_eq!(
            small.find_last_before(b"abc", 0).expect("empty bound"),
            None
        );
        assert_eq!(
            small.find_last_before(b"abc abc", 6).expect("short bound"),
            None
        );

        let query = vec![b'a'; super::IO_BUFFER_SIZE + 7];
        let text = vec![b'a'; super::IO_BUFFER_SIZE * 3];
        let mut large = document(&text);
        assert_eq!(
            large
                .find_last_before(&query, large.length)
                .expect("scan a query spanning multiple reverse chunks"),
            Some(u64::try_from(text.len() - query.len()).expect("match offset fits in u64"))
        );
    }
    #[test]
    fn giant_graphemes_are_consumed_without_splitting_or_retaining_their_bytes() {
        let source = format!("a{}tail", "\u{301}".repeat(100_000));
        let mut document = document(source.as_bytes());
        let row = document
            .row(Document::start(), 8)
            .expect("read a giant grapheme");
        assert_eq!(row.text, "\u{fffd}tail");
        assert_eq!(row.next, None);
        assert!(row.text.len() < super::MAX_VIEW_GRAPHEME_BYTES);
    }

    #[test]
    fn malformed_view_input_is_replaced_without_dropping_following_content() {
        for source in [b"\xfftail".as_slice(), b"\xc0tail", b"\xe2tail", b"\xe2"] {
            let mut document = document(source);
            let row = document
                .row(Document::start(), 80)
                .expect("read malformed input");
            assert!(row.text.starts_with('\u{fffd}'), "{:?}", row.text);
        }
    }

    #[test]
    fn backward_search_scans_repetitive_input_once_and_keeps_overlapping_matches() {
        let mut document = document(&vec![b'a'; 1_000_000]);
        assert_eq!(
            document
                .find_last_before(b"aaa", document.length)
                .expect("search repetitive text"),
            Some(999_997)
        );
        assert_eq!(
            document
                .find_last_before(b"aaa", 3)
                .expect("match ends at bound"),
            Some(0)
        );
        assert_eq!(
            document
                .find_last_before(b"aaa", 2)
                .expect("query exceeds bound"),
            None
        );
        assert_eq!(
            document.find_last_before(b"", 3).expect("empty query"),
            None
        );
    }

    #[test]
    fn backward_search_finds_a_match_crossing_a_chunk_edge_with_exclusive_bounds() {
        let mut text = vec![b'x'; super::IO_BUFFER_SIZE * 2];
        let query = b"match across the reverse chunk edge";
        let start = super::IO_BUFFER_SIZE - 5;
        text[start..start + query.len()].copy_from_slice(query);
        let mut document = document(&text);
        let match_start = u64::try_from(start).expect("match start fits in u64");
        let match_end = match_start
            .checked_add(u64::try_from(query.len()).expect("query length fits in u64"))
            .expect("match end fits in u64");

        assert_eq!(
            document
                .find_last_before(query, document.length)
                .expect("search across the chunk edge"),
            Some(match_start)
        );
        assert_eq!(
            document
                .find_last_before(query, match_end - 1)
                .expect("exclusive bound omits an incomplete match"),
            None
        );
        assert_eq!(
            document
                .find_last_before(query, match_end)
                .expect("exclusive bound includes a complete match"),
            Some(match_start)
        );
    }
    #[test]
    fn reader_navigation_keys_move_by_visual_rows_and_pages() {
        let mut document = document(b"first needle\nsecond needle\nlast\n");
        let mut reader = Reader::new(Settings::new());
        let size = Dimensions {
            rows: 3,
            columns: 8,
        };
        for (key, expected) in [
            (Key::Down, Position { line: 0, offset: 8 }),
            (Key::Up, Position { line: 0, offset: 0 }),
            (
                Key::PageDown,
                Position {
                    line: 1,
                    offset: 13,
                },
            ),
            (Key::PageUp, Position { line: 0, offset: 0 }),
            (
                Key::End,
                Position {
                    line: 3,
                    offset: 32,
                },
            ),
            (Key::Home, Position { line: 0, offset: 0 }),
        ] {
            assert!(
                !reader
                    .handle_key(key, &mut document, size)
                    .expect("navigate")
            );
            assert_eq!(reader.position, expected, "{key:?}");
        }
    }

    #[test]
    fn reader_settings_keys_change_values_and_cycle_profiles() {
        let mut document = document(b"first needle\nsecond needle\nlast\n");
        let mut reader = Reader::new(Settings::new());
        let size = Dimensions {
            rows: 3,
            columns: 8,
        };
        let initial_fraction = reader.settings.fraction;
        let initial_minimum = reader.settings.min_word_length;
        for (key, every, minimum, fraction) in [
            ('+', 1, initial_minimum, initial_fraction + 0.05),
            ('-', 1, initial_minimum, initial_fraction),
            ('[', 1, initial_minimum, initial_fraction),
            (']', 2, initial_minimum, initial_fraction),
            (
                'm',
                2,
                initial_minimum.saturating_sub(1).max(1),
                initial_fraction,
            ),
            ('M', 2, initial_minimum, initial_fraction),
        ] {
            assert!(
                !reader
                    .handle_key(Key::Character(key), &mut document, size)
                    .expect("adjust settings")
            );
            assert_eq!(reader.settings.every, every, "key {key}");
            assert_eq!(reader.settings.min_word_length, minimum, "key {key}");
            assert!(
                (reader.settings.fraction - fraction).abs() < 0.000_001,
                "key {key}: {}",
                reader.settings.fraction
            );
            assert!(reader.settings.profile.is_none(), "key {key}");
        }

        assert!(
            !reader
                .handle_key(Key::Character('s'), &mut document, size)
                .expect("hide settings")
        );
        assert!(!reader.show_settings);
        assert!(
            !reader
                .handle_key(Key::Character('s'), &mut document, size)
                .expect("show settings")
        );
        assert!(reader.show_settings);
        assert!(
            !reader
                .handle_key(Key::Character('?'), &mut document, size)
                .expect("show help")
        );
        assert!(reader.show_help);
        assert!(
            !reader
                .handle_key(Key::Character('?'), &mut document, size)
                .expect("close help")
        );
        assert!(!reader.show_help);
        for profile in [
            Profile::Subtle,
            Profile::Balanced,
            Profile::Strong,
            Profile::Subtle,
        ] {
            reader
                .handle_key(Key::Character('p'), &mut document, size)
                .expect("cycle profiles");
            assert_eq!(reader.settings.profile, Some(profile));
        }
    }

    #[test]
    fn reader_search_keys_edit_query_find_next_previous_and_cancel() {
        let mut document = document(b"first needle\nsecond needle\nlast\n");
        let mut reader = Reader::new(Settings::new());
        let size = Dimensions {
            rows: 3,
            columns: 8,
        };
        reader
            .handle_key(Key::Character('/'), &mut document, size)
            .expect("open search");
        assert_eq!(reader.search_input.as_deref(), Some(""));
        reader
            .handle_key(Key::Character('x'), &mut document, size)
            .expect("type query");
        assert_eq!(reader.search_input.as_deref(), Some("x"));
        reader
            .handle_key(Key::Backspace, &mut document, size)
            .expect("edit query");
        assert_eq!(reader.search_input.as_deref(), Some(""));
        reader
            .handle_key(Key::Left, &mut document, size)
            .expect("ignore navigation in search");
        assert_eq!(reader.search_input.as_deref(), Some(""));
        for character in "needle".chars() {
            reader
                .handle_key(Key::Character(character), &mut document, size)
                .expect("type query");
        }
        reader
            .handle_key(Key::Enter, &mut document, size)
            .expect("submit query");
        assert_eq!(reader.search_match, Some(6));
        reader
            .handle_key(Key::Character('n'), &mut document, size)
            .expect("next match");
        assert_eq!(reader.search_match, Some(20));
        assert_eq!(
            reader.position,
            Position {
                line: 1,
                offset: 13
            }
        );
        reader
            .handle_key(Key::Character('N'), &mut document, size)
            .expect("previous match");
        assert_eq!(reader.search_match, Some(6));
        assert_eq!(reader.position, Position { line: 0, offset: 0 });
        reader
            .handle_key(Key::Character('/'), &mut document, size)
            .expect("open search");
        reader
            .handle_key(Key::Escape, &mut document, size)
            .expect("cancel search");
        assert!(reader.search_input.is_none());
        assert!(
            reader
                .handle_key(Key::ControlC, &mut document, size)
                .expect("quit")
        );
    }

    #[test]
    fn search_wraps_at_document_edges_and_preserves_position_when_unmatched() {
        let mut document = document(b"needle middle needle");
        let mut reader = Reader::new(Settings::new());
        reader.search_query = "needle".to_owned();
        let size = Dimensions {
            rows: 4,
            columns: 80,
        };

        reader
            .handle_key(Key::Character('N'), &mut document, size)
            .expect("previous match wraps from the beginning");
        assert_eq!(reader.search_match, Some(14));
        reader.position = Document::start();
        reader
            .handle_key(Key::Character('n'), &mut document, size)
            .expect("next match wraps from the end");
        assert_eq!(reader.search_match, Some(0));

        reader.search_query = "not present".to_owned();
        let old_position = reader.position;
        let old_match = reader.search_match;
        reader
            .handle_key(Key::Character('n'), &mut document, size)
            .expect("an unmatched search is harmless");
        assert_eq!(reader.position, old_position);
        assert_eq!(reader.search_match, old_match);

        reader.search_query.clear();
        reader
            .handle_key(Key::Character('N'), &mut document, size)
            .expect("an empty search is a no-op");
        assert_eq!(reader.position, old_position);
    }

    #[test]
    fn settings_keys_clamp_numeric_values_and_clear_a_selected_profile() {
        let mut document = document(b"settings");
        let mut reader = Reader::new(Settings {
            fraction: 0.0,
            every: 1,
            min_word_length: 1,
            profile: Some(Profile::Strong),
            ..Settings::new()
        });
        let size = Dimensions {
            rows: 2,
            columns: 20,
        };
        for key in ['-', '[', 'm'] {
            reader
                .handle_key(Key::Character(key), &mut document, size)
                .expect("minimum settings remain valid");
        }
        assert_eq!(reader.settings.fraction.to_bits(), 0.0_f64.to_bits());
        assert_eq!(reader.settings.every, 1);
        assert_eq!(reader.settings.min_word_length, 1);
        assert!(reader.settings.profile.is_none());

        reader.settings.fraction = 1.0;
        reader.settings.every = usize::MAX;
        reader.settings.min_word_length = usize::MAX;
        for key in ['+', ']', 'M'] {
            reader
                .handle_key(Key::Character(key), &mut document, size)
                .expect("maximum settings remain valid");
        }
        assert_eq!(reader.settings.fraction.to_bits(), 1.0_f64.to_bits());
        assert_eq!(reader.settings.every, usize::MAX);
        assert_eq!(reader.settings.min_word_length, usize::MAX);
    }

    #[test]
    fn key_decoder_handles_terminal_variants_malformed_input_and_buffer_limits() {
        for (bytes, key) in [
            (b"\x1b[B".as_slice(), Key::Down),
            (b"\x1b[C", Key::Right),
            (b"\x1b[D", Key::Left),
            (b"\x1b[H", Key::Home),
            (b"\x1b[F", Key::End),
            (b"\x1b[1~", Key::Home),
            (b"\x1b[7~", Key::Home),
            (b"\x1b[4~", Key::End),
            (b"\x1b[8~", Key::End),
            (b"\x1b[5~", Key::PageUp),
            (b"\x1b[6~", Key::PageDown),
            (b"\x1bOH", Key::Home),
            (b"\x1bOF", Key::End),
            (b"\x7f", Key::Backspace),
            (b"\x08", Key::Backspace),
            (b"\r", Key::Enter),
            (b"\x03", Key::ControlC),
            ("界".as_bytes(), Key::Character('界')),
            ("👩".as_bytes(), Key::Character('👩')),
            (b"\xff", Key::Character('\u{fffd}')),
        ] {
            let mut keys = KeyBuffer::default();
            keys.push(bytes);
            assert_eq!(keys.next_key(Instant::now()), Some(key), "{bytes:?}");
            assert_eq!(keys.next_key(Instant::now()), None);
        }
        for bytes in [b"\x1b[9~".as_slice(), b"\x1bOX", b"\x00"] {
            let mut keys = KeyBuffer::default();
            keys.push(bytes);
            assert_eq!(keys.next_key(Instant::now()), None);
        }
        let mut keys = KeyBuffer::default();
        keys.push(b"\xe2");
        assert_eq!(keys.next_key(Instant::now()), None);
        assert_eq!(
            keys.next_key(Instant::now() + Duration::from_millis(151)),
            Some(Key::Character('\u{fffd}'))
        );
        let mut keys = KeyBuffer::default();
        keys.push(b"\x1b[");
        assert_eq!(keys.next_key(Instant::now()), None);
        assert_eq!(
            keys.next_key(Instant::now() + Duration::from_millis(41)),
            Some(Key::Escape)
        );
        let mut keys = KeyBuffer::default();
        keys.push(&vec![b'x'; super::INPUT_BUFFER_LIMIT + 1]);
        assert_eq!(keys.bytes.len(), super::INPUT_BUFFER_LIMIT);
    }
    #[test]
    fn the_last_document_row_is_drawn_once_and_remaining_screen_rows_are_blank() {
        let mut document = document(b"only row");
        let reader = Reader::new(Settings {
            style: Style::Never,
            ..Settings::new()
        });
        let frame = super::build_frame(
            &mut document,
            &reader,
            Dimensions {
                rows: 8,
                columns: 80,
            },
        )
        .expect("build final-page frame");
        let frame = String::from_utf8(frame).expect("screen frame is UTF-8");
        assert_eq!(frame.matches("only row").count(), 1, "{frame:?}");
        assert!(frame.contains("\x1b[2;1H\x1b[2K\x1b[3;1H"), "{frame:?}");
    }

    #[test]
    fn viewport_and_status_render_help_search_profiles_and_unicode_truncation() {
        let mut document = document("a wide界 row\nsecond\n".as_bytes());
        let mut reader = Reader::new(Settings {
            style: Style::Never,
            ..Settings::new()
        });
        reader.show_help = true;
        let help = super::build_frame(
            &mut document,
            &reader,
            Dimensions {
                rows: 9,
                columns: 12,
            },
        )
        .expect("build a small help viewport");
        let help = String::from_utf8(help).expect("help frame is UTF-8");
        assert!(help.contains("Bionic Reade"), "{help:?}");
        assert!(help.contains("q, Esc, or "), "{help:?}");

        reader.show_help = false;
        reader.search_input = Some("partial".to_owned());
        assert_eq!(status_text(&document, &reader), "/partial");
        let searching = super::build_frame(
            &mut document,
            &reader,
            Dimensions {
                rows: 2,
                columns: 8,
            },
        )
        .expect("build a search viewport");
        let searching = String::from_utf8(searching).expect("search frame is UTF-8");
        assert!(searching.contains("/partial"), "{searching:?}");

        reader.search_input = None;
        reader.show_settings = true;
        reader.search_query = "done".to_owned();
        for (profile, label, emphasis, attribute) in [
            (Profile::Subtle, "subtle", Emphasis::Bold, "bold"),
            (
                Profile::Balanced,
                "balanced",
                Emphasis::Underline,
                "underline",
            ),
            (Profile::Strong, "strong", Emphasis::Reverse, "reverse"),
        ] {
            reader.settings.profile = Some(profile);
            reader.settings.emphasis = emphasis;
            let status = status_text(&document, &reader);
            assert!(status.contains(&format!("attr {attribute}")), "{status}");
            assert!(status.contains(&format!("preset {label}")), "{status}");
            assert!(status.contains("| /done"), "{status}");
        }
        assert_eq!(help_row(usize::MAX, 8), Vec::<u8>::new());
        assert_eq!(truncate_cells("界x", 1), "");
        assert_eq!(truncate_cells("界x", 2), "界");
    }

    #[test]
    fn terminal_event_loop_reports_input_errors_and_restores_screen_on_drop() {
        let input = File::open(".").expect("open a directory for a failing input stream");
        let output = tempfile::tempfile().expect("create a captured terminal output");
        let mut captured = output
            .try_clone()
            .expect("clone terminal output for assertions");
        // SAFETY: `libc::termios` is a plain C state value; the invalid descriptor
        // used by this test makes restoration fail harmlessly and the result is never read.
        let original = unsafe { std::mem::zeroed() };
        let mut terminal = Terminal {
            input,
            output,
            original,
            screen_entered: true,
        };
        let mut document = document(b"event loop input error");
        let mut reader = Reader::new(Settings::new());
        let signals = SignalWatcher::new().expect("register temporary signal watcher");
        let error = reader
            .event_loop(&mut terminal, &mut document, &signals)
            .expect_err("report an input error from the terminal");
        assert!(error.to_string().contains("could not read terminal input"));
        drop(terminal);
        captured
            .seek(SeekFrom::Start(0))
            .expect("rewind captured terminal output");
        let mut output = Vec::new();
        captured
            .read_to_end(&mut output)
            .expect("read terminal output after cleanup");
        assert!(
            output
                .windows(b"\x1b[?1049l".len())
                .any(|bytes| bytes == b"\x1b[?1049l"),
            "the alternate screen should be restored after the input error"
        );
    }

    #[test]
    fn terminal_setup_and_render_errors_keep_their_operation_context() {
        let input = File::open("/dev/null").expect("open harmless terminal input");
        let output = File::open(".").expect("open a directory as a failing output stream");
        // SAFETY: `libc::termios` is a plain C state value; this test only exercises
        // terminal writes and does not depend on the saved value's contents.
        let original = unsafe { std::mem::zeroed() };
        let mut terminal = Terminal {
            input,
            output,
            original,
            screen_entered: false,
        };
        let setup_error = terminal
            .enter_screen()
            .expect_err("reject a non-writable screen");
        assert!(
            setup_error
                .to_string()
                .contains("could not enter reader screen")
        );
        assert!(
            terminal.screen_entered,
            "drop should attempt screen restoration"
        );

        let mut document = document(b"render error");
        let reader = Reader::new(Settings::new());
        let draw_error = terminal
            .draw(
                &mut document,
                &reader,
                Dimensions {
                    rows: 2,
                    columns: 20,
                },
            )
            .expect_err("report a non-writable screen renderer");
        assert!(
            draw_error
                .to_string()
                .contains("could not render reader screen")
        );
    }
}

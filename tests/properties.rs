//! Generated invariants for byte preservation, segmentation, and incremental rendering.

use std::collections::HashSet;
use std::io::{self, Read};

use bionic_reader::document::{self, ControlFilter};
use bionic_reader::segmentation;
use bionic_reader::settings::{ControlPolicy, InputFormat, OutputFormat, Settings};
use bionic_reader::transformer::{self, Transformer};
use proptest::collection;
use proptest::prelude::{ProptestConfig, Strategy, any};
use proptest::{prop_assert, prop_assert_eq, proptest};
use unicode_segmentation::UnicodeSegmentation;

fn unstyled(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut position = 0;
    while position < input.len() {
        if input[position..].starts_with(b"\x1b[1m") {
            position += 4;
        } else if input[position..].starts_with(b"\x1b[22m") {
            position += 5;
        } else {
            output.push(input[position]);
            position += 1;
        }
    }
    output
}

struct ChunkedInput<'a> {
    bytes: &'a [u8],
    width: usize,
}

impl Read for ChunkedInput<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let count = self.width.min(buf.len()).min(self.bytes.len());
        buf[..count].copy_from_slice(&self.bytes[..count]);
        self.bytes = &self.bytes[count..];
        Ok(count)
    }
}

proptest! {
    #![proptest_config(ProptestConfig::default())]

    #[test]
    fn markdown_fixation_frequency_is_repeat_safe(
        words in collection::vec(collection::vec(b'a'..=b'z', 1..24), 0..24),
        every in 1usize..8,
        minimum in 1usize..8,
        fraction_step in 0u8..=4,
    ) {
        let source = words.into_iter()
            .map(|word| word.into_iter().map(char::from).collect::<String>())
            .collect::<Vec<_>>().join(" ");
        let settings = Settings {
            fraction: f64::from(fraction_step) / 4.0,
            every,
            min_word_length: minimum,
            format: OutputFormat::Markdown,
            ..Settings::default()
        };
        let once = document::render(source.as_bytes(), &settings, true)
            .expect("render plain words as Markdown");
        let twice = document::render(&once, &Settings {
            input_format: InputFormat::Markdown,
            ..settings
        }, true).expect("reprocess generated Markdown");
        prop_assert_eq!(twice, once);
    }

    #[test]
    fn plain_unicode_preserves_bytes_and_is_repeat_safe(
        characters in collection::vec(any::<char>().prop_filter("no terminal escape", |c| *c != '\u{1b}'), 0..192),
        every in 1_usize..5,
        fraction in 0_u32..101,
    ) {
        let text: String = characters.into_iter().collect();
        let settings = Settings {
            fraction: f64::from(fraction) / 100.0,
            every,
            ..Settings::default()
        };
        let output = transformer::transform(text.as_bytes(), &settings, true);
        prop_assert_eq!(unstyled(&output), text.as_bytes());
        prop_assert_eq!(transformer::transform(&output, &settings, true), output);
    }

    #[test]
    fn fixation_spans_end_on_extended_grapheme_boundaries(text in any::<String>()) {
        let mut boundaries: HashSet<usize> = text.grapheme_indices(true).map(|(position, _)| position).collect();
        boundaries.insert(text.len());
        for span in segmentation::emphasis_spans(&text, &Settings::default()) {
            prop_assert!(boundaries.contains(&span.start));
            prop_assert!(boundaries.contains(&span.end));
            prop_assert!(span.start < span.end);
        }
    }

    #[test]
    fn arbitrary_byte_chunking_matches_complete_rendering(
        input in collection::vec(any::<u8>(), 0..512),
        width in 1_usize..64,
    ) {
        let settings = Settings::default();
        let expected = transformer::transform(&input, &settings, true);
        let mut renderer = Transformer::new(&settings, true);
        let mut output = Vec::new();
        for chunk in input.chunks(width) { output.extend(renderer.push(chunk)); }
        output.extend(renderer.finish());
        prop_assert_eq!(output, expected);
        prop_assert_eq!(transformer::transform(&input, &settings, false), input);
    }

    #[test]
    fn safe_control_filter_is_chunk_independent(
        input in collection::vec(any::<u8>(), 0..512),
        width in 1_usize..64,
    ) {
        let mut complete = ControlFilter::new(ControlPolicy::Safe);
        let mut expected = complete.push(&input);
        expected.extend(complete.finish());
        let mut filter = ControlFilter::new(ControlPolicy::Safe);
        let mut output = Vec::new();
        for chunk in input.chunks(width) { output.extend(filter.push(chunk)); }
        output.extend(filter.finish());
        prop_assert_eq!(output, expected);
    }

    #[test]
    fn document_representations_are_independent_of_reader_chunk_sizes(
        text in any::<String>(),
        width in 1_usize..64,
        format_index in 0_usize..4,
    ) {
        let settings = Settings {
            format: [OutputFormat::Ansi, OutputFormat::Plain, OutputFormat::Markdown, OutputFormat::Html][format_index],
            ..Settings::default()
        };
        let expected = document::render(text.as_bytes(), &settings, true).expect("valid text conversion");
        let mut input = ChunkedInput { bytes: text.as_bytes(), width };
        let mut output = Vec::new();
        document::render_stream(&mut input, &mut output, &settings, true).expect("valid streaming conversion");
        prop_assert_eq!(output, expected);
    }
}

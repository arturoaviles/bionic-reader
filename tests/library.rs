//! Application-level document transformation behavior.

use bionic_reader::reading::{Options, Reader};
use bionic_reader::settings::OutputFormat;
use serde::Deserialize;

#[derive(Deserialize)]
struct Corpus {
    render: Vec<RenderCase>,
    segments: Vec<SegmentCase>,
}

#[derive(Deserialize)]
struct RenderCase {
    name: String,
    text: String,
    format: OutputFormat,
    options: serde_json::Value,
    output: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct SegmentCase {
    name: String,
    text: String,
    options: Options,
    output: Vec<bionic_reader::reading::TextSegment>,
}

#[test]
fn application_can_render_a_document_without_terminal_detection() {
    let reader = Reader::default();
    let result = reader
        .render("Hello world", OutputFormat::Html, &Options::default())
        .expect("valid plain text renders as HTML");
    assert_eq!(result, "<strong>Hel</strong>lo <strong>wor</strong>ld");
}

#[test]
fn custom_presentation_receives_text_segments_with_configured_frequency() {
    let reader = Reader::default();
    let options = Options {
        every: Some(2),
        ..Options::default()
    };
    let segments = reader
        .segments("Hello world again", &options)
        .expect("valid text produces segments");
    let parts: Vec<(&str, bool)> = segments
        .iter()
        .map(|segment| (segment.text.as_str(), segment.emphasized))
        .collect();
    assert_eq!(
        parts,
        [
            ("Hel", true),
            ("lo world ", false),
            ("aga", true),
            ("in", false)
        ]
    );
}

#[test]
fn application_can_select_a_profile_and_override_its_fraction() {
    let reader = Reader::default();
    let options: Options = serde_json::from_str(r#"{"profile":"strong","fraction":0.25}"#)
        .expect("document options decode");
    assert_eq!(
        reader
            .render("reading", OutputFormat::Html, &options)
            .expect("render profile"),
        "<strong>re</strong>ading"
    );
}

#[test]
fn public_reader_matches_the_shared_language_neutral_corpus() {
    let corpus: Corpus = serde_json::from_str(include_str!("fixtures/library.json"))
        .expect("shared corpus is valid");
    let reader = Reader::default();
    for case in corpus.render {
        let options = serde_json::from_value::<Options>(case.options);
        if let Some(expected) = case.error {
            assert_eq!(expected, "invalid_input", "{}", case.name);
            if let Ok(options) = options {
                let error = reader
                    .render(&case.text, case.format, &options)
                    .expect_err(&case.name);
                assert!(error.is_invalid_input(), "{}: {error}", case.name);
            }
        } else {
            let options = options.expect(&case.name);
            let output = reader
                .render(&case.text, case.format, &options)
                .expect(&case.name);
            assert_eq!(Some(output), case.output, "{}", case.name);
        }
    }
    for case in corpus.segments {
        let actual = reader
            .segments(&case.text, &case.options)
            .expect(&case.name);
        assert_eq!(actual, case.output, "{}", case.name);
    }
}

#[test]
fn markdown_segments_keep_code_protected_and_source_literal() {
    let options: Options =
        serde_json::from_str(r#"{"input_format":"markdown"}"#).expect("Markdown options decode");
    let segments = Reader::default()
        .segments("# Hello `world`\n", &options)
        .expect("Markdown segments are available");
    let parts: Vec<(&str, bool)> = segments
        .iter()
        .map(|part| (part.text.as_str(), part.emphasized))
        .collect();
    assert_eq!(
        parts,
        [("# ", false), ("Hel", true), ("lo `world`\n", false)]
    );
}

#[cfg(feature = "config")]
#[test]
fn application_can_explicitly_parse_cli_preferences() {
    let options = Options::parse_config("profile = 'strong'\nfraction = 0.25\n")
        .expect("explicit configuration parses");
    assert_eq!(
        Reader::default()
            .render("reading", OutputFormat::Html, &options)
            .expect("configured document renders"),
        "<strong>re</strong>ading"
    );
}

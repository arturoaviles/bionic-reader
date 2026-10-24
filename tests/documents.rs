//! Integration tests for safe document conversion and incremental control filtering.

use std::io::{Cursor, Read, Write};
use std::ops::Range;

use bionic_reader::document::{ControlFilter, render, render_stream, render_with_segmenter};
use bionic_reader::segmentation::WordSegmenter;
use bionic_reader::settings::{ControlPolicy, InputFormat, OutputFormat, Profile, Settings, Style};
use pulldown_cmark::{Event, Parser};

fn settings(input_format: InputFormat, format: OutputFormat) -> Settings {
    Settings {
        input_format,
        format,
        style: Style::Always,
        ..Settings::default()
    }
}

fn markdown_visible_text(source: &[u8]) -> String {
    let source = std::str::from_utf8(source).expect("test Markdown is valid UTF-8");
    let mut visible = String::new();
    for event in Parser::new(source) {
        match event {
            Event::Text(text) | Event::Code(text) | Event::Html(text) | Event::InlineHtml(text) => {
                visible.push_str(&text);
            }
            _ => {}
        }
    }
    visible
}

struct ChunkedReader<'a> {
    bytes: &'a [u8],
    width: usize,
}

impl Read for ChunkedReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let count = self.width.min(buf.len()).min(self.bytes.len());
        buf[..count].copy_from_slice(&self.bytes[..count]);
        self.bytes = &self.bytes[count..];
        Ok(count)
    }
}

#[derive(Debug)]
struct FinalTokenSegmenter;

impl WordSegmenter for FinalTokenSegmenter {
    fn word_ranges(&self, text: &str) -> Vec<Range<usize>> {
        text.rfind("beta")
            .map(|start| std::iter::once(start..start + "beta".len()).collect())
            .unwrap_or_default()
    }
}

#[test]
fn safe_control_filter_preserves_sgr_and_safe_links_only() {
    let mut filter = ControlFilter::new(ControlPolicy::Safe);
    let mut output = filter.push(b"\x1b[31mred\x1b]52;c;secret");
    output.extend(filter.push(b"\x07\x1b[2Jsafe\x1b]8;;https://example.test\x1b\\link"));
    output.extend(filter.push(b"\x1b]8;;javascript:alert(1)\x07tail"));
    output.extend(filter.finish());

    assert_eq!(
        output,
        b"\x1b[31mredsafe\x1b]8;;https://example.test\x1b\\linktail"
    );
}

#[test]
fn preserve_policy_returns_every_control_byte() {
    let input = b"x\x1b]52;c;secret\x07\x1b[2J\xff";
    let mut filter = ControlFilter::new(ControlPolicy::Preserve);
    let mut output = filter.push(&input[..5]);
    output.extend(filter.push(&input[5..]));
    output.extend(filter.finish());
    assert_eq!(output, input);
}

#[test]
fn plain_text_filter_removes_safe_styling_and_hyperlinks() {
    let input = b"plain\x1b[31mred\x1b[0m\x1b]8;;https://example.test\x1b\\link\x1b]8;;\x1b\\\xff";
    let mut filter = ControlFilter::plain_text();
    let mut output = filter.push(&input[..19]);
    output.extend(filter.push(&input[19..]));
    output.extend(filter.finish());
    assert_eq!(output, b"plainredlink\xff");
}

#[test]
fn safe_policy_removes_non_layout_controls_without_corrupting_utf8() {
    let mut filter = ControlFilter::new(ControlPolicy::Safe);
    let first = "A\tß\nX\r\0\u{0008}\u{000b}\u{000c}\u{001f}\u{007f}\u{0080}".as_bytes();
    let second = "\u{009b}2J\u{009d}52;c;hidden\u{009c}👩‍💻Z".as_bytes();
    let mut output = filter.push(first);
    output.extend(filter.push(&second[..1]));
    output.extend(filter.push(&second[1..3]));
    output.extend(filter.push(&second[3..]));
    output.extend(filter.finish());

    assert_eq!(output, "A\tß\nX\r👩‍💻Z".as_bytes());
}

#[test]
fn safe_policy_preserves_only_sgr_among_c1_csi_controls() {
    let input = b"before\x9b31mafter\x9b2Jdone";
    let mut filter = ControlFilter::new(ControlPolicy::Safe);
    let mut output = filter.push(input);
    output.extend(filter.finish());
    assert_eq!(output, b"before\x9b31mafterdone");
}

#[test]
fn safe_policy_keeps_c1_osc8_and_drops_c1_osc52_and_dcs_payloads() {
    let input = b"A\x9d8;;https://example.test\x9cB\x9d52;c;secret\x9cC\x90private\x9cD";
    let mut filter = ControlFilter::new(ControlPolicy::Safe);
    let mut output = filter.push(input);
    output.extend(filter.finish());
    assert_eq!(output, b"A\x9d8;;https://example.test\x9cBCD");
}

#[test]
fn plain_output_removes_controls_and_preserves_invalid_utf8() {
    let settings = settings(InputFormat::Text, OutputFormat::Plain);
    let output = render(b"a\xff\x1b[31mred\x1b]52;c;hidden\x07!", &settings, true)
        .expect("render plain bytes");
    assert_eq!(output, [b'a', 0xff, b'r', b'e', b'd', b'!']);
}

#[test]
fn markdown_emphasis_is_idempotent_even_with_an_adjacent_suffix() {
    let settings = Settings {
        input_format: InputFormat::Markdown,
        format: OutputFormat::Markdown,
        fraction: 0.5,
        ..Settings::default()
    };
    let first = render(b"reading", &settings, true).expect("render Markdown source");
    assert_eq!(first, b"**read**ing");
    let second = render(&first, &settings, true).expect("render already emphasized source");
    assert_eq!(second, first);

    for source in [b"**read**ing".as_slice(), b"read**ing**"] {
        let output = render(source, &settings, true).expect("keep existing strong word intact");
        assert_eq!(output, source);
    }
}

#[test]
fn markdown_frequency_is_idempotent_after_generated_partial_strong_prefixes() {
    for every in [2, 3] {
        let first_pass_settings = Settings {
            input_format: InputFormat::Text,
            format: OutputFormat::Markdown,
            fraction: 0.5,
            every,
            min_word_length: 2,
            ..Settings::default()
        };
        let source = b"documentation reading reflection response\r\nfinal word";
        let first = render(source, &first_pass_settings, true).expect("render Markdown text");

        let repeat_settings = Settings {
            input_format: InputFormat::Markdown,
            ..first_pass_settings
        };
        let second = render(&first, &repeat_settings, true).expect("render Markdown again");
        assert_eq!(second, first, "every={every}");

        let mut reader = Cursor::new(&first);
        let mut streamed = Vec::new();
        render_stream(&mut reader, &mut streamed, &repeat_settings, true)
            .expect("stream Markdown again");
        assert_eq!(streamed, first, "streaming every={every}");
    }
}

#[test]
fn markdown_frequency_is_idempotent_with_subtle_profile_and_long_partial_words() {
    let mut first_pass_settings = Settings {
        input_format: InputFormat::Text,
        format: OutputFormat::Markdown,
        ..Settings::default()
    };
    first_pass_settings.apply_profile(Profile::Subtle);
    let source = b"documentation internationalization misunderstanding\r\n";
    let first = render(source, &first_pass_settings, true).expect("render subtle Markdown");

    let repeat_settings = Settings {
        input_format: InputFormat::Markdown,
        ..first_pass_settings
    };
    let second = render(&first, &repeat_settings, true).expect("render subtle Markdown again");
    assert_eq!(second, first);
}

#[test]
fn html_frequency_counts_existing_and_generated_strong_words_once() {
    let text_options = Settings {
        format: OutputFormat::Markdown,
        fraction: 0.5,
        every: 2,
        min_word_length: 2,
        ..Settings::default()
    };
    let source = b"documentation reading reflection response";
    let markdown = render(source, &text_options, true).expect("generate Markdown prefixes");
    let html_options = Settings {
        input_format: InputFormat::Markdown,
        format: OutputFormat::Html,
        ..text_options
    };
    let html = render(&markdown, &html_options, true).expect("render generated Markdown as HTML");
    assert_eq!(
        html,
        b"<p><strong>documen</strong>tation reading <strong>refle</strong>ction response</p>\n"
    );
}

#[test]
fn disabled_markdown_rendering_keeps_generated_emphasis_unchanged() {
    let options = settings(InputFormat::Markdown, OutputFormat::Markdown);
    let source = b"**on**e two **thr**ee four";
    assert_eq!(
        render(source, &options, false).expect("preserve existing Markdown emphasis"),
        source
    );
}

#[test]
fn existing_strong_words_advance_markdown_frequency() {
    let options = Settings {
        input_format: InputFormat::Markdown,
        format: OutputFormat::Markdown,
        fraction: 0.5,
        every: 2,
        min_word_length: 2,
        ..Settings::default()
    };
    let source = b"**documentation** reading reflection response\r\n";
    let output = render(source, &options, true).expect("render Markdown with existing strong");
    assert_eq!(
        output,
        b"**documentation** reading **refle**ction response\r\n"
    );
}

#[test]
fn markdown_slice_api_enforces_its_size_limit() {
    let options = settings(InputFormat::Markdown, OutputFormat::Markdown);
    let input = vec![b'x'; 8 * 1024 * 1024 + 1];
    let error = render(&input, &options, false).expect_err("large Markdown slice is rejected");
    assert!(error.is_invalid_input());
    assert!(error.to_string().contains("render_stream"));
}

#[test]
fn markdown_to_markdown_rejects_code_emphasis_when_skip_code_is_false() {
    let options = Settings {
        input_format: InputFormat::Markdown,
        format: OutputFormat::Markdown,
        skip_code: false,
        ..Settings::default()
    };
    let error = render(b"`reading`", &options, true).expect_err("unsupported code emphasis");
    assert!(error.is_invalid_input());
    assert!(error.to_string().contains("skip_code = true"));

    let mut reader = Cursor::new(b"```text\nreading\n```".as_slice());
    let mut output = Vec::new();
    let error = render_stream(&mut reader, &mut output, &options, true)
        .expect_err("stream rejects unsupported code emphasis");
    assert!(error.is_invalid_input());
}

#[test]
fn markdown_keeps_destinations_and_code_unchanged() {
    let settings = Settings {
        input_format: InputFormat::Markdown,
        format: OutputFormat::Markdown,
        fraction: 1.0,
        ..Settings::default()
    };
    let source =
        b"[reading](https://example.test/reading) `reading`\r\n\r\n```text\r\nreading\r\n```";
    let output = render(source, &settings, true).expect("render Markdown source");
    let output = String::from_utf8(output).expect("Markdown remains UTF-8");
    assert!(output.contains("[**reading**](https://example.test/reading)"));
    assert!(output.contains("`reading`"));
    assert!(output.contains("\r\n```text\r\nreading\r\n```"));
}

#[test]
fn markdown_plain_output_preserves_source_syntax_entities_and_escapes() {
    let source = b"&amp; \\*reading\\*";
    let plain = settings(InputFormat::Markdown, OutputFormat::Plain);
    let original = b"# **reading** &amp; \\*escaped\\* [link](./guide)\r\n";
    let markdown = settings(InputFormat::Markdown, OutputFormat::Markdown);
    let rendered = render(source, &markdown, true).expect("render escaped Markdown source");
    assert_eq!(
        markdown_visible_text(&rendered),
        markdown_visible_text(source)
    );
    assert_eq!(
        render(source, &plain, false).expect("preserve escaped Markdown source"),
        source
    );
    assert_eq!(
        render(original, &plain, false).expect("preserve Markdown source as plain output"),
        original
    );
}

#[test]
fn markdown_plain_stream_preserves_invalid_utf8_and_strips_controls_only() {
    let options = settings(InputFormat::Markdown, OutputFormat::Plain);
    let input = b"# title\xff\x1b[31mred\x1b[0m\r\n";
    let expected = b"# title\xffred\r\n";
    let mut reader = Cursor::new(input);
    let mut output = Vec::new();
    render_stream(&mut reader, &mut output, &options, true)
        .expect("stream Markdown source as original plain bytes");
    assert_eq!(output, expected);
}

#[test]
fn html_output_escapes_markup_and_blocks_unsafe_links() {
    let settings = settings(InputFormat::Markdown, OutputFormat::Html);
    let output = render(
        b"[click](javascript:alert(1)) <script>alert(1)</script>",
        &settings,
        true,
    )
    .expect("render safe HTML");
    let output = String::from_utf8(output).expect("HTML remains UTF-8");
    assert!(!output.contains("javascript:"));
    assert!(!output.contains("<script>"));
    assert!(output.contains("&lt;script&gt;"), "{output:?}");
    assert!(output.contains("&lt;/script&gt;"), "{output:?}");
}

#[test]
fn html_links_reject_active_schemes_and_control_characters() {
    let settings = settings(InputFormat::Markdown, OutputFormat::Html);
    for destination in [
        "javascript:alert(1)",
        "data:text/html,hello",
        "vbscript:msgbox(1)",
        "file:///etc/passwd",
        "java\\script:alert(1)",
    ] {
        let source = format!("[text]({destination})");
        let output = render(source.as_bytes(), &settings, false).expect("render safe HTML link");
        let output = String::from_utf8(output).expect("HTML remains UTF-8");
        assert!(output.contains("<a href=\"\">"), "{destination}: {output}");
        assert!(!output.contains(destination), "{destination}: {output}");
    }
    let relative = render(b"[local](./guide/page)", &settings, false).expect("safe relative link");
    assert!(
        String::from_utf8(relative)
            .expect("HTML remains UTF-8")
            .contains("href=\"./guide/page\"")
    );
}

#[test]
fn html_image_alt_text_is_never_wrapped_in_emphasis_markup() {
    let settings = settings(InputFormat::Markdown, OutputFormat::Html);
    let output = render(
        b"![reading](https://example.test/image.png)",
        &settings,
        true,
    )
    .expect("render safe image markup");
    let output = String::from_utf8(output).expect("HTML remains UTF-8");
    assert!(output.contains("alt=\"reading\""), "{output}");
    assert!(!output.contains("alt=\"<strong>"), "{output}");
}

#[test]
fn markdown_image_alt_text_is_not_bolded() {
    let settings = Settings {
        input_format: InputFormat::Markdown,
        format: OutputFormat::Markdown,
        fraction: 1.0,
        ..Settings::default()
    };
    let source = b"![reading](https://example.test/image.png)";
    assert_eq!(
        render(source, &settings, true).expect("keep image alt text unchanged"),
        source
    );
}

#[test]
fn text_to_html_escapes_untrusted_text() {
    let settings = settings(InputFormat::Text, OutputFormat::Html);
    let output = render(b"<img src=x onerror=alert(1)>", &settings, false)
        .expect("render escaped HTML text");
    assert_eq!(output, b"&lt;img src=x onerror=alert(1)&gt;");
}

#[test]
fn markdown_text_conversion_handles_multilingual_text() {
    let options = settings(InputFormat::Text, OutputFormat::Markdown);
    let input = "你好，世界。 القراءة مهمة. reading";
    let markdown = render(input.as_bytes(), &options, true).expect("render multilingual text");
    let markdown = String::from_utf8(markdown).expect("Markdown remains UTF-8");
    assert!(markdown.contains("**你**好，"));
    assert!(markdown.contains("ال"));
    assert!(markdown.contains("**read**ing"));
    let plain = settings(InputFormat::Markdown, OutputFormat::Plain);
    assert_eq!(
        render(markdown.as_bytes(), &plain, false).expect("recover visible Markdown text"),
        markdown.as_bytes()
    );
}

#[test]
fn terminal_input_preserves_controls_for_ansi_output() {
    let mut options = settings(InputFormat::Terminal, OutputFormat::Ansi);
    options.controls = ControlPolicy::Safe;
    let output = render(b"\x1b[2Jscreen", &options, false).expect("preserve terminal input");
    assert_eq!(output, b"\x1b[2Jscreen");
    let mut reader = Cursor::new(b"\x1b[2Jscreen");
    let mut streamed = Vec::new();
    render_stream(&mut reader, &mut streamed, &options, false)
        .expect("preserve streamed terminal input");
    assert_eq!(streamed, output);
}

#[test]
fn stream_plain_output_matches_document_output() {
    let settings = settings(InputFormat::Text, OutputFormat::Plain);
    let input = b"text\xff\x1b[31mred\x1b[0m";
    let expected = render(input, &settings, true).expect("render plain document");
    let mut reader = Cursor::new(input);
    let mut output = Vec::new();
    render_stream(&mut reader, &mut output, &settings, true).expect("render plain stream");
    assert_eq!(output, expected);
}

#[test]
fn stream_markdown_keeps_a_word_whole_across_read_chunks() {
    let settings = settings(InputFormat::Text, OutputFormat::Markdown);
    let mut input = vec![b' '; 16 * 1024 - 3];
    input.extend_from_slice(b"reading");
    let mut reader = Cursor::new(input);
    let mut output = Vec::new();
    render_stream(&mut reader, &mut output, &settings, true)
        .expect("render Markdown stream across word boundary");
    assert!(
        String::from_utf8(output)
            .expect("Markdown remains UTF-8")
            .ends_with("**read**ing")
    );
}

#[test]
fn stream_text_to_markdown_has_a_bounded_word_buffer_not_a_document_cap() {
    let settings = settings(InputFormat::Text, OutputFormat::Markdown);
    let input = vec![b'a'; 8 * 1024 * 1024 + 1];
    let mut reader = Cursor::new(input.as_slice());
    let mut output = Vec::new();
    render_stream(&mut reader, &mut output, &settings, true).expect("stream a large text document");
    assert_eq!(output, input);
}

#[test]
fn text_to_markdown_stream_matches_slice_render_for_unicode_and_connector_words() {
    let options = Settings {
        format: OutputFormat::Markdown,
        ..Settings::default()
    };
    for source in ["Aਏ{aa'a", "𐒰'\u{a947}A"] {
        let expected = render(source.as_bytes(), &options, true).expect("render complete text");
        let mut reader = ChunkedReader {
            bytes: source.as_bytes(),
            width: 1,
        };
        let mut actual = Vec::new();
        render_stream(&mut reader, &mut actual, &options, true)
            .expect("render text across byte-sized reads");
        assert_eq!(actual, expected, "source={source:?}");
    }
}

#[test]
fn stream_markdown_accepts_documents_larger_than_eight_megabytes() {
    let settings = settings(InputFormat::Markdown, OutputFormat::Markdown);
    let input = vec![b'a'; 8 * 1024 * 1024 + 1];
    let mut reader = Cursor::new(input.as_slice());
    let mut output = Vec::new();
    render_stream(&mut reader, &mut output, &settings, true)
        .expect("large Markdown document streams with oversized-block fallback");
    assert_eq!(output, input);
}

#[test]
fn stream_markdown_renders_large_documents_from_bounded_blocks() {
    let settings = Settings {
        input_format: InputFormat::Markdown,
        format: OutputFormat::Markdown,
        fraction: 0.5,
        ..Settings::default()
    };
    let mut input = Vec::new();
    let code_line = vec![b'x'; 900 * 1024];
    for _ in 0..10 {
        input.extend_from_slice(b"```text\n");
        input.extend_from_slice(&code_line);
        input.extend_from_slice(b"\n```\n\nreading\n\n");
    }
    assert!(input.len() > 8 * 1024 * 1024);
    let mut reader = Cursor::new(input);
    let mut output = Vec::new();
    render_stream(&mut reader, &mut output, &settings, true)
        .expect("render large Markdown document as bounded blocks");
    assert!(
        output
            .windows(b"**read**ing".len())
            .any(|window| window == b"**read**ing")
    );
}

#[test]
fn stream_markdown_fallback_escapes_oversized_html_blocks() {
    let settings = settings(InputFormat::Markdown, OutputFormat::Html);
    let mut input = vec![b'x'; 1024 * 1024 + 64];
    input.extend_from_slice(b"<script>alert(1)</script>");
    let mut reader = Cursor::new(input);
    let mut output = Vec::new();
    render_stream(&mut reader, &mut output, &settings, false)
        .expect("oversized Markdown block falls back safely");
    let html = String::from_utf8(output).expect("HTML output remains UTF-8");
    assert!(!html.contains("<script>"), "{html:?}");
    assert!(html.contains("&lt;script&gt;"), "{html:?}");
}

#[test]
fn stream_markdown_fallback_escapes_oversized_markdown_as_literal_text() {
    let options = settings(InputFormat::Markdown, OutputFormat::Markdown);
    let malicious = b"<script>alert(1)</script> [click](javascript:alert(1))";
    let mut input = vec![b'x'; 1024 * 1024 + 64];
    input.extend_from_slice(malicious);
    let mut reader = Cursor::new(input);
    let mut output = Vec::new();
    render_stream(&mut reader, &mut output, &options, false)
        .expect("oversized Markdown block escapes literal syntax");
    let markdown = String::from_utf8(output).expect("Markdown output remains UTF-8");
    assert!(!markdown.contains("<script>"), "{markdown:?}");
    assert!(!markdown.contains("[click](javascript:"), "{markdown:?}");
    assert!(
        markdown_visible_text(markdown.as_bytes())
            .ends_with("<script>alert(1)</script> [click](javascript:alert(1))")
    );
}

#[test]
fn stream_markdown_incomplete_reference_fallback_escapes_raw_markup_and_links() {
    let options = settings(InputFormat::Markdown, OutputFormat::Markdown);
    let malicious = b"<script>alert(1)</script> [click](javascript:alert(1))";
    let mut input = b"[oversized]: ".to_vec();
    input.extend(vec![b'x'; 1024 * 1024 + 64]);
    input.extend_from_slice(b"\n\n");
    input.extend_from_slice(malicious);
    let mut reader = Cursor::new(input);
    let mut output = Vec::new();
    render_stream(&mut reader, &mut output, &options, false)
        .expect("incomplete reference index safely falls back");
    let markdown = String::from_utf8(output).expect("Markdown output remains UTF-8");
    assert!(!markdown.contains("<script>"), "{markdown:?}");
    assert!(!markdown.contains("[click](javascript:"), "{markdown:?}");
    assert!(
        markdown_visible_text(markdown.as_bytes())
            .ends_with("<script>alert(1)</script> [click](javascript:alert(1))")
    );
}

#[test]
fn stream_markdown_tracks_code_fences_across_read_chunks() {
    let settings = Settings {
        input_format: InputFormat::Markdown,
        format: OutputFormat::Markdown,
        fraction: 1.0,
        ..Settings::default()
    };
    let input = b"```text\r\nreading\r\n\r\n```\r\n\r\nreading";
    let mut reader = Cursor::new(input);
    let mut output = Vec::new();
    render_stream(&mut reader, &mut output, &settings, true)
        .expect("render fenced Markdown stream");
    assert_eq!(output, b"```text\r\nreading\r\n\r\n```\r\n\r\n**reading**");
}

#[test]
fn stream_markdown_matches_slice_render_for_common_blocks() {
    let input = b"# Heading reading\r\n\r\n- First reading\r\n- Second item\r\n\r\nA [linked reading](https://example.test) and `code`.\r\n\r\n```text\r\nreading\r\n\r\nmore\r\n```\r\n";
    for format in [
        OutputFormat::Markdown,
        OutputFormat::Html,
        OutputFormat::Ansi,
    ] {
        let options = settings(InputFormat::Markdown, format);
        let expected = render(input, &options, true).expect("render complete Markdown source");
        let mut reader = Cursor::new(input);
        let mut actual = Vec::new();
        render_stream(&mut reader, &mut actual, &options, true)
            .expect("render Markdown incrementally");
        assert_eq!(actual, expected, "format={format:?}");
    }
}

#[test]
fn stream_markdown_resolves_forward_reference_links() {
    let input = b"[linked reading][guide]\n\n[guide]: https://example.test/guide\n\nreading";
    for format in [OutputFormat::Markdown, OutputFormat::Html] {
        let options = settings(InputFormat::Markdown, format);
        let expected = render(input, &options, true).expect("render complete Markdown source");
        let mut reader = Cursor::new(input);
        let mut actual = Vec::new();
        render_stream(&mut reader, &mut actual, &options, true)
            .expect("resolve references while streaming Markdown");
        assert_eq!(actual, expected, "format={format:?}");
        if format == OutputFormat::Html {
            assert!(
                String::from_utf8_lossy(&actual).contains("href=\"https://example.test/guide\"")
            );
        }
    }
}

#[test]
fn stream_markdown_rejects_unsafe_forward_reference_destinations() {
    let options = settings(InputFormat::Markdown, OutputFormat::Html);
    let source = b"[click][bad]\n\n[bad]: javascript:alert(1)";
    let mut reader = Cursor::new(source);
    let mut output = Vec::new();
    render_stream(&mut reader, &mut output, &options, false)
        .expect("reject active forward-reference URL");
    let html = String::from_utf8(output).expect("safe HTML remains UTF-8");
    assert!(html.contains("<a href=\"\">click</a>"), "{html}");
    assert!(!html.contains("javascript:"), "{html}");
}

#[test]
fn stream_markdown_ignores_reference_looking_lines_inside_fences() {
    let options = settings(InputFormat::Markdown, OutputFormat::Html);
    let source = b"```text\n[guide]: javascript:alert(1)\n```\n\n[guide]\n";
    let mut reader = Cursor::new(source);
    let mut actual = Vec::new();
    render_stream(&mut reader, &mut actual, &options, false)
        .expect("ignore code fence text while indexing references");
    let actual = String::from_utf8(actual).expect("safe HTML remains UTF-8");
    assert!(!actual.contains("href=\"javascript:"), "{actual}");
    assert!(
        actual.contains("<pre><code class=\"language-text\">"),
        "{actual}"
    );
}

#[derive(Default)]
struct FlushCountingWriter {
    bytes: Vec<u8>,
    flushes: usize,
}

impl Write for FlushCountingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.bytes.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.flushes += 1;
        Ok(())
    }
}

#[test]
fn stream_flushes_each_emitted_terminal_chunk() {
    let settings = settings(InputFormat::Text, OutputFormat::Ansi);
    let mut reader = Cursor::new(b"reading ");
    let mut output = FlushCountingWriter::default();
    render_stream(&mut reader, &mut output, &settings, true)
        .expect("render an interactive terminal chunk");
    assert_eq!(output.bytes, b"\x1b[1mread\x1b[22ming ");
    assert!(output.flushes > 0, "an emitted chunk should be flushed");
}

#[test]
fn oversized_controls_have_chunk_independent_limits_and_split_terminators() {
    for introducer in [b"\x1b]".as_slice(), b"\x1bP", b"\x9f"] {
        for terminator in [b"\x1b\\".as_slice(), b"\xc2\x9c"] {
            let mut input = introducer.to_vec();
            input.extend(std::iter::repeat_n(b'x', 9000));
            input.extend_from_slice(terminator);
            input.extend_from_slice(b"reading");
            let mut whole = ControlFilter::new(ControlPolicy::Safe);
            let mut expected = whole.push(&input);
            expected.extend(whole.finish());
            assert_eq!(expected, b"reading");
            for width in [1, 2, 31, 8193] {
                let mut filter = ControlFilter::new(ControlPolicy::Safe);
                let mut actual = Vec::new();
                for chunk in input.chunks(width) {
                    actual.extend(filter.push(chunk));
                }
                actual.extend(filter.finish());
                assert_eq!(actual, expected, "width={width}");
            }
        }
    }
    let mut input = b"\x1b[".to_vec();
    input.extend(std::iter::repeat_n(b'1', 9000));
    input.extend_from_slice(b"mreading");
    let mut filter = ControlFilter::new(ControlPolicy::Safe);
    assert_eq!(filter.push(&input), b"reading");
}

#[test]
fn markdown_ansi_preserves_every_source_byte_except_added_emphasis() {
    let source = b"# Heading\r\n\r\n- A [reading](https://example.test) &amp; `code`\r\n\r\n```rust\r\nlet value = 1;\r\n```\r\n";
    let options = settings(InputFormat::Markdown, OutputFormat::Ansi);
    let output = render(source, &options, true).expect("render Markdown as ANSI");
    let mut strip = ControlFilter::plain_text();
    let mut plain = strip.push(&output);
    plain.extend(strip.finish());
    assert_eq!(plain, source);
    assert!(
        output
            .windows(b"`code`".len())
            .any(|part| part == b"`code`")
    );
    let mut input = ChunkedReader {
        bytes: source,
        width: 1,
    };
    let mut streamed = Vec::new();
    render_stream(&mut input, &mut streamed, &options, true).expect("stream Markdown as ANSI");
    assert_eq!(streamed, output);
}

#[test]
fn unsafe_markdown_links_keep_surrounding_headings_lists_and_safe_links() {
    let source =
        b"# Heading\n\n- A [click](javascript:alert(1)) and [safe](https://example.test).\n";
    let options = settings(InputFormat::Markdown, OutputFormat::Markdown);
    let output = render(source, &options, false).expect("sanitize link without changing structure");
    let text = std::str::from_utf8(&output).expect("Markdown is UTF-8");
    assert!(text.starts_with("# Heading\n\n- A "), "{text}");
    assert!(text.contains("[safe](https://example.test)"), "{text}");
    let html = render(
        &output,
        &settings(InputFormat::Markdown, OutputFormat::Html),
        false,
    )
    .expect("render sanitized Markdown");
    let html = String::from_utf8(html).expect("HTML is UTF-8");
    assert!(html.contains("<h1>Heading</h1>"), "{html}");
    assert!(html.contains("<li>"), "{html}");
    assert!(!html.contains("href=\"javascript:"), "{html}");
}

struct FailingReader;

impl Read for FailingReader {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "fixture input failure",
        ))
    }
}

struct FailingWriter {
    fail_flush: bool,
}

impl Write for FailingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.fail_flush {
            Ok(buf.len())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "fixture output failure",
            ))
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "fixture flush failure",
        ))
    }
}

#[test]
fn all_stream_renderers_preserve_read_write_and_flush_error_sources() {
    for input_format in [
        InputFormat::Text,
        InputFormat::Markdown,
        InputFormat::Terminal,
    ] {
        for format in [
            OutputFormat::Ansi,
            OutputFormat::Plain,
            OutputFormat::Markdown,
            OutputFormat::Html,
        ] {
            let options = settings(input_format, format);
            let error = render_stream(&mut FailingReader, &mut Vec::new(), &options, true)
                .expect_err("input error must propagate");
            assert_eq!(error.io_kind(), Some(std::io::ErrorKind::PermissionDenied));
            for fail_flush in [false, true] {
                let error = render_stream(
                    &mut Cursor::new(b"reading "),
                    &mut FailingWriter { fail_flush },
                    &options,
                    true,
                )
                .expect_err("output error must propagate");
                assert_eq!(error.io_kind(), Some(std::io::ErrorKind::BrokenPipe));
            }
        }
    }
}

#[test]
fn markdown_reference_catalog_is_parsed_once_for_many_small_blocks() {
    let mut source = "word\n\n".repeat(2000);
    for index in 0..2000 {
        source.push_str(&format!("[ref{index}]: https://example.test/{index}\n"));
    }
    source.push_str("\n[final][ref1999]\n");
    let options = settings(InputFormat::Markdown, OutputFormat::Html);
    let mut output = Vec::new();
    render_stream(
        &mut Cursor::new(source.as_bytes()),
        &mut output,
        &options,
        false,
    )
    .expect("stream many blocks with a large reference index");
    let html = String::from_utf8(output).expect("HTML is UTF-8");
    assert_eq!(html.matches("<p>word</p>").count(), 2000);
    assert!(html.contains("href=\"https://example.test/1999\""));
}

#[test]
fn nested_html_and_images_in_unsafe_links_do_not_create_overlapping_source_edits() {
    for source in [
        "- [<em>click</em>](javascript:alert(1))",
        "# [![image](javascript:one)](javascript:two)",
    ] {
        let options = settings(InputFormat::Markdown, OutputFormat::Markdown);
        let output =
            render(source.as_bytes(), &options, true).expect("sanitize nested unsafe content");
        let text = String::from_utf8(output).expect("output remains UTF-8");
        assert!(text.starts_with(&source[..2]), "{text}");
        let html = render(
            text.as_bytes(),
            &settings(InputFormat::Markdown, OutputFormat::Html),
            false,
        )
        .expect("render sanitized source");
        let html = String::from_utf8(html).expect("HTML remains UTF-8");
        assert!(!html.contains("href=\"javascript:"), "{html}");
        assert!(!html.contains("src=\"javascript:"), "{html}");
    }
}

#[test]
fn ansi_code_opt_in_emphasizes_inline_indented_and_fenced_code_without_rewriting() {
    let options = Settings {
        skip_code: false,
        fraction: 1.0,
        ..settings(InputFormat::Markdown, OutputFormat::Ansi)
    };
    for source in ["`reading`", "    reading\n", "```rust\nreading\n```"] {
        let output = render(source.as_bytes(), &options, true).expect("emphasize code source");
        assert!(
            output
                .windows(b"\x1b[1mreading\x1b[22m".len())
                .any(|bytes| bytes == b"\x1b[1mreading\x1b[22m"),
            "{output:?}"
        );
        let mut strip = ControlFilter::plain_text();
        let mut plain = strip.push(&output);
        plain.extend(strip.finish());
        assert_eq!(plain, source.as_bytes());
    }
}

#[test]
fn custom_segmenter_drives_markdown_and_html_document_spans() {
    let segmenter = FinalTokenSegmenter;
    let source = b"alpha beta";
    for (input_format, format, expected) in [
        (
            InputFormat::Text,
            OutputFormat::Markdown,
            b"alpha **beta**".as_slice(),
        ),
        (
            InputFormat::Text,
            OutputFormat::Html,
            b"alpha <strong>beta</strong>".as_slice(),
        ),
        (
            InputFormat::Markdown,
            OutputFormat::Markdown,
            b"alpha **beta**".as_slice(),
        ),
        (
            InputFormat::Markdown,
            OutputFormat::Html,
            b"<p>alpha <strong>beta</strong></p>\n".as_slice(),
        ),
    ] {
        let options = Settings {
            fraction: 1.0,
            input_format,
            format,
            ..Settings::default()
        };
        assert_eq!(
            render_with_segmenter(source, &options, true, &segmenter)
                .expect("render with caller-supplied word boundaries"),
            expected,
            "input_format={input_format:?}, format={format:?}"
        );
    }

    let options = settings(InputFormat::Text, OutputFormat::Markdown);
    assert_eq!(
        render_with_segmenter(source, &options, false, &segmenter)
            .expect("disable only added emphasis"),
        source
    );
}

#[test]
fn plain_output_strips_controls_even_when_preserve_is_requested() {
    let options = Settings {
        input_format: InputFormat::Terminal,
        format: OutputFormat::Plain,
        controls: ControlPolicy::Preserve,
        ..Settings::default()
    };
    let source = b"one\x1b[31mtwo\x1b]8;;https://example.test\x1b\\link\x1b]8;;\x1b\\\x07\xff";
    assert_eq!(
        render(source, &options, true).expect("plain output drops terminal controls"),
        b"onetwolink\xff"
    );
    let mut reader = ChunkedReader {
        bytes: source,
        width: 2,
    };
    let mut output = Vec::new();
    render_stream(&mut reader, &mut output, &options, true)
        .expect("plain stream drops terminal controls");
    assert_eq!(output, b"onetwolink\xff");
}

#[test]
fn markdown_output_escapes_text_syntax_when_emphasis_is_disabled() {
    let options = settings(InputFormat::Text, OutputFormat::Markdown);
    let source = b"# [alpha] *beta* <tag>";
    let output = render(source, &options, false).expect("escape Markdown control syntax");
    assert_eq!(output, b"\\# \\[alpha\\] \\*beta\\* \\<tag\\>");
    assert_eq!(
        markdown_visible_text(&output),
        String::from_utf8_lossy(source)
    );
}

#[test]
fn document_ansi_applies_the_selected_control_policy() {
    let source = b"before\x1b[2Jafter";
    let safe = settings(InputFormat::Text, OutputFormat::Ansi);
    assert_eq!(
        render(source, &safe, false).expect("filter unsafe terminal cursor control"),
        b"beforeafter"
    );

    let preserve = Settings {
        controls: ControlPolicy::Preserve,
        ..safe
    };
    assert_eq!(
        render(source, &preserve, false).expect("preserve explicit terminal controls"),
        source
    );
}

#[test]
fn incomplete_escape_sequences_are_dropped_only_when_the_stream_finishes() {
    for incomplete in [b"\x1b".as_slice(), b"\x1b[".as_slice(), b"\x1b]52;c;secret"] {
        let mut filter = ControlFilter::new(ControlPolicy::Safe);
        assert_eq!(filter.push(b"prefix"), b"prefix");
        assert!(filter.push(incomplete).is_empty());
        assert!(filter.finish().is_empty());
    }

    let mut malformed_utf8 = ControlFilter::new(ControlPolicy::Safe);
    let bytes = b"x\xe2(\xa1y\xc2";
    let mut output = malformed_utf8.push(bytes);
    output.extend(malformed_utf8.finish());
    assert_eq!(output, bytes);
}

#[test]
fn oversized_intermediate_escape_stops_draining_at_an_invalid_boundary() {
    let mut input = b"\x1b(".to_vec();
    input.extend(std::iter::repeat_n(b'(', 9000));
    input.extend_from_slice(b"\0kept");

    let mut whole = ControlFilter::new(ControlPolicy::Safe);
    let mut expected = whole.push(&input);
    expected.extend(whole.finish());
    assert_eq!(expected, b"kept");

    for width in [1, 2, 31, 8193, input.len()] {
        let mut filter = ControlFilter::new(ControlPolicy::Safe);
        let mut output = Vec::new();
        for chunk in input.chunks(width) {
            output.extend(filter.push(chunk));
        }
        output.extend(filter.finish());
        assert_eq!(output, expected, "width={width}");
    }

    let mut preserve = ControlFilter::new(ControlPolicy::Preserve);
    let mut preserved = preserve.push(&input);
    preserved.extend(preserve.finish());
    assert_eq!(preserved, input);
}

#[test]
fn string_controls_ignore_utf8_continuation_bytes_that_equal_c1_st() {
    let mut safe_link = b"\x1b]8;;https://example.test/".to_vec();
    safe_link.extend_from_slice("Üₜ".as_bytes());
    safe_link.extend_from_slice(b"\x1b\\link\x1b]8;;\x1b\\");

    let mut osc_title = b"\x1b]0;".to_vec();
    osc_title.extend_from_slice("Üₜsecret".as_bytes());
    osc_title.extend_from_slice(b"\x07visible");
    let mut dcs_payload = b"\x1bP".to_vec();
    dcs_payload.extend_from_slice("Üₜsecret".as_bytes());
    dcs_payload.extend_from_slice(b"\x1b\\visible");
    let encoded_st = b"\x1b]0;title\xc2\x9cvisible".to_vec();
    for (input, expected) in [
        (safe_link.as_slice(), safe_link.as_slice()),
        (osc_title.as_slice(), b"visible".as_slice()),
        (dcs_payload.as_slice(), b"visible".as_slice()),
        (encoded_st.as_slice(), b"visible".as_slice()),
    ] {
        for width in [input.len(), 1, 2, 3] {
            let mut filter = ControlFilter::new(ControlPolicy::Safe);
            let mut output = Vec::new();
            for chunk in input.chunks(width) {
                output.extend(filter.push(chunk));
            }
            output.extend(filter.finish());
            assert_eq!(output, expected, "width={width}, input={input:?}");
        }
    }

    // A raw C1 ST still closes OSC when it is not the continuation of a valid UTF-8 scalar.
    let mut raw_st = b"\x1b]0;title".to_vec();
    raw_st.push(0x9c);
    raw_st.extend_from_slice(b"visible");
    let mut filter = ControlFilter::new(ControlPolicy::Safe);
    let mut output = filter.push(&raw_st);
    output.extend(filter.finish());
    assert_eq!(output, b"visible");
}

#[test]
fn oversized_string_draining_keeps_utf8_context_for_c1_st_detection() {
    for (introducer, terminator) in [
        (b"\x1b]0;".as_slice(), b"\x07".as_slice()),
        (b"\x1bP".as_slice(), b"\x1b\\".as_slice()),
    ] {
        let mut filter = ControlFilter::new(ControlPolicy::Safe);
        let mut oversized = introducer.to_vec();
        oversized.extend(std::iter::repeat_n(b'x', 9000));
        assert!(filter.push(&oversized).is_empty());

        let mut output = filter.push("Üₜsecret".as_bytes());
        let mut closing = terminator.to_vec();
        closing.extend_from_slice(b"visible");
        output.extend(filter.push(&closing));
        output.extend(filter.finish());
        assert_eq!(output, b"visible", "introducer={introducer:?}");
    }
}

#[test]
fn oversized_string_transition_keeps_a_partial_utf8_prefix_for_c1_st_detection() {
    for (introducer, terminator) in [
        (b"\x1b]0;".as_slice(), b"\x07".as_slice()),
        (b"\x1bP".as_slice(), b"\x1b\\".as_slice()),
    ] {
        for partial_utf8 in [b"\xc3".as_slice(), b"\xe2\x82"] {
            let mut oversized = introducer.to_vec();
            oversized.extend(std::iter::repeat_n(b'x', 9000));
            oversized.extend_from_slice(partial_utf8);

            let mut tail = vec![0x9c];
            tail.extend_from_slice(b"secret");
            tail.extend_from_slice(terminator);
            tail.extend_from_slice(b"visible");

            for width in 1..=tail.len() {
                let mut filter = ControlFilter::new(ControlPolicy::Safe);
                assert!(filter.push(&oversized).is_empty());
                let mut output = Vec::new();
                for chunk in tail.chunks(width) {
                    output.extend(filter.push(chunk));
                }
                output.extend(filter.finish());
                assert_eq!(
                    output, b"visible",
                    "introducer={introducer:?}, partial={partial_utf8:?}, width={width}"
                );
            }
        }
    }
}

#[test]
fn reference_catalog_byte_limit_falls_back_safely_across_many_definitions() {
    let mut source = String::new();
    let destination_tail = "x".repeat(950);
    for index in 0..1200 {
        source.push_str(&format!(
            "[r{index}]: https://example.test/{destination_tail}\n"
        ));
    }
    source.push_str("\n<script>alert(1)</script> [click](javascript:alert(1))");
    let options = settings(InputFormat::Markdown, OutputFormat::Markdown);
    let mut output = Vec::new();
    render_stream(
        &mut Cursor::new(source.as_bytes()),
        &mut output,
        &options,
        false,
    )
    .expect("oversized reference catalog falls back without unsafe markup");
    let markdown = String::from_utf8(output).expect("Markdown output remains UTF-8");
    assert!(!markdown.contains("<script>"), "{markdown:?}");
    assert!(!markdown.contains("[click](javascript:"), "{markdown:?}");
    assert!(
        markdown_visible_text(markdown.as_bytes())
            .ends_with("<script>alert(1)</script> [click](javascript:alert(1))")
    );
}

#[test]
fn render_entry_points_reject_invalid_settings_before_reading_or_writing() {
    let options = Settings {
        every: 0,
        ..Settings::default()
    };
    let error = render(b"reading", &options, true).expect_err("invalid every setting");
    assert!(error.is_invalid_input());

    let mut input = Cursor::new(b"reading");
    let mut output = Vec::new();
    let error = render_stream(&mut input, &mut output, &options, true)
        .expect_err("stream rejects invalid settings before reading input");
    assert!(error.is_invalid_input());
    assert!(output.is_empty());
}

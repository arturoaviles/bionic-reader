#![no_main]

use bionic_reader::document::{self, ControlFilter};
use bionic_reader::settings::{ControlPolicy, Emphasis, InputFormat, OutputFormat, Settings};
use bionic_reader::transformer::{self, Transformer};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    let settings = Settings {
        emphasis: match input.first().copied().unwrap_or_default() % 3 {
            0 => Emphasis::Bold,
            1 => Emphasis::Underline,
            _ => Emphasis::Reverse,
        },
        ..Settings::default()
    };
    assert_eq!(transformer::transform(input, &settings, false), input);
    let expected = transformer::transform(input, &settings, true);
    let width = input
        .first()
        .map_or(1, |value| usize::from(*value) % 63 + 1);
    let mut transformer = Transformer::new(&settings, true);
    let mut output = Vec::new();
    for chunk in input.chunks(width) {
        output.extend(transformer.push(chunk));
    }
    output.extend(transformer.finish());
    assert_eq!(output, expected);

    let mut filter = ControlFilter::new(ControlPolicy::Safe);
    let mut whole = filter.push(input);
    whole.extend(filter.finish());
    let mut filter = ControlFilter::new(ControlPolicy::Safe);
    let mut chunked = Vec::new();
    for chunk in input.chunks(width) {
        chunked.extend(filter.push(chunk));
    }
    chunked.extend(filter.finish());
    assert_eq!(chunked, whole);

    if std::str::from_utf8(input).is_ok() {
        for format in [
            OutputFormat::Markdown,
            OutputFormat::Html,
            OutputFormat::Plain,
        ] {
            let options = Settings {
                input_format: InputFormat::Markdown,
                format,
                ..Settings::default()
            };
            let _ = document::render(input, &options, true);
        }
    }
});

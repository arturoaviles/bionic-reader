//! Byte-stream regressions discovered by the sanitizer fuzz target.

use bionic_reader::settings::{Emphasis, Settings};
use bionic_reader::transformer::{self, Transformer};

#[test]
fn malformed_multilingual_input_keeps_emphasis_independent_of_chunk_width() {
    let input = &[
        0x02, 0xa0, 0xe0, 0xb8, 0xb6, 0xe0, 0xb8, 0x89, 0xe0, 0xb8, 0xb2, 0xe0, 0xb9, 0x84, 0xe0,
        0xb8, 0x91, 0x3c, 0xe2, 0x80, 0x8d, 0xf0, 0x9f, 0xa0, 0xe5, 0xa5, 0xbd, 0xe4, 0x38, 0x96,
        0xe7, 0x95, 0x8c, 0x20, 0xe6, 0x97, 0xa5, 0xe6, 0x68, 0xac, 0xe8, 0xaa, 0x9e, 0x20, 0xe0,
        0xb8, 0xa0, 0xe0, 0xb8, 0xb2, 0xe0, 0xb8, 0xa9, 0xe0, 0xb8, 0xb2, 0xe0, 0xb9, 0x84, 0xe0,
        0xb8, 0x91, 0x3c, 0x2e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xe2, 0x80, 0x8d, 0xf0,
        0x9f, 0x91, 0xa6,
    ];
    let settings = Settings {
        every: 7,
        min_word_length: 3,
        fraction: 1.0,
        emphasis: Emphasis::Reverse,
        ..Settings::default()
    };
    let minimized = "\u{0e36}ฉาไษาไ👦".as_bytes();
    let malformed_prefix = [b"\xa0".as_slice(), minimized].concat();
    for settings in [settings, Settings::default()] {
        for input in [input.as_slice(), minimized, malformed_prefix.as_slice()] {
            let expected = transformer::transform(input, &settings, true);
            for width in 1..=input.len() {
                let mut renderer = Transformer::new(&settings, true);
                let mut output = Vec::new();
                for chunk in input.chunks(width) {
                    output.extend(renderer.push(chunk));
                }
                output.extend(renderer.finish());
                assert_eq!(output, expected, "chunk width {width}");
            }
            for split in 0..=input.len() {
                let mut renderer = Transformer::new(&settings, true);
                let mut output = renderer.push(&input[..split]);
                output.extend(renderer.push(&input[split..]));
                output.extend(renderer.finish());
                assert_eq!(output, expected, "split at byte {split}");
            }
            assert_eq!(transformer::transform(input, &settings, false), input);
        }
    }
}

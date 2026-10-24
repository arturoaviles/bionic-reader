//! Throughput and allocation measurements for the incremental transformation hot path.

use bionic_reader::settings::Settings;
use bionic_reader::transformer::Transformer;
use divan::counter::BytesCount;
use divan::{AllocProfiler, Bencher};

#[global_allocator]
static ALLOCATOR: AllocProfiler = AllocProfiler::system();

fn main() {
    divan::main();
}

#[derive(Clone, Copy, Debug)]
enum Passage {
    Ascii,
    UnicodeAnsi,
    Punctuation,
    Passthrough,
}

impl Passage {
    fn input(self) -> Vec<u8> {
        match self {
            Self::Ascii | Self::Passthrough => {
                b"Reading terminal output preserves words and whitespace.\n".repeat(128)
            }
            Self::UnicodeAnsi => "\x1b[2;34mCafé déjà vu, naïve coöperation.\x1b[0m 日本語 👩‍💻\n"
                .repeat(128)
                .into_bytes(),
            Self::Punctuation => b"--- ... !!! 1234567890\t\n".repeat(256),
        }
    }
}

#[divan::bench(args = [Passage::Ascii, Passage::UnicodeAnsi, Passage::Punctuation, Passage::Passthrough], consts = [1, 64, 8192])]
fn stream<const CHUNK: usize>(bencher: Bencher, passage: Passage) {
    let input = passage.input();
    let settings = Settings::default();
    let enabled = !matches!(passage, Passage::Passthrough);
    bencher
        .counter(BytesCount::new(input.len()))
        .bench_local(|| {
            let mut transformer = Transformer::new(&settings, enabled);
            for chunk in input.chunks(CHUNK) {
                divan::black_box(transformer.push(divan::black_box(chunk)));
            }
            divan::black_box(transformer.finish());
        });
}

//! Run with `cargo run --example reading`.

use bionic_reader::reading::{Options, Reader};
use bionic_reader::settings::OutputFormat;

fn main() -> Result<(), bionic_reader::Error> {
    let reader = Reader::new();
    let options = Options::default();
    println!(
        "{}",
        reader.render("Hello world", OutputFormat::Html, &options)?
    );
    for segment in reader.segments("Hello world", &options)? {
        println!("{:?}: {}", segment.text, segment.emphasized);
    }
    Ok(())
}

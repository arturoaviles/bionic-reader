//! Private JSON command adapter for the embedded Go WebAssembly engine.

use std::io::{self, Read};

use bionic_reader::reading::{Options, Reader};
use bionic_reader::settings::OutputFormat;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Version,
    Render {
        text: String,
        format: OutputFormat,
        #[serde(default)]
        options: Options,
    },
    Segments {
        text: String,
        #[serde(default)]
        options: Options,
    },
    ParseConfig {
        contents: String,
    },
}

fn execute(input: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let value = match serde_json::from_str(input)? {
        Request::Version => json!(env!("CARGO_PKG_VERSION")),
        Request::Render {
            text,
            format,
            options,
        } => {
            json!(Reader::new().render(&text, format, &options)?)
        }
        Request::Segments { text, options } => json!(Reader::new().segments(&text, &options)?),
        Request::ParseConfig { contents } => json!(Options::parse_config(&contents)?),
    };
    Ok(value)
}

fn main() {
    let mut input = String::new();
    let result = io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)
        .and_then(|_| execute(&input));
    let response = match result {
        Ok(value) => json!({ "value": value }),
        Err(error) => json!({ "error": { "code": "invalid_input", "message": error.to_string() } }),
    };
    println!("{response}");
}

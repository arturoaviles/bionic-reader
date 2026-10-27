//! WebAssembly adapter for the shared transformation engine.

use bionic_reader::reading::{Options, Reader};
use bionic_reader::settings::OutputFormat;
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::wasm_bindgen;

fn invalid(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

/// The Cargo-owned version compiled into this engine.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// Stateless document transformations using the built-in multilingual data.
#[wasm_bindgen]
#[derive(Debug, Default)]
pub struct Engine {
    reader: Reader,
}

#[wasm_bindgen]
impl Engine {
    /// Creates a reusable engine; each document has independent frequency state.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns formatted Unicode text, or a validation diagnostic.
    pub fn render(&self, text: &str, format: &str, options: &str) -> Result<String, JsValue> {
        let format: OutputFormat = serde_json::from_value(format.into()).map_err(invalid)?;
        let options: Options = serde_json::from_str(options).map_err(invalid)?;
        self.reader.render(text, format, &options).map_err(invalid)
    }

    /// Returns JSON text segments for presentation without byte-index conversion.
    pub fn segments(&self, text: &str, options: &str) -> Result<String, JsValue> {
        let options: Options = serde_json::from_str(options).map_err(invalid)?;
        let segments = self.reader.segments(text, &options).map_err(invalid)?;
        serde_json::to_string(&segments).map_err(invalid)
    }
}

/// Parses explicitly supplied CLI TOML preferences without filesystem discovery.
#[wasm_bindgen]
pub fn parse_config(contents: &str) -> Result<String, JsValue> {
    let options = Options::parse_config(contents).map_err(invalid)?;
    serde_json::to_string(&options).map_err(invalid)
}

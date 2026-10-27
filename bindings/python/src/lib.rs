//! Native Python adapter for the shared transformation engine.

use bionic_reader::reading::{Options, Reader};
use bionic_reader::settings::OutputFormat;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::{Bound, PyModule, PyModuleMethods, PyResult};
use pyo3::{PyErr, pyclass, pyfunction, pymethods, pymodule, wrap_pyfunction};

fn invalid(error: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(error.to_string())
}

#[pyclass(module = "bionic_reader._native")]
struct NativeReader {
    reader: Reader,
}

#[pymethods]
impl NativeReader {
    #[new]
    fn new() -> Self {
        Self {
            reader: Reader::new(),
        }
    }

    fn render(&self, text: &str, format: &str, options: &str) -> PyResult<String> {
        let format: OutputFormat = serde_json::from_value(format.into()).map_err(invalid)?;
        let options: Options = serde_json::from_str(options).map_err(invalid)?;
        self.reader.render(text, format, &options).map_err(invalid)
    }

    fn segments(&self, text: &str, options: &str) -> PyResult<String> {
        let options: Options = serde_json::from_str(options).map_err(invalid)?;
        let segments = self.reader.segments(text, &options).map_err(invalid)?;
        serde_json::to_string(&segments).map_err(invalid)
    }
}

#[pyfunction]
fn parse_config(contents: &str) -> PyResult<String> {
    let options = Options::parse_config(contents).map_err(invalid)?;
    serde_json::to_string(&options).map_err(invalid)
}

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    module.add_class::<NativeReader>()?;
    module.add_function(wrap_pyfunction!(parse_config, module)?)?;
    Ok(())
}

use pyo3::exceptions::PyException;
use pyo3::prelude::*;

pub mod binary;
mod compute_name;
mod failure_message;
mod junit;
mod raw_upload;
mod testrun;
mod validated_string;
mod warning;

pub use testrun::{Outcome, PropertiesValue, Testrun};
pub use validated_string::ValidatedString;
pyo3::create_exception!(test_results_parser, ComputeNameError, PyException);

/// A Python module implemented in Rust.
#[pymodule]
fn test_results_parser(_: Python, m: &Bound<PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(raw_upload::parse_raw_upload, m)?)?;
    m.add_function(wrap_pyfunction!(failure_message::build_message, m)?)?;
    m.add_function(wrap_pyfunction!(failure_message::escape_message, m)?)?;
    m.add_function(wrap_pyfunction!(failure_message::shorten_file_paths, m)?)?;

    m.add_class::<binary::AggregationReader>()?;
    m.add_class::<binary::BinaryFormatWriter>()?;
    m.add_class::<binary::TestAggregate>()?;

    Ok(())
}

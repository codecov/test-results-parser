use pyo3::exceptions::PyException;
use pyo3::prelude::*;

mod failure_message;
mod junit;
mod testrun;

pyo3::create_exception!(test_results_parser, ParserError, PyException);

/// A Python module implemented in Rust.
#[pymodule]
fn test_results_parser(py: Python, m: &Bound<PyModule>) -> PyResult<()> {
    m.add("ParserError", py.get_type::<ParserError>())?;
    m.add_class::<testrun::Testrun>()?;
    m.add_class::<testrun::Outcome>()?;
    m.add_class::<testrun::Framework>()?;
    m.add_class::<testrun::ParsingInfo>()?;

    m.add_function(wrap_pyfunction!(junit::parse_junit_xml, m)?)?;
    m.add_function(wrap_pyfunction!(failure_message::build_message, m)?)?;
    m.add_function(wrap_pyfunction!(failure_message::escape_message, m)?)?;
    m.add_function(wrap_pyfunction!(failure_message::shorten_file_paths, m)?)?;

    Ok(())
}

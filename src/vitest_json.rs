use pyo3::prelude::*;

use serde::{Deserialize, Serialize};

use crate::helpers::{s, ParserError};
use crate::testrun::{Outcome, Testrun};

#[derive(Serialize, Deserialize, Debug)]
struct AssertionResult {
    #[serde(rename = "ancestorTitles")]
    ancestor_titles: Vec<String>,
    #[serde(rename = "fullName")]
    full_name: String,
    status: String,
    title: String,
    #[serde(rename = "duration")]
    duration_milliseconds: i64,
    #[serde(rename = "failureMessages")]
    failure_messages: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct VitestResult {
    #[serde(rename = "assertionResults")]
    assertion_results: Vec<AssertionResult>,
    name: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct VitestReport {
    #[serde(rename = "testResults")]
    test_results: Vec<VitestResult>,
}

#[pyfunction]
pub fn parse_vitest_json(file_bytes: Vec<u8>) -> PyResult<Vec<Testrun>> {
    let file_string = String::from_utf8_lossy(&file_bytes).into_owned();

    let val: VitestReport = serde_json::from_str(file_string.as_str()).map_err(|err| {
        ParserError::new_err(format!("Error parsing vitest JSON: {}", err.to_string()))
    })?;

    let testruns: Result<Vec<Testrun>, _> = val
        .test_results
        .into_iter()
        .flat_map(|result| {
            result
                .assertion_results
                .into_iter()
                .map(move |aresult| {
                    Ok(Testrun {
                        name: aresult.full_name,
                        duration: aresult.duration_milliseconds as f64 / 1000.0,
                        outcome: (match aresult.status.as_str() {
                            "failed" => Outcome::Failure,
                            "pending" => Outcome::Skip,
                            "passed" => Outcome::Pass,
                            x => {
                                return Err(ParserError::new_err(format!(
                                    "Error reading outcome. {} is an invalid value",
                                    x
                                )))
                            }
                        }),
                        testsuite: result.name.clone(),
                        failure_message: aresult.failure_messages.join(" "),
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect();

    Ok(testruns?)
}

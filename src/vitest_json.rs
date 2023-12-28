use pyo3::{exceptions::PyRuntimeError, prelude::*};

use serde::{Deserialize, Serialize};

use std::{fs::File, io::BufReader};

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
pub fn parse_vitest_json(filename: String) -> PyResult<Vec<Testrun>> {
    let f = File::open(&filename)?;
    let mut testruns: Vec<Testrun> = Vec::new();
    let reader = BufReader::new(f);

    let val: VitestReport = serde_json::from_reader(reader).unwrap();

    testruns = val
        .test_results
        .into_iter()
        .map(|result| {
            result
                .assertion_results
                .into_iter()
                .map(move |aresult| Testrun {
                    name: aresult.full_name,
                    duration: aresult.duration_milliseconds as f64 / 1000.0,
                    outcome: (match aresult.status.as_str() {
                        "failed" => Ok(Outcome::Failure),
                        "pending" => Ok(Outcome::Skip),
                        "passed" => Ok(Outcome::Pass),
                        _ => Err(PyRuntimeError::new_err("oh noooooooo")),
                    })
                    .unwrap(),
                    testsuite: result.name.clone(),
                })
                .collect::<Vec<_>>()
        })
        .flatten()
        .collect();

    Ok(testruns)
}
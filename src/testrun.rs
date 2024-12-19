use std::fmt::Display;

use pyo3::class::basic::CompareOp;
use pyo3::{prelude::*, pyclass};

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq)]
// See https://github.com/PyO3/pyo3/issues/4723
#[allow(ambiguous_associated_items)]
#[pyclass(eq, eq_int)]
pub enum Outcome {
    Pass,
    Error,
    Failure,
    Skip,
}

#[pymethods]
impl Outcome {
    #[new]
    fn new(value: &str) -> Self {
        match value {
            "pass" => Outcome::Pass,
            "failure" => Outcome::Failure,
            "error" => Outcome::Error,
            "skip" => Outcome::Skip,
            _ => Outcome::Failure,
        }
    }

    fn __str__(&self) -> &str {
        match &self {
            Outcome::Pass => "pass",
            Outcome::Failure => "failure",
            Outcome::Error => "error",
            Outcome::Skip => "skip",
        }
    }
}

impl Serialize for Outcome {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl Display for Outcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            Outcome::Pass => write!(f, "pass"),
            Outcome::Failure => write!(f, "failure"),
            Outcome::Error => write!(f, "error"),
            Outcome::Skip => write!(f, "skip"),
        }
    }
}

static FRAMEWORKS: &[(&str, Framework)] = &[
    ("pytest", Framework::Pytest),
    ("vitest", Framework::Vitest),
    ("jest", Framework::Jest),
    ("phpunit", Framework::PHPUnit),
];

static EXTENSIONS: &[(&str, Framework)] =
    &[(".py", Framework::Pytest), (".php", Framework::PHPUnit)];

fn check_substring_before_word_boundary(string: &str, substring: &str) -> bool {
    if let Some((_, suffix)) = string.to_lowercase().split_once(substring) {
        return suffix
            .chars()
            .next()
            .map_or(true, |first_char| !first_char.is_alphanumeric());
    }
    false
}

pub fn check_testsuites_name(testsuites_name: &str) -> Option<Framework> {
    FRAMEWORKS
        .iter()
        .filter_map(|(name, framework)| {
            check_substring_before_word_boundary(testsuites_name, name).then_some(*framework)
        })
        .next()
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[pyclass]
pub struct Testrun {
    #[pyo3(get, set)]
    pub name: String,
    #[pyo3(get, set)]
    pub classname: String,
    #[pyo3(get, set)]
    pub duration: Option<f64>,
    #[pyo3(get, set)]
    pub outcome: Outcome,
    #[pyo3(get, set)]
    pub testsuite: String,
    #[pyo3(get, set)]
    pub failure_message: Option<String>,
    #[pyo3(get, set)]
    pub filename: Option<String>,
    #[pyo3(get, set)]
    pub build_url: Option<String>,
    #[pyo3(get, set)]
    pub computed_name: Option<String>,
}

impl Testrun {
    pub fn framework(&self) -> Option<Framework> {
        for (name, framework) in FRAMEWORKS {
            if check_substring_before_word_boundary(&self.testsuite, name) {
                return Some(framework.to_owned());
            }
        }

        for (extension, framework) in EXTENSIONS {
            if check_substring_before_word_boundary(&self.classname, extension)
                || check_substring_before_word_boundary(&self.name, extension)
            {
                return Some(framework.to_owned());
            }

            if let Some(message) = &self.failure_message {
                if check_substring_before_word_boundary(message, extension) {
                    return Some(framework.to_owned());
                }
            }

            if let Some(filename) = &self.filename {
                if check_substring_before_word_boundary(filename, extension) {
                    return Some(framework.to_owned());
                }
            }
        }
        None
    }
}

#[pymethods]
impl Testrun {
    #[allow(clippy::too_many_arguments)]
    #[new]
    #[pyo3(signature = (name, classname, duration, outcome, testsuite, failure_message=None, filename=None, build_url=None, computed_name=None))]
    fn new(
        name: String,
        classname: String,
        duration: Option<f64>,
        outcome: Outcome,
        testsuite: String,
        failure_message: Option<String>,
        filename: Option<String>,
        build_url: Option<String>,
        computed_name: Option<String>,
    ) -> Self {
        Self {
            name,
            classname,
            duration,
            outcome,
            testsuite,
            failure_message,
            filename,
            build_url,
            computed_name,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "({}, {}, {}, {:?}, {}, {:?}, {:?}, {:?})",
            self.name,
            self.classname,
            self.outcome,
            self.duration,
            self.testsuite,
            self.failure_message,
            self.filename,
            self.computed_name,
        )
    }

    fn __richcmp__(&self, other: &Self, op: CompareOp) -> PyResult<bool> {
        match op {
            CompareOp::Eq => Ok(self.name == other.name
                && self.classname == other.classname
                && self.outcome == other.outcome
                && self.duration == other.duration
                && self.testsuite == other.testsuite
                && self.failure_message == other.failure_message
                && self.filename == other.filename
                && self.computed_name == other.computed_name),
            _ => todo!(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[pyclass(eq, eq_int)]
pub enum Framework {
    Pytest,
    Vitest,
    Jest,
    PHPUnit,
}

#[pymethods]
impl Framework {
    fn __str__(&self) -> &str {
        match &self {
            Framework::Pytest => "Pytest",
            Framework::Vitest => "Vitest",
            Framework::Jest => "Jest",
            Framework::PHPUnit => "PHPUnit",
        }
    }
}

impl Display for Framework {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            Framework::Pytest => write!(f, "Pytest"),
            Framework::Vitest => write!(f, "Vitest"),
            Framework::Jest => write!(f, "Jest"),
            Framework::PHPUnit => write!(f, "PHPUnit"),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[pyclass]
pub struct ParsingInfo {
    #[pyo3(get, set)]
    pub framework: Option<Framework>,
    #[pyo3(get, set)]
    pub testruns: Vec<Testrun>,
}

#[pymethods]
impl ParsingInfo {
    #[new]
    #[pyo3(signature = (framework, testruns))]
    fn new(framework: Option<Framework>, testruns: Vec<Testrun>) -> Self {
        Self {
            framework,
            testruns,
        }
    }

    fn __repr__(&self) -> String {
        format!("({:?}, {:?})", self.framework, self.testruns)
    }

    fn __richcmp__(&self, other: &Self, op: CompareOp) -> PyResult<bool> {
        match op {
            CompareOp::Eq => {
                Ok(self.framework == other.framework && self.testruns == other.testruns)
            }
            _ => todo!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_framework_testsuites_name_no_match() {
        let f = check_testsuites_name("whatever");
        assert_eq!(f, None)
    }

    #[test]
    fn test_detect_framework_testsuites_name_match() {
        let f = check_testsuites_name("jest tests");
        assert_eq!(f, Some(Framework::Jest))
    }

    #[test]
    fn test_detect_framework_testsuite_name() {
        let t = Testrun {
            classname: "".to_string(),
            name: "".to_string(),
            duration: None,
            outcome: Outcome::Pass,
            testsuite: "pytest".to_string(),
            failure_message: None,
            filename: None,
            build_url: None,
            computed_name: None,
        };
        assert_eq!(t.framework(), Some(Framework::Pytest));
    }

    #[test]
    fn test_detect_framework_filenames() {
        let t = Testrun {
            classname: "".to_string(),
            name: "".to_string(),
            duration: None,
            outcome: Outcome::Pass,
            testsuite: "".to_string(),
            failure_message: None,
            filename: Some(".py".to_string()),
            build_url: None,
            computed_name: None,
        };
        assert_eq!(t.framework(), Some(Framework::Pytest));
    }

    #[test]
    fn test_detect_framework_example_classname() {
        let t = Testrun {
            classname: ".py".to_string(),
            name: "".to_string(),
            duration: None,
            outcome: Outcome::Pass,
            testsuite: "".to_string(),
            failure_message: None,
            filename: None,
            build_url: None,
            computed_name: None,
        };
        assert_eq!(t.framework(), Some(Framework::Pytest));
    }

    #[test]
    fn test_detect_framework_example_name() {
        let t = Testrun {
            classname: "".to_string(),
            name: ".py".to_string(),
            duration: None,
            outcome: Outcome::Pass,
            testsuite: "".to_string(),
            failure_message: None,
            filename: None,
            build_url: None,
            computed_name: None,
        };
        assert_eq!(t.framework(), Some(Framework::Pytest));
    }

    #[test]
    fn test_detect_framework_failure_messages() {
        let t = Testrun {
            classname: "".to_string(),
            name: "".to_string(),
            duration: None,
            outcome: Outcome::Pass,
            testsuite: "".to_string(),
            failure_message: Some(".py".to_string()),
            filename: None,
            build_url: None,
            computed_name: None,
        };
        assert_eq!(t.framework(), Some(Framework::Pytest));
    }

    #[test]
    fn test_detect_build_url() {
        let t = Testrun {
            classname: "".to_string(),
            name: "".to_string(),
            duration: None,
            outcome: Outcome::Pass,
            testsuite: "".to_string(),
            failure_message: Some(".py".to_string()),
            filename: None,
            build_url: Some("https://example.com/build_url".to_string()),
            computed_name: None,
        };
        assert_eq!(t.framework(), Some(Framework::Pytest));
    }
}

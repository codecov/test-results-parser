use std::fmt::Display;

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum Outcome {
    Pass,
    Error,
    Failure,
    Skip,
}

impl Display for Outcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            Outcome::Pass => write!(f, "Pass"),
            Outcome::Failure => write!(f, "Failure"),
            Outcome::Error => write!(f, "Error"),
            Outcome::Skip => write!(f, "Skip"),
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
pub struct Testrun {
    pub name: String,
    pub classname: String,
    pub duration: Option<f64>,
    pub outcome: Outcome,
    pub testsuite: String,
    pub failure_message: Option<String>,
    pub filename: Option<String>,
    pub build_url: Option<String>,
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

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum Framework {
    Pytest,
    Vitest,
    Jest,
    PHPUnit,
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
pub struct ParsingInfo {
    pub framework: Option<Framework>,
    pub testruns: Vec<Testrun>,
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct Failure {
    pub duration: Option<f64>,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Default)]

pub struct PRCommentSummary {
    pub passed_num: usize,
    pub failed_num: usize,
    pub skipped_num: usize,
    pub failures: Vec<Failure>,
    pub flaky_failures: Vec<Failure>,
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct FlakeDetectionSummary {
    pub passed: Vec<String>,
    pub failed: Vec<String>,
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

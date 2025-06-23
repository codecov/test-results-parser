use pyo3::prelude::*;
use pyo3::types::{PyDict, PyFloat, PyInt, PyList, PyString};
use pyo3::{PyAny, PyResult};
use serde::Serialize;
use serde_json::Value;

use crate::validated_string::ValidatedString;

static FRAMEWORKS: [(&str, Framework); 4] = [
    ("pytest", Framework::Pytest),
    ("vitest", Framework::Vitest),
    ("jest", Framework::Jest),
    ("phpunit", Framework::PHPUnit),
];

static EXTENSIONS: [(&str, Framework); 2] =
    [(".py", Framework::Pytest), (".php", Framework::PHPUnit)];

fn check_substring_before_word_boundary(string: &str, substring: &str) -> bool {
    if let Some((_, suffix)) = string.to_lowercase().split_once(substring) {
        return suffix
            .chars()
            .next()
            .is_none_or(|first_char| !first_char.is_alphanumeric());
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

#[derive(Clone, Copy, Debug, Serialize, PartialEq)]
pub enum Outcome {
    Pass,
    Failure,
    Skip,
    Error,
}

impl<'py> IntoPyObject<'py> for Outcome {
    type Target = PyString;
    type Output = Bound<'py, Self::Target>;
    type Error = std::convert::Infallible;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, std::convert::Infallible> {
        match self {
            Outcome::Pass => Ok("pass".into_pyobject(py)?),
            Outcome::Failure => Ok("failure".into_pyobject(py)?),
            Outcome::Skip => Ok("skip".into_pyobject(py)?),
            Outcome::Error => Ok("error".into_pyobject(py)?),
        }
    }
}

impl<'py> FromPyObject<'py> for Outcome {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        let s = ob.extract::<&str>()?;
        match s {
            "pass" => Ok(Outcome::Pass),
            "failure" => Ok(Outcome::Failure),
            "skip" => Ok(Outcome::Skip),
            "error" => Ok(Outcome::Error),
            _ => Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "Invalid outcome: {}",
                s
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq)]
pub enum Framework {
    Pytest,
    Vitest,
    Jest,
    PHPUnit,
}

impl<'py> IntoPyObject<'py> for Framework {
    type Target = PyString;
    type Output = Bound<'py, Self::Target>;
    type Error = std::convert::Infallible;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        match self {
            Framework::Pytest => Ok("Pytest".into_pyobject(py)?),
            Framework::Vitest => Ok("Vitest".into_pyobject(py)?),
            Framework::Jest => Ok("Jest".into_pyobject(py)?),
            Framework::PHPUnit => Ok("PHPUnit".into_pyobject(py)?),
        }
    }
}

impl<'py> FromPyObject<'py> for Framework {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        let s = ob.extract::<&str>()?;
        match s {
            "Pytest" => Ok(Framework::Pytest),
            "Vitest" => Ok(Framework::Vitest),
            "Jest" => Ok(Framework::Jest),
            "PHPUnit" => Ok(Framework::PHPUnit),
            _ => Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "Invalid outcome: {}",
                s
            ))),
        }
    }
}

/// Wrapper for serde_json::Value to enable PyO3 conversion
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct PropertiesValue(pub Option<Value>);

impl<'py> IntoPyObject<'py> for PropertiesValue {
    type Target = PyAny;
    type Output = Bound<'py, Self::Target>;
    type Error = pyo3::PyErr;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        match self.0 {
            Some(value) => {
                let converted_result = value_to_pyobject(&value, py)?;
                Ok(converted_result.into_bound(py))
            }
            None => Ok(py.None().into_bound(py)),
        }
    }
}

impl<'py> FromPyObject<'py> for PropertiesValue {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        if ob.is_none() {
            return Ok(PropertiesValue(None));
        }

        let value = pyobject_to_value(ob)?;
        Ok(PropertiesValue(Some(value)))
    }
}

fn pyobject_to_value(ob: &Bound<'_, PyAny>) -> PyResult<Value> {
    if ob.is_none() {
        return Ok(Value::Null);
    }

    if let Ok(s) = ob.extract::<&str>() {
        return Ok(Value::String(s.to_string()));
    }

    if let Ok(i) = ob.extract::<i64>() {
        return Ok(Value::Number(i.into()));
    }

    if let Ok(f) = ob.extract::<f64>() {
        return Ok(Value::Number(serde_json::Number::from_f64(f).unwrap()));
    }

    if let Ok(b) = ob.extract::<bool>() {
        return Ok(Value::Bool(b));
    }

    if let Ok(list) = ob.downcast::<PyList>() {
        let values = list
            .iter()
            .map(|item| pyobject_to_value(&item))
            .collect::<PyResult<Vec<Value>>>()?;
        return Ok(Value::Array(values));
    }

    if let Ok(dict) = ob.downcast::<PyDict>() {
        let mut map = serde_json::Map::new();
        for (key, value) in dict.iter() {
            let key_str = key.extract::<&str>()?;
            let value_json = pyobject_to_value(&value)?;
            map.insert(key_str.to_string(), value_json);
        }
        return Ok(Value::Object(map));
    }

    // If we can't convert it, try to convert to string as fallback
    let s = ob.str()?.to_string();
    Ok(Value::String(s))
}

fn value_to_pyobject(value: &Value, py: Python) -> PyResult<PyObject> {
    match value {
        Value::String(s) => Ok(PyString::new(py, s).into()),
        Value::Number(n) => Ok(PyFloat::new(py, n.as_f64().unwrap()).into()),
        // Using PyBool returns a Borrowed reference to 'True' or 'False' (singletons)
        // This messed up the `.into()` conversion. So I'm using PyInt instead.
        Value::Bool(b) => Ok(PyInt::new(py, *b as i64).into()),
        Value::Null => Ok(py.None()),
        Value::Array(a) => {
            let values_as_pyobjects = a
                .iter()
                .map(|v| value_to_pyobject(v, py))
                .collect::<Result<Vec<PyObject>, PyErr>>()?;
            let list = PyList::new(py, values_as_pyobjects)?;
            Ok(list.into())
        }
        Value::Object(o) => {
            let dict = PyDict::new(py);
            for (key, value) in o.iter() {
                dict.set_item(key, value_to_pyobject(value, py)?)?;
            }
            Ok(dict.into())
        }
    }
}
// i can't seem to get  pyo3(from_item_all) to work when IntoPyObject is also being derived
#[derive(IntoPyObject, FromPyObject, Clone, Debug, Serialize, PartialEq)]
pub struct Testrun {
    #[pyo3(item)]
    pub name: ValidatedString,
    #[pyo3(item)]
    pub classname: ValidatedString,
    #[pyo3(item)]
    pub duration: Option<f64>,
    #[pyo3(item)]
    pub outcome: Outcome,
    #[pyo3(item)]
    pub testsuite: ValidatedString,
    #[pyo3(item)]
    pub failure_message: Option<String>,
    #[pyo3(item)]
    pub filename: Option<ValidatedString>,
    #[pyo3(item)]
    pub build_url: Option<String>,
    #[pyo3(item)]
    pub computed_name: ValidatedString,
    #[pyo3(item)]
    pub properties: PropertiesValue,
}

impl Testrun {
    pub fn framework(&self) -> Option<Framework> {
        for (name, framework) in FRAMEWORKS {
            if check_substring_before_word_boundary(&self.testsuite, name) {
                return Some(framework);
            }
        }

        for (extension, framework) in EXTENSIONS {
            if check_substring_before_word_boundary(&self.classname, extension)
                || check_substring_before_word_boundary(&self.name, extension)
            {
                return Some(framework);
            }

            if let Some(message) = &self.failure_message {
                if check_substring_before_word_boundary(message, extension) {
                    return Some(framework);
                }
            }

            if let Some(filename) = &self.filename {
                if check_substring_before_word_boundary(filename, extension) {
                    return Some(framework);
                }
            }
        }
        None
    }
}

#[derive(Clone, Debug, Serialize, IntoPyObject)]
pub struct ParsingInfo {
    pub framework: Option<Framework>,
    pub testruns: Vec<Testrun>,
    pub warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn setup() {
        pyo3::prepare_freethreaded_python();
    }

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
            classname: ValidatedString::default(),
            name: ValidatedString::default(),
            duration: None,
            outcome: Outcome::Pass,
            testsuite: "pytest".try_into().unwrap(),
            failure_message: None,
            filename: None,
            build_url: None,
            computed_name: ValidatedString::default(),
            properties: PropertiesValue(None),
        };
        assert_eq!(t.framework(), Some(Framework::Pytest))
    }

    #[test]
    fn test_detect_framework_filenames() {
        let t = Testrun {
            classname: ValidatedString::default(),
            name: ValidatedString::default(),
            duration: None,
            outcome: Outcome::Pass,
            testsuite: ValidatedString::default(),
            failure_message: None,
            filename: Some(".py".try_into().unwrap()),
            build_url: None,
            computed_name: ValidatedString::default(),
            properties: PropertiesValue(None),
        };
        assert_eq!(t.framework(), Some(Framework::Pytest))
    }

    #[test]
    fn test_detect_framework_example_classname() {
        let t = Testrun {
            classname: ".py".try_into().unwrap(),
            name: ValidatedString::default(),
            duration: None,
            outcome: Outcome::Pass,
            testsuite: ValidatedString::default(),
            failure_message: None,
            filename: None,
            build_url: None,
            computed_name: ValidatedString::default(),
            properties: PropertiesValue(None),
        };
        assert_eq!(t.framework(), Some(Framework::Pytest))
    }

    #[test]
    fn test_detect_framework_example_name() {
        let t = Testrun {
            classname: ValidatedString::default(),
            name: ".py".try_into().unwrap(),
            duration: None,
            outcome: Outcome::Pass,
            testsuite: ValidatedString::default(),
            failure_message: None,
            filename: None,
            build_url: None,
            computed_name: ValidatedString::default(),
            properties: PropertiesValue(None),
        };
        assert_eq!(t.framework(), Some(Framework::Pytest))
    }

    #[test]
    fn test_detect_framework_failure_messages() {
        let t = Testrun {
            classname: ValidatedString::default(),
            name: ValidatedString::default(),
            duration: None,
            outcome: Outcome::Pass,
            testsuite: ValidatedString::default(),
            failure_message: Some(".py".to_string()),
            filename: None,
            build_url: None,
            computed_name: ValidatedString::default(),
            properties: PropertiesValue(None),
        };
        assert_eq!(t.framework(), Some(Framework::Pytest))
    }

    #[test]
    fn test_detect_build_url() {
        let t = Testrun {
            classname: ValidatedString::default(),
            name: ValidatedString::default(),
            duration: None,
            outcome: Outcome::Pass,
            testsuite: ValidatedString::default(),
            failure_message: Some(".py".to_string()),
            filename: None,
            build_url: Some("https://example.com/build_url".to_string()),
            computed_name: ValidatedString::default(),
            properties: PropertiesValue(None),
        };
        assert_eq!(t.framework(), Some(Framework::Pytest))
    }

    #[test]
    fn test_properties_into_none_conversion() {
        setup();
        let property = PropertiesValue(None);
        Python::with_gil(|py| {
            let property_py = property
                .into_pyobject(py)
                .expect("Failed to convert PropertiesValue to Python object");
            assert!(property_py.is_none());
        })
    }

    #[test]
    fn test_properties_into_string_conversion() {
        setup();
        let property = PropertiesValue(Some(json!("test_string")));
        Python::with_gil(|py| {
            let property_py = property
                .into_pyobject(py)
                .expect("Failed to convert PropertiesValue to Python object");
            let round_trip_value = PropertiesValue::extract_bound(&property_py)
                .expect("Failed to extract PropertiesValue from Python object");
            assert_eq!(
                round_trip_value,
                PropertiesValue(Some(json!("test_string")))
            );
        })
    }

    #[test]
    fn test_properties_into_integer_conversion() {
        setup();
        let property = PropertiesValue(Some(json!(42)));
        Python::with_gil(|py| {
            let property_py = property
                .into_pyobject(py)
                .expect("Failed to convert PropertiesValue to Python object");
            let round_trip_value = PropertiesValue::extract_bound(&property_py)
                .expect("Failed to extract PropertiesValue from Python object");
            // The conversion of ANY number into PyObj casts that number into float
            assert_eq!(round_trip_value, PropertiesValue(Some(json!(42.0))));
        })
    }

    #[test]
    fn test_properties_into_boolean_conversion() {
        setup();
        let property = PropertiesValue(Some(json!(true)));
        Python::with_gil(|py| {
            let property_py = property
                .into_pyobject(py)
                .expect("Failed to convert PropertiesValue to Python object");
            let round_trip_value = PropertiesValue::extract_bound(&property_py)
                .expect("Failed to extract PropertiesValue from Python object");
            // Conversion from booleans are cast into integers
            assert_eq!(round_trip_value, PropertiesValue(Some(json!(1))));
        })
    }

    #[test]
    fn test_properties_into_empty_list_conversion() {
        setup();
        let property = PropertiesValue(Some(json!([])));
        Python::with_gil(|py| {
            let property_py = property
                .into_pyobject(py)
                .expect("Failed to convert PropertiesValue to Python object");
            let round_trip_value = PropertiesValue::extract_bound(&property_py)
                .expect("Failed to extract PropertiesValue from Python object");
            assert_eq!(round_trip_value, PropertiesValue(Some(json!([]))));
        })
    }

    #[test]
    fn test_properties_into_list_with_values_conversion() {
        setup();
        let property = PropertiesValue(Some(json!(["item1", 123, 4.20, true])));
        Python::with_gil(|py| {
            let property_py = property
                .into_pyobject(py)
                .expect("Failed to convert PropertiesValue to Python object");
            let round_trip_value = PropertiesValue::extract_bound(&property_py)
                .expect("Failed to extract PropertiesValue from Python object");
            // Note: booleans get converted to integers in the round trip
            assert_eq!(
                round_trip_value,
                PropertiesValue(Some(json!(["item1", 123.0, 4.20, 1])))
            );
        })
    }

    #[test]
    fn test_properties_into_empty_dict_conversion() {
        setup();
        let property = PropertiesValue(Some(json!({})));
        Python::with_gil(|py| {
            let property_py = property
                .into_pyobject(py)
                .expect("Failed to convert PropertiesValue to Python object");
            let round_trip_value = PropertiesValue::extract_bound(&property_py)
                .expect("Failed to extract PropertiesValue from Python object");
            assert_eq!(round_trip_value, PropertiesValue(Some(json!({}))));
        })
    }

    #[test]
    fn test_properties_into_dict_with_values_conversion() {
        setup();
        let property = PropertiesValue(Some(json!({
            "string_key": "string_value",
            "int_key": 456,
            "bool_key": false
        })));
        Python::with_gil(|py| {
            let property_py = property
                .into_pyobject(py)
                .expect("Failed to convert PropertiesValue to Python object");
            let round_trip_value = PropertiesValue::extract_bound(&property_py)
                .expect("Failed to extract PropertiesValue from Python object");
            // Note: booleans get converted to integers in the round trip
            assert_eq!(
                round_trip_value,
                PropertiesValue(Some(json!({
                    "string_key": "string_value",
                    "int_key": 456.0,
                    "bool_key": 0
                })))
            );
        })
    }

    #[test]
    fn test_properties_into_nested_dict_conversion() {
        setup();
        let property = PropertiesValue(Some(json!({
            "outer_key": "outer_value",
            "nested": {
                "inner_key": "inner_value"
            }
        })));
        Python::with_gil(|py| {
            let property_py = property
                .into_pyobject(py)
                .expect("Failed to convert PropertiesValue to Python object");
            let round_trip_value = PropertiesValue::extract_bound(&property_py)
                .expect("Failed to extract PropertiesValue from Python object");
            assert_eq!(
                round_trip_value,
                PropertiesValue(Some(json!({
                    "outer_key": "outer_value",
                    "nested": {
                        "inner_key": "inner_value"
                    }
                })))
            );
        })
    }

    #[test]
    fn test_properties_into_list_with_dict_conversion() {
        setup();
        let property = PropertiesValue(Some(json!([
            "list_item",
            {
                "key": "value"
            }
        ])));
        Python::with_gil(|py| {
            let property_py = property
                .into_pyobject(py)
                .expect("Failed to convert PropertiesValue to Python object");
            let round_trip_value = PropertiesValue::extract_bound(&property_py)
                .expect("Failed to extract PropertiesValue from Python object");
            assert_eq!(
                round_trip_value,
                PropertiesValue(Some(json!([
                    "list_item",
                    {
                        "key": "value"
                    }
                ])))
            );
        })
    }

    #[test]
    fn test_properties_into_dict_with_list_conversion() {
        setup();
        let property = PropertiesValue(Some(json!({
            "numbers": [1, 2, 3],
            "name": "test"
        })));
        Python::with_gil(|py| {
            let property_py = property
                .into_pyobject(py)
                .expect("Failed to convert PropertiesValue to Python object");
            let round_trip_value = PropertiesValue::extract_bound(&property_py)
                .expect("Failed to extract PropertiesValue from Python object");
            assert_eq!(
                round_trip_value,
                PropertiesValue(Some(json!({
                    "numbers": [1.0, 2.0, 3.0],
                    "name": "test"
                })))
            );
        })
    }
}

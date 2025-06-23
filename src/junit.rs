use anyhow::{Context, Result};
use pyo3::prelude::*;
use serde_json::Value;
use std::collections::HashSet;
use std::fmt;

use quick_xml::events::attributes::{Attribute, Attributes};
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

use crate::compute_name::{compute_name, unescape_str};
use crate::testrun::{check_testsuites_name, Framework, Outcome, PropertiesValue, Testrun};
use crate::validated_string::ValidatedString;
use crate::warning::WarningInfo;
use thiserror::Error;

#[derive(Error, Debug)]
enum ParseAttrsError {
    #[error("Limit of string is 1000 chars, for {0}, we got {1}")]
    AttrTooLong(&'static str, usize),
    #[error("Error converting attribute {0} to UTF-8 string")]
    ConversionError(&'static str),
    #[error("Missing name attribute in testcase")]
    NameMissing,
    #[error("Error parsing attribute")]
    ParseError,
}

fn convert_attribute(attribute: Attribute) -> Result<String> {
    let bytes = attribute.value.into_owned();
    Ok(String::from_utf8(bytes)?)
}

fn extract_validated_string(
    attribute: Attribute,
    field_name: &'static str,
) -> Result<ValidatedString, ParseAttrsError> {
    let unvalidated_string =
        convert_attribute(attribute).map_err(|_| ParseAttrsError::ConversionError(field_name))?;
    let string_len = unvalidated_string.len();
    ValidatedString::from_string(unvalidated_string)
        .map_err(|_| ParseAttrsError::AttrTooLong(field_name, string_len))
}

struct TestcaseAttrs {
    name: ValidatedString,
    time: Option<String>,
    classname: Option<ValidatedString>,
    file: Option<ValidatedString>,
}

// originally from https://gist.github.com/scott-codecov/311c174ecc7de87f7d7c50371c6ef927#file-cobertura-rs-L18-L31
fn parse_testcase_attrs(attributes: Attributes) -> Result<TestcaseAttrs, ParseAttrsError> {
    let mut name: Option<ValidatedString> = None;
    let mut time: Option<String> = None;
    let mut classname: Option<ValidatedString> = None;
    let mut file: Option<ValidatedString> = None;

    for attribute in attributes {
        let attribute = attribute.map_err(|_| ParseAttrsError::ParseError)?;

        match attribute.key.into_inner() {
            b"time" => {
                time = Some(
                    convert_attribute(attribute)
                        .map_err(|_| ParseAttrsError::ConversionError("time"))?,
                );
            }
            b"classname" => {
                classname = Some(extract_validated_string(attribute, "classname")?);
            }
            b"name" => {
                name = Some(extract_validated_string(attribute, "name")?);
            }
            b"file" => {
                file = Some(extract_validated_string(attribute, "file")?);
            }
            _ => {}
        }
    }

    match name {
        Some(name) => Ok(TestcaseAttrs {
            name,
            time,
            classname,
            file,
        }),
        None => Err(ParseAttrsError::NameMissing),
    }
}

fn get_attribute(e: &BytesStart, name: &str) -> Result<Option<String>> {
    let attr = if let Some(message) = e
        .try_get_attribute(name)
        .context("Error parsing attribute")?
    {
        Some(String::from_utf8(message.value.to_vec())?)
    } else {
        None
    };
    Ok(attr)
}

fn populate(
    rel_attrs: TestcaseAttrs,
    testsuite: ValidatedString,
    testsuite_time: Option<&str>,
    framework: Option<Framework>,
    network: Option<&HashSet<String>>,
) -> Result<(Testrun, Option<Framework>)> {
    let name = rel_attrs.name;
    let classname = rel_attrs.classname.unwrap_or_default();
    let duration = rel_attrs
        .time
        .as_deref()
        .or(testsuite_time)
        .and_then(|t| t.parse().ok());
    let file = rel_attrs.file;

    let mut t = Testrun {
        name,
        classname,
        duration,
        outcome: Outcome::Pass,
        testsuite,
        failure_message: None,
        filename: file,
        build_url: None,
        computed_name: ValidatedString::default(),
        properties: PropertiesValue(None),
    };

    let framework = framework.or_else(|| t.framework());
    let computed_name = compute_name(
        &t.classname,
        &t.name,
        framework,
        t.filename.as_deref(),
        network,
    );
    t.computed_name = ValidatedString::from_string(computed_name)
        .context("Error converting computed name to ValidatedString")?;

    Ok((t, framework))
}

pub fn get_position_info(input: &[u8], byte_offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut last_newline = 0;

    for (i, &byte) in input.iter().take(byte_offset).enumerate() {
        if byte == b'\n' {
            line += 1;
            last_newline = i + 1;
        }
    }

    let column = byte_offset - last_newline + 1;

    (line, column)
}

#[derive(Error, Debug)]
struct NotEvalsPropertyError;

impl fmt::Display for NotEvalsPropertyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not evals property")
    }
}

/// Parses the `property` element found in the `testcase` element.
///
/// This function is used to parse the `evals` attribute of the `testcase` element.
/// It will update the `properties` field of the `testrun` object with the new value.
///
/// The `name` attribute in `property` encodes the hierarchy of the `value` attribute
/// inside `Testrun.properties` (which is a JSON object).
/// For example
/// &lt;property name="evals.scores.isUseful.type" value="boolean" /&gt;
/// &lt;property name="evals.scores.isUseful.value" value="true" /&gt;
/// &lt;property name="evals.scores.isUseful.sum" value="1" /&gt;
/// &lt;property name="evals.scores.isUseful.llm_judge" value="gemini_2.5pro" /&gt;
///
/// will be parsed as:
/// {
///     "scores": {
///         "isUseful": {
///             "type": "boolean",
///             "value": "true",
///             "sum": "1",
///             "llm_judge": "gemini_2.5pro"
///         }
///     }
/// }
fn parse_property_element(e: &BytesStart, existing_properties: &mut PropertiesValue) -> Result<()> {
    // Early return if not an evals property
    let name = get_attribute(e, "name")?
        .filter(|n| n.starts_with("evals"))
        .ok_or(NotEvalsPropertyError)?;

    let value = get_attribute(e, "value")?
        .ok_or_else(|| anyhow::anyhow!("Property must have value attribute"))?;

    let name_parts: Vec<&str> = name.split(".").collect();
    if name_parts.len() < 2 {
        anyhow::bail!("Property name must have at least 2 parts");
    }

    // Initialize properties if needed
    if existing_properties.0.is_none() {
        *existing_properties = PropertiesValue(Some(serde_json::json!({})));
    }

    let mut current = existing_properties.0.as_mut().unwrap();

    // Navigate through intermediate parts (skip first "evals" and last key)
    for part in &name_parts[1..name_parts.len() - 1] {
        current = match current {
            Value::Object(map) => {
                map.entry(part.to_string()).or_insert_with(|| {
                    if *part == "evaluations" {
                        serde_json::json!([])
                    } else {
                        serde_json::json!({})
                    }
                });
                map.get_mut(*part).unwrap()
            }
            Value::Array(array) => {
                let idx = part
                    .parse::<usize>()
                    .map_err(|_| anyhow::anyhow!("Invalid array index: {}", part))?;
                if idx >= array.len() {
                    array.resize(idx + 1, serde_json::json!({}));
                }
                array.get_mut(idx).unwrap()
            }
            _ => anyhow::bail!(
                "Cannot drill down into non-object/non-array value at part: {}",
                part
            ),
        };
    }

    // Set the final value
    match current {
        Value::Object(map) => {
            map.insert(name_parts.last().unwrap().to_string(), Value::String(value));
        }
        _ => anyhow::bail!("Cannot set value in non-object at final key"),
    }

    Ok(())
}

enum TestrunOrSkipped {
    Testrun(Testrun),
    Skipped,
}

fn handle_property_element(
    e: &BytesStart,
    saved_testrun: &mut Option<TestrunOrSkipped>,
    buffer_position: u64,
    warnings: &mut Vec<WarningInfo>,
) -> Result<()> {
    // Check if there is a testrun currently being processed
    if saved_testrun.is_none() {
        return Ok(());
    }
    let saved = saved_testrun
        .as_mut()
        .context("Error accessing saved testrun")?;
    if let TestrunOrSkipped::Testrun(testrun) = saved {
        if let Err(e) = parse_property_element(e, &mut testrun.properties) {
            if !e.is::<NotEvalsPropertyError>() {
                warnings.push(WarningInfo::new(
                    format!("Error parsing `property` element: {}", e),
                    buffer_position,
                ));
            }
        }
    };
    Ok(())
}

pub fn use_reader(
    reader: &mut Reader<&[u8]>,
    network: Option<&HashSet<String>>,
) -> PyResult<(Option<Framework>, Vec<Testrun>, Vec<WarningInfo>)> {
    let mut testruns: Vec<Testrun> = Vec::new();
    let mut saved_testrun: Option<TestrunOrSkipped> = None;

    let mut in_failure: bool = false;
    let mut in_error: bool = false;

    let mut framework: Option<Framework> = None;

    let mut warnings: Vec<WarningInfo> = Vec::new();

    // every time we come across a testsuite element we update this vector:
    // if the testsuite element contains the time attribute append its value to this vec
    // else append a clone of the last value in the vec
    let mut testsuite_names: Vec<Option<ValidatedString>> = vec![];
    let mut testsuite_times: Vec<Option<String>> = vec![];

    let mut buf = Vec::new();
    loop {
        let event = reader
            .read_event_into(&mut buf)
            .context("Error parsing XML")?;
        match event {
            Event::Eof => {
                break;
            }
            Event::Start(e) => match e.name().as_ref() {
                b"testcase" => {
                    let attrs = parse_testcase_attrs(e.attributes());
                    match attrs {
                        Ok(attrs) => {
                            let (testrun, parsed_framework) = populate(
                                attrs,
                                testsuite_names
                                    .iter()
                                    .rev()
                                    .find_map(|e| e.clone())
                                    .unwrap_or_default(),
                                testsuite_times.iter().rev().find_map(|e| e.as_deref()),
                                framework,
                                network,
                            )?;
                            saved_testrun = Some(TestrunOrSkipped::Testrun(testrun));
                            framework = parsed_framework;
                        }
                        Err(error) => match error {
                            ParseAttrsError::AttrTooLong(_, _) => {
                                warnings.push(WarningInfo::new(
                                    format!("Warning while parsing testcase attributes: {}", error),
                                    reader.buffer_position() - e.len() as u64,
                                ));
                                saved_testrun = Some(TestrunOrSkipped::Skipped);
                            }
                            _ => {
                                Err(anyhow::anyhow!(
                                    "Error parsing testcase attributes: {}",
                                    error
                                ))?;
                            }
                        },
                    }
                }
                b"skipped" => {
                    let saved = saved_testrun
                        .as_mut()
                        .context("Error accessing saved testrun")?;
                    match saved {
                        TestrunOrSkipped::Testrun(testrun) => {
                            testrun.outcome = Outcome::Skip;
                        }
                        TestrunOrSkipped::Skipped => {}
                    }
                }
                b"error" => {
                    let saved = saved_testrun
                        .as_mut()
                        .context("Error accessing saved testrun")?;
                    match saved {
                        TestrunOrSkipped::Testrun(testrun) => {
                            testrun.outcome = Outcome::Error;

                            testrun.failure_message = get_attribute(&e, "message")?
                                .map(|failure_message| unescape_str(&failure_message).into());
                        }
                        TestrunOrSkipped::Skipped => {}
                    }

                    in_error = true;
                }
                b"failure" => {
                    let saved = saved_testrun
                        .as_mut()
                        .context("Error accessing saved testrun")?;
                    match saved {
                        TestrunOrSkipped::Testrun(testrun) => {
                            testrun.outcome = Outcome::Failure;

                            testrun.failure_message = get_attribute(&e, "message")?
                                .map(|failure_message| unescape_str(&failure_message).into());
                        }
                        TestrunOrSkipped::Skipped => {}
                    }

                    in_failure = true;
                }
                b"testsuite" => {
                    testsuite_names.push(
                        get_attribute(&e, "name")?
                            .map(|s| {
                                ValidatedString::from_string(s)
                                    .context("Error converting testsuite name to ValidatedString")
                            })
                            .transpose()?,
                    );
                    testsuite_times.push(get_attribute(&e, "time")?);
                }
                b"testsuites" => {
                    let testsuites_name = get_attribute(&e, "name")?;
                    framework = testsuites_name.and_then(|name| check_testsuites_name(&name))
                }
                b"property" => handle_property_element(
                    &e,
                    &mut saved_testrun,
                    reader.buffer_position(),
                    &mut warnings,
                )?,
                _ => {}
            },
            Event::End(e) => match e.name().as_ref() {
                b"testcase" => {
                    let saved = saved_testrun.take().context(
                        "Met testcase closing tag without first meeting testcase opening tag",
                    )?;
                    match saved {
                        TestrunOrSkipped::Testrun(testrun) => testruns.push(testrun),
                        TestrunOrSkipped::Skipped => {}
                    }
                }
                b"failure" => in_failure = false,
                b"error" => in_error = false,
                b"testsuite" => {
                    testsuite_times.pop();
                    testsuite_names.pop();
                }
                _ => (),
            },
            Event::Empty(e) => match e.name().as_ref() {
                b"testcase" => {
                    let attrs = parse_testcase_attrs(e.attributes());
                    match attrs {
                        Ok(attrs) => {
                            let (testrun, parsed_framework) = populate(
                                attrs,
                                testsuite_names
                                    .iter()
                                    .rev()
                                    .find_map(|e| e.clone())
                                    .unwrap_or_default(),
                                testsuite_times.iter().rev().find_map(|e| e.as_deref()),
                                framework,
                                network,
                            )?;
                            testruns.push(testrun);
                            framework = parsed_framework;
                        }
                        Err(error) => match error {
                            ParseAttrsError::AttrTooLong(_, _) => {
                                warnings.push(WarningInfo::new(
                                    format!("Warning while parsing testcase attributes: {}", error),
                                    reader.buffer_position() - e.len() as u64,
                                ));
                            }
                            _ => Err(anyhow::anyhow!(
                                "Error parsing testcase attributes: {}",
                                error
                            ))?,
                        },
                    }
                }
                b"failure" => {
                    let saved = saved_testrun
                        .as_mut()
                        .context("Error accessing saved testrun")?;
                    match saved {
                        TestrunOrSkipped::Testrun(testrun) => {
                            testrun.outcome = Outcome::Failure;

                            testrun.failure_message = get_attribute(&e, "message")?
                                .map(|failure_message| unescape_str(&failure_message).into());
                        }
                        TestrunOrSkipped::Skipped => {}
                    }
                }
                b"skipped" => {
                    let saved = saved_testrun
                        .as_mut()
                        .context("Error accessing saved testrun")?;
                    match saved {
                        TestrunOrSkipped::Testrun(testrun) => {
                            testrun.outcome = Outcome::Skip;
                        }
                        TestrunOrSkipped::Skipped => {}
                    }
                }
                b"error" => {
                    let saved = saved_testrun
                        .as_mut()
                        .context("Error accessing saved testrun")?;
                    match saved {
                        TestrunOrSkipped::Testrun(testrun) => {
                            testrun.outcome = Outcome::Error;

                            testrun.failure_message = get_attribute(&e, "message")?
                                .map(|failure_message| unescape_str(&failure_message).into());
                        }
                        TestrunOrSkipped::Skipped => {}
                    }
                }
                b"property" => handle_property_element(
                    &e,
                    &mut saved_testrun,
                    reader.buffer_position(),
                    &mut warnings,
                )?,
                _ => {}
            },
            Event::Text(mut xml_failure_message) => {
                if in_failure || in_error {
                    let saved = saved_testrun
                        .as_mut()
                        .context("Error accessing saved testrun")?;
                    match saved {
                        TestrunOrSkipped::Testrun(testrun) => {
                            xml_failure_message.inplace_trim_end();
                            xml_failure_message.inplace_trim_start();

                            testrun.failure_message = Some(
                                unescape_str(std::str::from_utf8(&xml_failure_message)?).into(),
                            );
                        }
                        TestrunOrSkipped::Skipped => {}
                    }
                }
            }

            // There are several other `Event`s we do not consider here
            _ => (),
        }
        buf.clear()
    }

    Ok((framework, testruns, warnings))
}

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
use crate::warning::WarningInfo;
use thiserror::Error;

#[derive(Error, Debug)]
enum ParseAttrsError {
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

struct TestcaseAttrs {
    name: String,
    time: Option<String>,
    classname: Option<String>,
    file: Option<String>,
}

// originally from https://gist.github.com/scott-codecov/311c174ecc7de87f7d7c50371c6ef927#file-cobertura-rs-L18-L31
fn parse_testcase_attrs(attributes: Attributes) -> Result<TestcaseAttrs, ParseAttrsError> {
    let mut name: Option<String> = None;
    let mut time: Option<String> = None;
    let mut classname: Option<String> = None;
    let mut file: Option<String> = None;

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
                classname = Some(
                    convert_attribute(attribute)
                        .map_err(|_| ParseAttrsError::ConversionError("classname"))?,
                );
            }
            b"name" => {
                name = Some(
                    convert_attribute(attribute)
                        .map_err(|_| ParseAttrsError::ConversionError("name"))?,
                );
            }
            b"file" => {
                file = Some(
                    convert_attribute(attribute)
                        .map_err(|_| ParseAttrsError::ConversionError("file"))?,
                );
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
    testsuite: String,
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
        computed_name: String::new(),
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
    t.computed_name = computed_name;

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

fn handle_property_element(
    e: &BytesStart,
    saved_testrun: &mut Option<Testrun>,
    buffer_position: u64,
    warnings: &mut Vec<WarningInfo>,
) -> Result<()> {
    // Check if there is a testrun currently being processed
    if saved_testrun.is_none() {
        return Ok(());
    }
    let testrun = saved_testrun
        .as_mut()
        .context("Error accessing saved testrun")?;
    if let Err(e) = parse_property_element(e, &mut testrun.properties) {
        if !e.is::<NotEvalsPropertyError>() {
            warnings.push(WarningInfo::new(
                format!("Error parsing `property` element: {}", e),
                buffer_position,
            ));
        }
    }
    Ok(())
}

pub fn use_reader(
    reader: &mut Reader<&[u8]>,
    network: Option<&HashSet<String>>,
) -> PyResult<(Option<Framework>, Vec<Testrun>, Vec<WarningInfo>)> {
    let mut testruns: Vec<Testrun> = Vec::new();
    let mut saved_testrun: Option<Testrun> = None;

    let mut in_failure: bool = false;
    let mut in_error: bool = false;

    let mut framework: Option<Framework> = None;

    let mut warnings: Vec<WarningInfo> = Vec::new();

    // every time we come across a testsuite element we update this vector:
    // if the testsuite element contains the time attribute append its value to this vec
    // else append a clone of the last value in the vec
    let mut testsuite_names: Vec<Option<String>> = vec![];
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
                            saved_testrun = Some(testrun);
                            framework = parsed_framework;
                        }
                        Err(error) => {
                            Err(anyhow::anyhow!(
                                "Error parsing testcase attributes: {}",
                                error
                            ))?;
                        }
                    }
                }
                b"skipped" => {
                    let testrun = saved_testrun
                        .as_mut()
                        .context("Encountered <skipped> tag outside of <testcase>")?;
                    testrun.outcome = Outcome::Skip;
                }
                b"error" => {
                    let testrun = saved_testrun
                        .as_mut()
                        .context("Encountered <error> tag outside of <testcase>")?;
                    testrun.outcome = Outcome::Error;

                    testrun.failure_message = get_attribute(&e, "message")?
                        .map(|failure_message| unescape_str(&failure_message).into());

                    in_error = true;
                }
                b"failure" => {
                    let testrun = saved_testrun
                        .as_mut()
                        .context("Encountered <failure> tag outside of <testcase>")?;
                    testrun.outcome = Outcome::Failure;

                    testrun.failure_message = get_attribute(&e, "message")?
                        .map(|failure_message| unescape_str(&failure_message).into());

                    in_failure = true;
                }
                b"testsuite" => {
                    testsuite_names.push(get_attribute(&e, "name")?);
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
                    if let Some(testrun) = saved_testrun.take() {
                        testruns.push(testrun);
                    } else {
                        Err(anyhow::anyhow!(
                            "Encountered closing </testcase> tag without corresponding opening <testcase> tag"
                        ))?;
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
                        Err(error) => {
                            Err(anyhow::anyhow!(
                                "Error parsing testcase attributes: {}",
                                error
                            ))?;
                        }
                    }
                }
                b"failure" => {
                    let testrun = saved_testrun
                        .as_mut()
                        .context("Encountered <failure/> tag outside of <testcase>")?;
                    testrun.outcome = Outcome::Failure;

                    testrun.failure_message = get_attribute(&e, "message")?
                        .map(|failure_message| unescape_str(&failure_message).into());
                }
                b"skipped" => {
                    let testrun = saved_testrun
                        .as_mut()
                        .context("Encountered <skipped/> tag outside of <testcase>")?;
                    testrun.outcome = Outcome::Skip;
                }
                b"error" => {
                    let testrun = saved_testrun
                        .as_mut()
                        .context("Encountered <error/> tag outside of <testcase>")?;
                    testrun.outcome = Outcome::Error;

                    testrun.failure_message = get_attribute(&e, "message")?
                        .map(|failure_message| unescape_str(&failure_message).into());
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
                    if let Some(testrun) = saved_testrun.as_mut() {
                        xml_failure_message.inplace_trim_end();
                        xml_failure_message.inplace_trim_start();

                        testrun.failure_message =
                            Some(unescape_str(std::str::from_utf8(&xml_failure_message)?).into());
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

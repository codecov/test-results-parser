use anyhow::{Context, Result};
use pyo3::prelude::*;
use std::collections::HashSet;

use quick_xml::events::attributes::{Attribute, Attributes};
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

use crate::compute_name::{compute_name, unescape_str};
use crate::testrun::{check_testsuites_name, Framework, Outcome, Testrun};
use crate::validated_string::{ValidatedString, ValidatedStringError};
use crate::warning::WarningInfo;
use thiserror::Error;

#[derive(Error, Debug)]
enum ParseAttrsError {
    #[error("Length limit of {0} is 1000 chars, we got {1} chars:\n\n{2}")]
    AttrTooLong(&'static str, usize, String),
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
    ValidatedString::from_string(unvalidated_string).map_err(
        |ValidatedStringError::StringTooLong(leftover)| {
            ParseAttrsError::AttrTooLong(field_name, string_len, leftover)
        },
    )
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

enum TestrunOrSkipped {
    Testrun(Testrun),
    Skipped,
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
                            ParseAttrsError::AttrTooLong(_, _, _) => {
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
                            ParseAttrsError::AttrTooLong(_, _, _) => {
                                warnings.push(WarningInfo::new(
                                    format!(
                                        "Warning while parsing testcase attributes:\n{}",
                                        error
                                    ),
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

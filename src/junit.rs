use anyhow::{Context, Result};
use pyo3::prelude::*;
use std::collections::HashSet;

use quick_xml::events::attributes::{Attribute, Attributes};
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

use crate::compute_name::{compute_name, unescape_str};
use crate::testrun::{check_testsuites_name, Framework, Outcome, Testrun};
use crate::validated_string::ValidatedString;
use crate::warning::WarningInfo;

fn convert_attribute(attribute: Attribute) -> Result<String> {
    let bytes = attribute.value.into_owned();
    let value = String::from_utf8(bytes).context("Error converting attribute to string")?;
    Ok(value)
}
struct TestcaseAttrs {
    name: ValidatedString,
    time: Option<String>,
    classname: Option<ValidatedString>,
    file: Option<ValidatedString>,
}

enum AttrsOrWarning {
    Attributes(TestcaseAttrs),
    Warning(String),
}

// originally from https://gist.github.com/scott-codecov/311c174ecc7de87f7d7c50371c6ef927#file-cobertura-rs-L18-L31
fn parse_testcase_attrs(attributes: Attributes) -> Result<AttrsOrWarning> {
    let mut name: Option<ValidatedString> = None;
    let mut time: Option<String> = None;
    let mut classname: Option<ValidatedString> = None;
    let mut file: Option<ValidatedString> = None;

    for attribute in attributes {
        let attribute = attribute.context("Error parsing attribute")?;

        match attribute.key.into_inner() {
            b"time" => {
                time = Some(convert_attribute(attribute)?);
            }
            b"classname" => {
                let unvalidated_classname = convert_attribute(attribute)?;
                classname = match ValidatedString::from_string(unvalidated_classname) {
                    Ok(name) => Some(name),
                    Err(_) => {
                        return Ok(AttrsOrWarning::Warning("Error validating classname".into()));
                    }
                };
            }
            b"name" => {
                let unvalidated_name = convert_attribute(attribute)?;
                name = match ValidatedString::from_string(unvalidated_name) {
                    Ok(name) => Some(name),
                    Err(_) => {
                        return Ok(AttrsOrWarning::Warning("Error validating name".into()));
                    }
                };
            }
            b"file" => {
                let unvalidated_file = convert_attribute(attribute)?;
                file = match ValidatedString::from_string(unvalidated_file) {
                    Ok(name) => Some(name),
                    Err(_) => {
                        return Ok(AttrsOrWarning::Warning("Error validating file".into()));
                    }
                };
            }
            _ => {}
        }
    }

    match name {
        Some(name) => Ok(AttrsOrWarning::Attributes(TestcaseAttrs {
            name,
            time,
            classname,
            file,
        })),
        None => anyhow::bail!("No name found"),
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
                    let attrs = parse_testcase_attrs(e.attributes())?;
                    match attrs {
                        AttrsOrWarning::Attributes(attrs) => {
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
                        AttrsOrWarning::Warning(warning) => {
                            warnings.push(WarningInfo::new(warning, reader.buffer_position()));
                            saved_testrun = Some(TestrunOrSkipped::Skipped);
                        }
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
                    let rel_attrs = parse_testcase_attrs(e.attributes())?;
                    match rel_attrs {
                        AttrsOrWarning::Attributes(attrs) => {
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
                        AttrsOrWarning::Warning(warning) => {
                            warnings.push(WarningInfo::new(warning, reader.buffer_position()));
                        }
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

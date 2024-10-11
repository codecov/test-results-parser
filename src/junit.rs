use pyo3::prelude::*;

use quick_xml::escape::unescape;
use quick_xml::events::attributes::Attributes;
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

use crate::testrun::{check_testsuites_name, Outcome, ParsingInfo, Testrun};
use crate::ParserError;

struct RelevantAttrs {
    classname: Option<String>,
    name: Option<String>,
    time: Option<String>,
    file: Option<String>,
}

fn unescape_string(s: String) -> PyResult<String> {
    unescape(&s)
        .map_err(|e| ParserError::new_err(format!("Error unescaping XML: {}", e)))
        .map(|s| s.to_string())
}

// from https://gist.github.com/scott-codecov/311c174ecc7de87f7d7c50371c6ef927#file-cobertura-rs-L18-L31
fn get_relevant_attrs(attributes: Attributes) -> PyResult<RelevantAttrs> {
    let mut rel_attrs: RelevantAttrs = RelevantAttrs {
        time: None,
        classname: None,
        name: None,
        file: None,
    };
    for attribute in attributes {
        let attribute = attribute
            .map_err(|e| ParserError::new_err(format!("Error parsing attribute: {}", e)))?;
        let bytes = attribute.value.into_owned();
        let value = String::from_utf8(bytes)?;
        match attribute.key.into_inner() {
            b"time" => rel_attrs.time = Some(value),
            b"classname" => rel_attrs.classname = Some(unescape_string(value)?),
            b"name" => rel_attrs.name = Some(unescape_string(value)?),
            b"file" => rel_attrs.file = Some(value),
            _ => {}
        }
    }
    Ok(rel_attrs)
}

fn get_attribute(e: &BytesStart, name: &str) -> PyResult<Option<String>> {
    let attr = if let Some(message) = e
        .try_get_attribute(name)
        .map_err(|e| ParserError::new_err(format!("Error parsing attribute: {}", e)))?
    {
        Some(String::from_utf8(message.value.to_vec())?)
    } else {
        None
    };
    Ok(attr)
}

fn populate(
    rel_attrs: RelevantAttrs,
    testsuite: String,
    testsuite_time: Option<String>,
) -> PyResult<Testrun> {
    let classname = rel_attrs.classname.unwrap_or_default();

    let name = rel_attrs
        .name
        .ok_or_else(|| ParserError::new_err("No name found"))?;

    let duration = match rel_attrs.time {
        None => testsuite_time
            .ok_or_else(|| ParserError::new_err("No time/duration found"))?
            .parse()?,
        Some(time_str) => time_str.parse()?,
    };

    Ok(Testrun {
        name,
        classname,
        duration,
        outcome: Outcome::Pass,
        testsuite,
        failure_message: None,
        filename: rel_attrs.file,
        build_url: None,
    })
}

#[pyfunction]
pub fn parse_junit_xml(file_bytes: &[u8]) -> PyResult<ParsingInfo> {
    let mut reader = Reader::from_reader(file_bytes);
    reader.config_mut().trim_text(true);

    let mut testruns: Vec<Testrun> = Vec::new();
    let mut saved_testrun: Option<Testrun> = None;

    let mut in_failure: bool = false;

    let mut buf = Vec::new();

    let mut testsuites_name: Option<String> = None;

    // every time we come across a testsuite element we update this vector:
    // if the testsuite element contains the time attribute append its value to this vec
    // else append a clone of the last value in the vec
    let mut testsuite_time: Vec<Option<String>> = vec![];
    let mut testsuite_names: Vec<Option<String>> = vec![];

    loop {
        let event = reader.read_event_into(&mut buf).map_err(|e| {
            ParserError::new_err(format!(
                "Error parsing XML at position: {} {:?}",
                reader.buffer_position(),
                e
            ))
        })?;
        match event {
            Event::Eof => {
                break;
            }
            Event::Start(e) => match e.name().as_ref() {
                b"testcase" => {
                    let rel_attrs = get_relevant_attrs(e.attributes())?;
                    saved_testrun = Some(populate(
                        rel_attrs,
                        testsuite_names
                            .iter()
                            .rev()
                            .find_map(|e| e.clone())
                            .clone()
                            .ok_or_else(|| ParserError::new_err("No testsuite name found"))?,
                        testsuite_time.iter().rev().find_map(|e| e.clone()),
                    )?);
                }
                b"skipped" => {
                    let testrun = saved_testrun
                        .as_mut()
                        .ok_or_else(|| ParserError::new_err("Error accessing saved testrun"))?;
                    testrun.outcome = Outcome::Skip;
                }
                b"error" => {
                    let testrun = saved_testrun
                        .as_mut()
                        .ok_or_else(|| ParserError::new_err("Error accessing saved testrun"))?;
                    testrun.outcome = Outcome::Error;
                }
                b"failure" => {
                    let testrun = saved_testrun
                        .as_mut()
                        .ok_or_else(|| ParserError::new_err("Error accessing saved testrun"))?;
                    testrun.outcome = Outcome::Failure;

                    testrun.failure_message = get_attribute(&e, "message")?
                        .map(|s| unescape_string(s))
                        .transpose()?;

                    in_failure = true;
                }
                b"testsuite" => {
                    testsuite_names.push(
                        get_attribute(&e, "name")?
                            .map(|s| unescape_string(s))
                            .transpose()?,
                    );
                    testsuite_time.push(get_attribute(&e, "time")?);
                }
                b"testsuites" => {
                    testsuites_name = get_attribute(&e, "name")?
                        .map(|s| unescape_string(s))
                        .transpose()?;
                }
                _ => {}
            },
            Event::End(e) => match e.name().as_ref() {
                b"testcase" => {
                    let testrun = saved_testrun.ok_or_else(|| {
                        ParserError::new_err(
                            "Met testcase closing tag without first meeting testcase opening tag",
                        )
                    })?;
                    testruns.push(testrun);
                    saved_testrun = None;
                }
                b"failure" => in_failure = false,
                b"testsuite" => {
                    testsuite_time.pop();
                    testsuite_names.pop();
                }
                _ => (),
            },
            Event::Empty(e) => {
                if e.name().as_ref() == b"testcase" {
                    let rel_attrs = get_relevant_attrs(e.attributes())?;
                    let testrun = populate(
                        rel_attrs,
                        testsuite_names
                            .last()
                            .unwrap()
                            .to_owned()
                            .ok_or_else(|| ParserError::new_err("No testsuite name found"))?,
                        testsuite_time.last().unwrap().to_owned(),
                    )?;
                    testruns.push(testrun);
                }
            }
            Event::Text(x) => {
                if in_failure {
                    let testrun = saved_testrun
                        .as_mut()
                        .ok_or_else(|| ParserError::new_err("Error accessing saved testrun"))?;

                    let mut xml_failure_message = x.into_owned();
                    xml_failure_message.inplace_trim_end();
                    xml_failure_message.inplace_trim_start();

                    testrun.failure_message =
                        Some(String::from_utf8(xml_failure_message.to_vec())?)
                            .map(|s| unescape_string(s))
                            .transpose()?;
                }
            }

            // There are several other `Event`s we do not consider here
            _ => (),
        }
        buf.clear()
    }

    let mut framework = testruns.iter().filter_map(|t| t.framework()).next();

    if framework.is_none() {
        if let Some(name) = testsuites_name {
            framework = check_testsuites_name(&name);
        }
    }

    Ok(ParsingInfo {
        framework,
        testruns,
    })
}

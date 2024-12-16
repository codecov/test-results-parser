use base64::prelude::*;
use pyo3::prelude::*;
use std::collections::HashSet;
use std::io::prelude::*;

use flate2::bufread::ZlibDecoder;

use quick_xml::events::attributes::Attributes;
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use serde::Deserialize;

use crate::compute_name::{compute_name, unescape_str};
use crate::testrun::{check_testsuites_name, Framework, Outcome, ParsingInfo, Testrun};
use crate::ParserError;

#[derive(Deserialize, Debug, Clone)]
struct TestResultFile {
    filename: String,
    #[serde(skip_deserializing)]
    _format: String,
    data: String,
    #[serde(skip_deserializing)]
    _labels: Vec<String>,
}
#[derive(Deserialize, Debug, Clone)]
struct RawTestResultUpload {
    #[serde(default)]
    network: Option<Vec<String>>,
    test_result_files: Vec<TestResultFile>,
}

#[derive(Debug, Clone)]
struct ReadableFile {
    filename: Vec<u8>,
    data: Vec<u8>,
}

const LEGACY_FORMAT_PREFIX: &[u8] = b"# path=";
const LEGACY_FORMAT_SUFFIX: &[u8] = b"<<<<<< EOF";

fn serialize_to_legacy_format(readable_files: Vec<ReadableFile>) -> Vec<u8> {
    let mut res = Vec::new();
    for file in readable_files {
        res.extend_from_slice(LEGACY_FORMAT_PREFIX);
        res.extend_from_slice(&file.filename);
        res.extend_from_slice(b"\n");
        res.extend_from_slice(&file.data);
        res.extend_from_slice(b"\n");
        res.extend_from_slice(LEGACY_FORMAT_SUFFIX);
        res.extend_from_slice(b"\n");
    }
    res
}

#[derive(Default)]
struct RelevantAttrs {
    classname: Option<String>,
    name: Option<String>,
    time: Option<String>,
    file: Option<String>,
}

// from https://gist.github.com/scott-codecov/311c174ecc7de87f7d7c50371c6ef927#file-cobertura-rs-L18-L31
fn get_relevant_attrs(attributes: Attributes) -> PyResult<RelevantAttrs> {
    let mut rel_attrs = RelevantAttrs::default();
    for attribute in attributes {
        let attribute = attribute
            .map_err(|e| ParserError::new_err(format!("Error parsing attribute: {}", e)))?;
        let bytes = attribute.value.into_owned();
        let value = String::from_utf8(bytes)?;
        match attribute.key.into_inner() {
            b"time" => rel_attrs.time = Some(value),
            b"classname" => rel_attrs.classname = Some(value),
            b"name" => rel_attrs.name = Some(value),
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
    testsuite_time: Option<&str>,
    framework: Option<Framework>,
    network: Option<&HashSet<String>>,
) -> PyResult<(Testrun, Option<Framework>)> {
    let classname = rel_attrs.classname.unwrap_or_default();

    let name = rel_attrs
        .name
        .ok_or_else(|| ParserError::new_err("No name found"))?;

    let duration = rel_attrs
        .time
        .as_deref()
        .or(testsuite_time)
        .and_then(|t| t.parse().ok());

    let mut t = Testrun {
        name,
        classname,
        duration,
        outcome: Outcome::Pass,
        testsuite,
        failure_message: None,
        filename: rel_attrs.file,
        build_url: None,
        computed_name: None,
    };

    let framework = framework.or_else(|| t.framework());
    if let Some(f) = framework {
        let computed_name = compute_name(&t.classname, &t.name, f, t.filename.as_deref(), network);
        t.computed_name = Some(computed_name);
    };

    Ok((t, framework))
}

#[pyfunction]
#[pyo3(signature = (raw_upload_bytes))]
pub fn parse_raw_upload(raw_upload_bytes: &[u8]) -> PyResult<(Vec<u8>, Vec<u8>)> {
    let upload: RawTestResultUpload = serde_json::from_slice(raw_upload_bytes)
        .map_err(|e| ParserError::new_err(format!("Error deserializing json: {}", e)))?;
    let network: Option<HashSet<String>> = upload.network.map(|v| v.into_iter().collect());

    let mut results: Vec<ParsingInfo> = Vec::new();
    let mut readable_files: Vec<ReadableFile> = Vec::new();

    for file in upload.test_result_files {
        let decoded_file_bytes = BASE64_STANDARD
            .decode(file.data)
            .map_err(|e| ParserError::new_err(format!("Error decoding base64: {}", e)))?;

        let mut decoder = ZlibDecoder::new(&decoded_file_bytes[..]);

        let mut decompressed_file_bytes = Vec::new();
        decoder
            .read_to_end(&mut decompressed_file_bytes)
            .map_err(|e| ParserError::new_err(format!("Error decompressing file: {}", e)))?;

        let mut reader = Reader::from_reader(&decompressed_file_bytes[..]);
        reader.config_mut().trim_text(true);
        let reader_result = use_reader(&mut reader, network.as_ref()).map_err(|e| {
            let pos = reader.buffer_position();
            let (line, col) = get_position_info(&decompressed_file_bytes, pos.try_into().unwrap());
            ParserError::new_err(format!(
                "Error parsing JUnit XML at {}:{}: {}",
                line, col, e
            ))
        })?;
        results.push(reader_result);

        let readable_file = ReadableFile {
            data: decompressed_file_bytes,
            filename: file.filename.into_bytes(),
        };
        readable_files.push(readable_file);
    }

    let results_bytes = rmp_serde::to_vec_named(&results)
        .map_err(|_| ParserError::new_err("Error serializing pr comment summary"))?;

    let readable_file = serialize_to_legacy_format(readable_files);

    Ok((results_bytes, readable_file))
}

fn get_position_info(input: &[u8], byte_offset: usize) -> (usize, usize) {
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

fn use_reader(
    reader: &mut Reader<&[u8]>,
    network: Option<&HashSet<String>>,
) -> PyResult<ParsingInfo> {
    let mut testruns: Vec<Testrun> = Vec::new();
    let mut saved_testrun: Option<Testrun> = None;

    let mut in_failure: bool = false;

    let mut framework: Option<Framework> = None;

    // every time we come across a testsuite element we update this vector:
    // if the testsuite element contains the time attribute append its value to this vec
    // else append a clone of the last value in the vec
    let mut testsuite_names: Vec<Option<String>> = vec![];
    let mut testsuite_times: Vec<Option<String>> = vec![];

    let mut buf = Vec::new();
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
                    let (testrun, parsed_framework) = populate(
                        rel_attrs,
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
                _ => {}
            },
            Event::End(e) => match e.name().as_ref() {
                b"testcase" => {
                    let testrun = saved_testrun.take().ok_or_else(|| {
                        ParserError::new_err(
                            "Met testcase closing tag without first meeting testcase opening tag",
                        )
                    })?;
                    testruns.push(testrun);
                }
                b"failure" => in_failure = false,
                b"testsuite" => {
                    testsuite_times.pop();
                    testsuite_names.pop();
                }
                _ => (),
            },
            Event::Empty(e) => match e.name().as_ref() {
                b"testcase" => {
                    let rel_attrs = get_relevant_attrs(e.attributes())?;
                    let (testrun, parsed_framework) = populate(
                        rel_attrs,
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
                b"failure" => {
                    let testrun = saved_testrun
                        .as_mut()
                        .ok_or_else(|| ParserError::new_err("Error accessing saved testrun"))?;
                    testrun.outcome = Outcome::Failure;

                    testrun.failure_message = get_attribute(&e, "message")?
                        .map(|failure_message| unescape_str(&failure_message).into());
                }
                _ => {}
            },
            Event::Text(mut xml_failure_message) => {
                if in_failure {
                    let testrun = saved_testrun
                        .as_mut()
                        .ok_or_else(|| ParserError::new_err("Error accessing saved testrun"))?;

                    xml_failure_message.inplace_trim_end();
                    xml_failure_message.inplace_trim_start();

                    testrun.failure_message =
                        Some(unescape_str(std::str::from_utf8(&xml_failure_message)?).into());
                }
            }

            // There are several other `Event`s we do not consider here
            _ => (),
        }
        buf.clear()
    }

    Ok(ParsingInfo {
        framework,
        testruns,
    })
}

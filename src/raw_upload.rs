use anyhow::Context;

use base64::prelude::*;
use pyo3::prelude::*;
use std::collections::HashSet;
use std::io::prelude::*;

use flate2::bufread::ZlibDecoder;

use quick_xml::reader::Reader;
use serde::Deserialize;

use crate::junit::{get_position_info, use_reader};
use crate::testrun::ParsingInfo;
use crate::warning::WarningInfo;

#[derive(Deserialize, Debug, Clone)]
struct TestResultFile {
    filename: String,
    data: String,
}
#[derive(Deserialize, Debug, Clone)]
struct RawTestResultUpload {
    #[serde(default)]
    network: Option<HashSet<String>>,
    test_results_files: Vec<TestResultFile>,
}

#[derive(Debug, Clone)]
struct ReadableFile {
    filename: String,
    data: Vec<u8>,
}

const LEGACY_FORMAT_PREFIX: &[u8] = b"# path=";
const LEGACY_FORMAT_SUFFIX: &[u8] = b"<<<<<< EOF";

fn serialize_to_legacy_format(readable_files: Vec<ReadableFile>) -> Vec<u8> {
    let mut res = Vec::new();
    for file in readable_files {
        res.extend_from_slice(LEGACY_FORMAT_PREFIX);
        res.extend_from_slice(file.filename.as_bytes());
        res.extend_from_slice(b"\n");
        res.extend_from_slice(&file.data);
        res.extend_from_slice(b"\n");
        res.extend_from_slice(LEGACY_FORMAT_SUFFIX);
        res.extend_from_slice(b"\n");
    }
    res
}

// the warnings should be ordered by location because they're pushed to the vec as we parse
// so we can guarantee that warning[x].location >= warning[x - 1].location
// implicitly tested by warnings-junit.xml
fn format_warnings(input: &[u8], warnings: Vec<WarningInfo>, filename: &str) -> Vec<String> {
    let mut offset = 0;
    let mut result = Vec::new();
    let mut line = 1;
    let mut col = 0;
    let mut input_iter = input.iter();
    for warning in warnings {
        for bytes in input_iter
            .by_ref()
            .take((warning.location - offset) as usize)
        {
            if *bytes == b'\n' {
                line += 1;
                col = 0;
            } else {
                col += 1;
            }
        }
        offset += warning.location;
        result.push(if warning.message.contains('\n') {
            format!("{}\n\nAt {}:{} in {}", warning.message, line, col, filename)
        } else {
            format!("{} at {}:{} in {}", warning.message, line, col, filename)
        });
    }
    result
}

#[pyfunction]
#[pyo3(signature = (raw_upload_bytes))]
pub fn parse_raw_upload(raw_upload_bytes: &[u8]) -> anyhow::Result<(Vec<ParsingInfo>, Vec<u8>)> {
    let upload: RawTestResultUpload =
        serde_json::from_slice(raw_upload_bytes).context("Error deserializing json")?;
    let network: Option<HashSet<String>> = upload.network;

    let mut results: Vec<ParsingInfo> = Vec::with_capacity(upload.test_results_files.len());
    let mut readable_files: Vec<ReadableFile> = Vec::with_capacity(upload.test_results_files.len());

    for file in upload.test_results_files {
        let decoded_file_bytes = BASE64_STANDARD
            .decode(file.data)
            .context("Error decoding base64")?;

        let mut decoder = ZlibDecoder::new(decoded_file_bytes.as_slice());

        let mut decompressed_file_bytes = Vec::new();
        decoder
            .read_to_end(&mut decompressed_file_bytes)
            .context("Error decompressing file")?;

        let mut reader = Reader::from_reader(decompressed_file_bytes.as_slice());
        reader.config_mut().trim_text(true);
        let (framework, testruns, warnings) = use_reader(&mut reader, network.as_ref())
            .with_context(|| {
                let pos_conversion = reader.buffer_position().try_into();
                match pos_conversion {
                    Ok(pos) => {
                        let (line, col) = get_position_info(&decompressed_file_bytes, pos);
                        format!(
                            "Error parsing JUnit XML in {} at {}:{}",
                            file.filename, line, col
                        )
                    }
                    Err(_) => format!("Error parsing JUnit XML in {}", file.filename),
                }
            })?;

        let warning_strings: Vec<String> =
            format_warnings(&decompressed_file_bytes, warnings, &file.filename);

        let parsing_info = ParsingInfo {
            framework,
            testruns,
            warnings: warning_strings,
        };
        results.push(parsing_info);

        let readable_file = ReadableFile {
            data: decompressed_file_bytes,
            filename: file.filename,
        };
        readable_files.push(readable_file);
    }

    let readable_file = serialize_to_legacy_format(readable_files);

    Ok((results, readable_file))
}

#[cfg(test)]
mod tests {
    use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
    use base64::Engine;
    use flate2::Compression;
    use std::io::Write;

    use super::*;
    use insta::{assert_yaml_snapshot, glob};

    fn file_into_bytes(filename: &str) -> Vec<u8> {
        let upload = std::fs::read(filename).unwrap();
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&upload).unwrap();
        let compressed = encoder.finish().unwrap();
        let base64_data = BASE64_STANDARD.encode(compressed);
        let upload_json = format!(
            r#"{{"network": [], "test_results_files": [{{"filename": "{}", "format": "base64+compressed", "data": "{}"}}]}}"#,
            filename.split('/').next_back().unwrap(),
            base64_data,
        );
        upload_json.into()
    }

    #[test]
    fn test_parse_raw_upload_success() {
        glob!("../tests", "*.xml", |path| {
            let upload_json = file_into_bytes(path.to_str().unwrap());
            let result = parse_raw_upload(&upload_json);
            match result {
                Ok((results, _)) => assert_yaml_snapshot!(results),
                Err(e) => {
                    assert_yaml_snapshot!(e.to_string());
                }
            }
        });
    }
}

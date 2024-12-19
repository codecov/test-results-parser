use base64::prelude::*;
use pyo3::prelude::*;
use std::collections::HashSet;
use std::io::prelude::*;

use flate2::bufread::ZlibDecoder;

use quick_xml::reader::Reader;
use serde::Deserialize;

use crate::junit::{get_position_info, use_reader};
use crate::testrun::ParsingInfo;
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
    test_results_files: Vec<TestResultFile>,
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

#[pyfunction]
#[pyo3(signature = (raw_upload_bytes))]
pub fn parse_raw_upload(raw_upload_bytes: &[u8]) -> PyResult<(Vec<u8>, Vec<u8>)> {
    let upload: RawTestResultUpload = serde_json::from_slice(raw_upload_bytes)
        .map_err(|e| ParserError::new_err(format!("Error deserializing json: {}", e)))?;
    let network: Option<HashSet<String>> = upload.network.map(|v| v.into_iter().collect());

    let mut results: Vec<ParsingInfo> = Vec::new();
    let mut readable_files: Vec<ReadableFile> = Vec::new();

    for file in upload.test_results_files {
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

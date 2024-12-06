use std::mem::transmute;

use anyhow::Context;
use pyo3::prelude::*;

use crate::Testrun;

use super::{TestAnalytics, TestAnalyticsWriter};

#[pyclass]
pub struct BinaryFormatWriter {
    writer: TestAnalyticsWriter,
}

impl BinaryFormatWriter {
    pub fn new() -> Self {
        Self {
            writer: TestAnalyticsWriter::new(60),
        }
    }
    pub fn add_testruns(
        &mut self,
        timestamp: u32,
        commit_hash: &str,
        flags: &[&str],
        testruns: &[Testrun],
    ) -> anyhow::Result<()> {
        let commit_hash_base16 = if commit_hash.len() > 40 {
            commit_hash
                .get(..40)
                .context("expected a hex-encoded commit hash")?
        } else {
            commit_hash
        };
        let mut commit_hash = super::CommitHash::default();
        base16ct::mixed::decode(commit_hash_base16, &mut commit_hash.0)?;

        let mut session = self.writer.start_session(timestamp, commit_hash, flags);
        for test in testruns {
            session.insert(test);
        }
        Ok(())
    }

    pub fn serialize(self) -> anyhow::Result<Vec<u8>> {
        let mut buffer = vec![];
        self.writer.serialize(&mut buffer)?;
        Ok(buffer)
    }
}

#[pyclass]
pub struct AggregationReader {
    buffer: Vec<u8>,
    format: TestAnalytics<'static>,
}

#[pyclass]
pub struct TestAggregate {
    // TODO
}

#[pymethods]
impl AggregationReader {
    #[new]
    pub fn new(buffer: Vec<u8>, timestamp: u32) -> anyhow::Result<Self> {
        let format = TestAnalytics::parse(&buffer, timestamp)?;
        // SAFETY: the lifetime of `TestAnalytics` depends on `buffer`,
        // which we do not mutate, and which outlives the parsed format.
        let format = unsafe { transmute(format) };

        Ok(Self { buffer, format })
    }

    #[pyo3(signature = (interval_start, interval_end, flag=None))]
    pub fn get_test_aggregates(
        &self,
        interval_start: usize,
        interval_end: usize,
        flag: Option<&str>,
    ) -> Vec<TestAggregate> {
        vec![]
    }
}

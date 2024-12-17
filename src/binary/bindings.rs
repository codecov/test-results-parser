use std::mem::transmute;

use anyhow::Context;
use pyo3::prelude::*;

use crate::Testrun;

use super::{TestAnalytics, TestAnalyticsWriter};

#[pyclass]
pub struct BinaryFormatWriter {
    writer: Option<TestAnalyticsWriter>,
}

#[pymethods]
impl BinaryFormatWriter {
    #[new]
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            writer: Some(TestAnalyticsWriter::new(60)),
        }
    }

    #[staticmethod]
    pub fn open(buffer: &[u8]) -> anyhow::Result<Self> {
        let format = TestAnalytics::parse(buffer, 0)?;
        let writer = TestAnalyticsWriter::from_existing_format(&format)?;
        Ok(Self {
            writer: Some(writer),
        })
    }

    pub fn add_testruns(
        &mut self,
        timestamp: u32,
        commit_hash: &str,
        flags: Vec<String>,
        testruns: Vec<Testrun>,
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

        let writer = self
            .writer
            .as_mut()
            .context("writer was already serialized")?;

        let flags: Vec<_> = flags.iter().map(|s| s.as_str()).collect();
        let mut session = writer.start_session(timestamp, commit_hash, &flags);
        for test in testruns {
            session.insert(&test);
        }
        Ok(())
    }

    pub fn serialize(&mut self) -> anyhow::Result<Vec<u8>> {
        let writer = self
            .writer
            .take()
            .context("writer was already serialized")?;
        let mut buffer = vec![];
        writer.serialize(&mut buffer)?;
        Ok(buffer)
    }
}

#[pyclass]
pub struct AggregationReader {
    _buffer: Vec<u8>,
    format: TestAnalytics<'static>,
}

#[pyclass(get_all)]
pub struct TestAggregate {
    pub name: String,
    // TODO:
    pub test_id: String,

    pub testsuite: Option<String>,
    pub flags: Vec<String>,

    pub failure_rate: f32,
    pub flake_rate: f32,

    // TODO:
    pub updated_at: u32,
    pub avg_duration: f64,

    pub total_fail_count: u32,
    pub total_flaky_fail_count: u32,
    pub total_pass_count: u32,
    pub total_skip_count: u32,

    pub commits_where_fail: usize,

    // TODO:
    pub last_duration: f32,
}

#[pymethods]
impl AggregationReader {
    #[new]
    pub fn new(buffer: Vec<u8>, timestamp: u32) -> anyhow::Result<Self> {
        let format = TestAnalytics::parse(&buffer, timestamp)?;
        // SAFETY: the lifetime of `TestAnalytics` depends on `buffer`,
        // which we do not mutate, and which outlives the parsed format.
        let format = unsafe { transmute::<TestAnalytics<'_>, TestAnalytics<'_>>(format) };

        Ok(Self {
            _buffer: buffer,
            format,
        })
    }

    #[pyo3(signature = (interval_start, interval_end, flags=None))]
    pub fn get_test_aggregates(
        &self,
        interval_start: usize,
        interval_end: usize,
        flags: Option<Vec<String>>,
    ) -> anyhow::Result<Vec<TestAggregate>> {
        let flags: Option<Vec<_>> = flags
            .as_ref()
            .map(|flags| flags.iter().map(|flag| flag.as_str()).collect());
        let desired_range = interval_start..interval_end;

        let tests = self.format.tests(desired_range, flags.as_deref())?;
        let mut collected_tests = vec![];

        for test in tests {
            let test = test?;

            collected_tests.push(TestAggregate {
                name: test.name()?.into(),
                test_id: "TODO".into(),
                testsuite: Some(test.testsuite()?.into()),
                flags: test.flags()?.into_iter().map(|s| s.into()).collect(),
                failure_rate: test.aggregates().failure_rate,
                flake_rate: test.aggregates().flake_rate,
                updated_at: 0, // TODO
                avg_duration: test.aggregates().avg_duration,
                total_fail_count: test.aggregates().total_fail_count,
                total_flaky_fail_count: test.aggregates().total_flaky_fail_count,
                total_pass_count: test.aggregates().total_pass_count,
                total_skip_count: test.aggregates().total_skip_count,
                commits_where_fail: test.aggregates().failing_commits,
                last_duration: 0., // TODO
            });
        }

        Ok(collected_tests)
    }
}

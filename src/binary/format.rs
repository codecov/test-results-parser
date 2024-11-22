use std::fmt;
use std::ops::Range;

use timestamps::{adjust_selection_range, offset_from_today};
use watto::Pod;

use super::*;

/// The current format version.
pub(crate) const TA_VERSION: u32 = 1;

/// The serialized [`TestAnalytics`] binary format.
///
/// This can be parsed from a binary buffer via [`TestAnalytics::parse`].
#[derive(Clone)]
pub struct TestAnalytics<'data> {
    pub(crate) timestamp: u32,
    pub(crate) header: &'data raw::Header,
    pub(crate) tests: &'data [raw::Test],
    pub(crate) testdata: &'data [raw::TestData],
    pub(crate) string_bytes: &'data [u8],
}

impl<'data> TestAnalytics<'data> {
    /// Parses the given buffer into [`TestAnalytics`].
    pub fn parse(buf: &'data [u8], timestamp: u32) -> Result<Self, TestAnalyticsError> {
        let (header, rest) =
            raw::Header::ref_from_prefix(buf).ok_or(TestAnalyticsErrorKind::InvalidHeader)?;

        if header.magic != raw::TA_MAGIC {
            return Err(TestAnalyticsErrorKind::InvalidMagic(header.magic).into());
        }

        if header.version != TA_VERSION {
            return Err(TestAnalyticsErrorKind::WrongVersion(header.version).into());
        }

        let (tests, rest) = raw::Test::slice_from_prefix(rest, header.num_tests as usize)
            .ok_or(TestAnalyticsErrorKind::InvalidTables)?;

        let expected_data = header.num_tests as usize * header.num_days as usize;

        let (testdata, rest) = raw::TestData::slice_from_prefix(rest, expected_data)
            .ok_or(TestAnalyticsErrorKind::InvalidTables)?;

        let string_bytes = rest.get(..header.string_bytes as usize).ok_or(
            TestAnalyticsErrorKind::UnexpectedStringBytes {
                expected: header.string_bytes as usize,
                found: rest.len(),
            },
        )?;

        Ok(Self {
            timestamp: timestamp.max(header.timestamp),
            header,
            tests,
            testdata,
            string_bytes,
        })
    }

    /// Iterates over the [`Test`]s included in the [`TestAnalytics`] summary.
    pub fn tests(&self) -> impl Iterator<Item = Test<'data, '_>> + '_ {
        let num_days = self.header.num_days as usize;
        self.tests.iter().enumerate().map(move |(i, test)| {
            let start_idx = i * num_days;
            let latest_test_timestamp = self.testdata[start_idx].last_timestamp;
            let today_offset = offset_from_today(latest_test_timestamp, self.timestamp);

            let data_range = start_idx..start_idx + num_days;
            Test {
                today_offset,
                container: self,
                data: test,
                data_range,
            }
        })
    }
}

impl<'data> fmt::Debug for TestAnalytics<'data> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TestAnalytics")
            .field("version", &self.header.version)
            .field("tests", &self.header.num_tests)
            .field("days", &self.header.num_days)
            .field("string_bytes", &self.header.string_bytes)
            .finish()
    }
}

/// This represents a specific test for which test analytics data is gathered.
#[derive(Debug, Clone)]
pub struct Test<'data, 'parsed> {
    today_offset: usize,
    container: &'parsed TestAnalytics<'data>,

    data: &'data raw::Test,
    data_range: Range<usize>,
}

impl<'data, 'parsed> Test<'data, 'parsed> {
    /// Returns the testsuite of the test.
    pub fn testsuite(&self) -> Result<&'data str, TestAnalyticsError> {
        watto::StringTable::read(
            self.container.string_bytes,
            self.data.testsuite_offset as usize,
        )
        .map_err(|_| TestAnalyticsErrorKind::InvalidStringReference.into())
    }

    /// Returns the name of the test.
    pub fn name(&self) -> Result<&'data str, TestAnalyticsError> {
        watto::StringTable::read(self.container.string_bytes, self.data.name_offset as usize)
            .map_err(|_| TestAnalyticsErrorKind::InvalidStringReference.into())
    }

    /// Calculates aggregate data for the given [`Range`] of days.
    pub fn get_aggregates(&self, desired_range: Range<usize>) -> Aggregates {
        let adjusted_range =
            adjust_selection_range(self.data_range.clone(), desired_range, self.today_offset);

        let mut total_pass_count = 0;
        let mut total_fail_count = 0;
        let mut total_skip_count = 0;
        let mut total_flaky_fail_count = 0;
        let mut total_duration = 0.;
        for testdata in &self.container.testdata[adjusted_range] {
            total_pass_count += testdata.total_pass_count as u32;
            total_fail_count += testdata.total_fail_count as u32;
            total_skip_count += testdata.total_skip_count as u32;
            total_flaky_fail_count += testdata.total_flaky_fail_count as u32;
            total_duration += testdata.total_duration as f64;
        }

        let total_run_count = total_pass_count + total_fail_count;
        let (failure_rate, flake_rate, avg_duration) = if total_run_count > 0 {
            (
                total_fail_count as f32 / total_run_count as f32,
                total_flaky_fail_count as f32 / total_run_count as f32,
                total_duration / total_run_count as f64,
            )
        } else {
            (0., 0., 0.)
        };

        Aggregates {
            total_pass_count,
            total_fail_count,
            total_skip_count,
            total_flaky_fail_count,

            failure_rate,
            flake_rate,

            avg_duration,
        }
    }
}

/// Contains test run data aggregated over a given time period.
#[derive(Debug, Clone, PartialEq)]
pub struct Aggregates {
    pub total_pass_count: u32,
    pub total_fail_count: u32,
    pub total_skip_count: u32,
    pub total_flaky_fail_count: u32,

    pub failure_rate: f32,
    pub flake_rate: f32,

    pub avg_duration: f64,
}

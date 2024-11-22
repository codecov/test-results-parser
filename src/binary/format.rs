use std::fmt;
use std::ops::Range;

use timestamps::{adjust_selection_range, offset_from_today};
use watto::{align_to, Pod};

use super::*;

/// The current format version.
pub(crate) const TA_VERSION: u32 = 1;

/// The serialized [`TestAnalytics`] binary format.
///
/// This can be parsed from a binary buffer via [`TestAnalytics::parse`].
#[derive(Clone, PartialEq)]
pub struct TestAnalytics<'data> {
    pub(crate) header: &'data raw::Header,
    pub(crate) tests: &'data [raw::Test],
    pub(crate) timestamp: u32,

    pub(crate) total_pass_count: &'data [u16],
    pub(crate) total_fail_count: &'data [u16],
    pub(crate) total_skip_count: &'data [u16],
    pub(crate) total_flaky_fail_count: &'data [u16],
    pub(crate) total_duration: &'data [f32],

    pub(crate) last_timestamp: &'data [u32],
    pub(crate) last_duration: &'data [f32],

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

        let (_, rest) = align_to(rest, 8).ok_or(TestAnalyticsErrorKind::InvalidTables)?;
        let (tests, rest) = raw::Test::slice_from_prefix(rest, header.num_tests as usize)
            .ok_or(TestAnalyticsErrorKind::InvalidTables)?;

        let expected_data = header.num_tests as usize * header.num_days as usize;

        let (_, rest) = align_to(rest, 8).ok_or(TestAnalyticsErrorKind::InvalidTables)?;
        let (total_pass_count, rest) = u16::slice_from_prefix(rest, expected_data)
            .ok_or(TestAnalyticsErrorKind::InvalidTables)?;

        let (_, rest) = align_to(rest, 8).ok_or(TestAnalyticsErrorKind::InvalidTables)?;
        let (total_fail_count, rest) = u16::slice_from_prefix(rest, expected_data)
            .ok_or(TestAnalyticsErrorKind::InvalidTables)?;

        let (_, rest) = align_to(rest, 8).ok_or(TestAnalyticsErrorKind::InvalidTables)?;
        let (total_skip_count, rest) = u16::slice_from_prefix(rest, expected_data)
            .ok_or(TestAnalyticsErrorKind::InvalidTables)?;

        let (_, rest) = align_to(rest, 8).ok_or(TestAnalyticsErrorKind::InvalidTables)?;
        let (total_flaky_fail_count, rest) = u16::slice_from_prefix(rest, expected_data)
            .ok_or(TestAnalyticsErrorKind::InvalidTables)?;

        let (_, rest) = align_to(rest, 8).ok_or(TestAnalyticsErrorKind::InvalidTables)?;
        let (total_duration, rest) = f32::slice_from_prefix(rest, expected_data)
            .ok_or(TestAnalyticsErrorKind::InvalidTables)?;

        let (_, rest) = align_to(rest, 8).ok_or(TestAnalyticsErrorKind::InvalidTables)?;
        let (last_timestamp, rest) = u32::slice_from_prefix(rest, expected_data)
            .ok_or(TestAnalyticsErrorKind::InvalidTables)?;

        let (_, rest) = align_to(rest, 8).ok_or(TestAnalyticsErrorKind::InvalidTables)?;
        let (last_duration, rest) = f32::slice_from_prefix(rest, expected_data)
            .ok_or(TestAnalyticsErrorKind::InvalidTables)?;

        let (_, rest) = align_to(rest, 8).ok_or(TestAnalyticsErrorKind::UnexpectedStringBytes {
            expected: header.string_bytes as usize,
            found: 0,
        })?;
        let string_bytes = rest.get(..header.string_bytes as usize).ok_or(
            TestAnalyticsErrorKind::UnexpectedStringBytes {
                expected: header.string_bytes as usize,
                found: rest.len(),
            },
        )?;

        Ok(Self {
            header,
            tests,
            timestamp: timestamp.max(header.timestamp),

            total_pass_count,
            total_fail_count,
            total_skip_count,
            total_flaky_fail_count,
            total_duration,

            last_timestamp,
            last_duration,

            string_bytes,
        })
    }

    /// Iterates over the [`Test`]s included in the [`TestAnalytics`] summary.
    pub fn tests(&self) -> impl Iterator<Item = Test<'data, '_>> + '_ {
        let num_days = self.header.num_days as usize;
        self.tests.iter().enumerate().map(move |(i, test)| {
            let start_idx = i * num_days;
            let latest_test_timestamp = self.last_timestamp[start_idx];
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
#[derive(Debug, Clone, PartialEq)]
pub struct Test<'data, 'parsed> {
    today_offset: usize,
    container: &'parsed TestAnalytics<'data>,

    data: &'data raw::Test,
    data_range: Range<usize>,
}

impl<'data, 'parsed> Test<'data, 'parsed> {
    /// Returns the name of the test.
    pub fn name(&self) -> Result<&'data str, TestAnalyticsError> {
        watto::StringTable::read(self.container.string_bytes, self.data.name_offset as usize)
            .map_err(|_| TestAnalyticsErrorKind::InvalidStringReference.into())
    }

    /// Calculates aggregate data for the given [`Range`] of days.
    pub fn get_aggregates(&self, desired_range: Range<usize>) -> Aggregates {
        let adjusted_range =
            adjust_selection_range(self.data_range.clone(), desired_range, self.today_offset);

        let total_pass_count = self.container.total_pass_count[adjusted_range.clone()]
            .iter()
            .map(|c| *c as u32)
            .sum();
        let total_fail_count = self.container.total_fail_count[adjusted_range.clone()]
            .iter()
            .map(|c| *c as u32)
            .sum();
        let total_skip_count = self.container.total_skip_count[adjusted_range.clone()]
            .iter()
            .map(|c| *c as u32)
            .sum();
        let total_flaky_fail_count = self.container.total_flaky_fail_count[adjusted_range.clone()]
            .iter()
            .map(|c| *c as u32)
            .sum();
        let total_duration: f64 = self.container.total_duration[adjusted_range.clone()]
            .iter()
            .map(|d| *d as f64)
            .sum();

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

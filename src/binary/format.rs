use std::fmt;
use std::ops::Range;

use watto::{align_to, Pod};

use super::raw;

pub(crate) const TA_VERSION: u32 = 1;

/// The serialized TestAnalytics binary format.
///
/// This can be parsed from a binary buffer via [`TestAnalytics::parse`].
#[derive(Clone, PartialEq)]
pub struct TestAnalytics<'data> {
    header: &'data raw::Header,
    tests: &'data [raw::Test],

    total_pass_count: &'data [u16],
    total_fail_count: &'data [u16],
    total_skip_count: &'data [u16],
    total_flaky_fail_count: &'data [u16],
    total_duration: &'data [f32],
    // last_duration: &'data [f32],
    // latest_run: &'data [u32],
    string_bytes: &'data [u8],
}

impl<'data> TestAnalytics<'data> {
    /// Parses the given buffer into [`TestAnalytics`].
    pub fn parse(buf: &'data [u8]) -> Option<Self> {
        let (header, rest) = raw::Header::ref_from_prefix(buf)?;

        if header.magic != raw::TA_MAGIC {
            return None;
        }

        if header.version != TA_VERSION {
            return None;
        }

        let (_, rest) = align_to(rest, 8)?;
        let (tests, rest) = raw::Test::slice_from_prefix(rest, header.num_tests as usize)?;

        let expected_data = header.num_tests as usize * header.num_days as usize;

        let (_, rest) = align_to(rest, 8)?;
        let (total_pass_count, rest) = u16::slice_from_prefix(rest, expected_data)?;

        let (_, rest) = align_to(rest, 8)?;
        let (total_fail_count, rest) = u16::slice_from_prefix(rest, expected_data)?;

        let (_, rest) = align_to(rest, 8)?;
        let (total_skip_count, rest) = u16::slice_from_prefix(rest, expected_data)?;

        let (_, rest) = align_to(rest, 8)?;
        let (total_flaky_fail_count, rest) = u16::slice_from_prefix(rest, expected_data)?;

        let (_, rest) = align_to(rest, 8)?;
        let (total_duration, rest) = f32::slice_from_prefix(rest, expected_data)?;

        let (_, rest) = align_to(rest, 8)?;
        let string_bytes = rest.get(..header.string_bytes as usize)?;

        Some(Self {
            header,
            tests,

            total_pass_count,
            total_fail_count,
            total_skip_count,
            total_flaky_fail_count,
            total_duration,

            string_bytes,
        })
    }

    pub fn tests(&self) -> impl Iterator<Item = Option<Test<'data>>> + '_ {
        self.tests.iter().enumerate().map(|(i, test)| {
            let data_range = (i * self.header.num_days as usize)..;
            let name =
                watto::StringTable::read(self.string_bytes, test.name_offset as usize).ok()?;

            Some(Test {
                name,
                num_days: self.header.num_days as usize,
                total_pass_count: &self.total_pass_count[data_range.clone()],
                total_fail_count: &self.total_fail_count[data_range.clone()],
                total_skip_count: &self.total_skip_count[data_range.clone()],
                total_flaky_fail_count: &self.total_flaky_fail_count[data_range.clone()],
                total_duration: &self.total_duration[data_range.clone()],
            })
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

#[derive(Clone, PartialEq)]
pub struct Test<'data> {
    name: &'data str,

    num_days: usize,
    total_pass_count: &'data [u16],
    total_fail_count: &'data [u16],
    total_skip_count: &'data [u16],
    total_flaky_fail_count: &'data [u16],
    total_duration: &'data [f32],
}

impl<'data> Test<'data> {
    pub fn name(&self) -> &'data str {
        self.name
    }

    pub fn get_aggregates(&self, range: Range<usize>) -> Aggregates {
        let range =
            self.num_days.saturating_sub(range.start)..self.num_days.saturating_sub(range.end);

        let total_pass_count = self.total_pass_count[range.clone()]
            .iter()
            .map(|c| *c as u32)
            .sum();
        let total_fail_count = self.total_fail_count[range.clone()]
            .iter()
            .map(|c| *c as u32)
            .sum();
        let total_skip_count = self.total_skip_count[range.clone()]
            .iter()
            .map(|c| *c as u32)
            .sum();
        let total_flaky_fail_count = self.total_flaky_fail_count[range.clone()]
            .iter()
            .map(|c| *c as u32)
            .sum();
        let total_duration: f64 = self.total_duration[range.clone()]
            .iter()
            .map(|d| *d as f64)
            .sum();

        let total_run_count = total_pass_count + total_fail_count;
        let avg_duration = if total_run_count > 0 {
            total_duration / total_run_count as f64
        } else {
            0.
        };

        Aggregates {
            total_pass_count,
            total_fail_count,
            total_skip_count,
            total_flaky_fail_count,
            avg_duration,
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct Aggregates {
    pub total_pass_count: u32,
    pub total_fail_count: u32,
    pub total_skip_count: u32,
    pub total_flaky_fail_count: u32,

    pub avg_duration: f64,
}

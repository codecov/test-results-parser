use std::collections::HashSet;
use std::fmt;
use std::ops::Range;

use commithashes_set::CommitHashesSet;
use flags_set::FlagsSet;
use smallvec::SmallVec;
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

    pub(crate) flags_set: FlagsSet<'data>,
    pub(crate) commithashes_bytes: &'data [u8],
    pub(crate) string_bytes: &'data [u8],

    pub(crate) tests: &'data [raw::Test],
    pub(crate) testdata: &'data [raw::TestData],
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

        let (flags_set, rest) = u32::slice_from_prefix(rest, header.flags_set_len as usize)
            .ok_or(TestAnalyticsErrorKind::InvalidTables)?;

        let (commithashes_bytes, rest) =
            u8::slice_from_prefix(rest, header.commithashes_bytes as usize)
                .ok_or(TestAnalyticsErrorKind::InvalidTables)?;

        let string_bytes = rest.get(..header.string_bytes as usize).ok_or(
            TestAnalyticsErrorKind::UnexpectedStringBytes {
                expected: header.string_bytes as usize,
                found: rest.len(),
            },
        )?;

        let flags_set = FlagsSet::load(string_bytes, flags_set)?;

        Ok(Self {
            timestamp: timestamp.max(header.timestamp),

            header,

            flags_set,
            commithashes_bytes,
            string_bytes,

            tests,
            testdata,
        })
    }

    /// Iterates over the [`Test`]s included in the [`TestAnalytics`] summary.
    pub fn tests(
        &self,
        desired_range: Range<usize>,
        flags: Option<&[&str]>,
    ) -> Result<
        impl Iterator<Item = Result<Test<'data, '_>, TestAnalyticsError>> + '_,
        TestAnalyticsError,
    > {
        let matching_flags_sets = if let Some(flags) = flags {
            let flag_sets = self.flags_set.iter(self.string_bytes);

            let mut matching_flags_sets: SmallVec<u32, 4> = Default::default();
            for res in flag_sets {
                let (offset, flag_set) = res?;
                if flags.iter().any(|flag| flag_set.contains(flag)) {
                    matching_flags_sets.push(offset);
                }
            }
            matching_flags_sets.sort();

            Some(matching_flags_sets)
        } else {
            None
        };
        let mut failing_commits = HashSet::new();

        let num_days = self.header.num_days as usize;
        let tests = self.tests.iter().enumerate().filter_map(move |(i, test)| {
            if let Some(flags_sets) = &matching_flags_sets {
                if !flags_sets.contains(&test.flag_set_offset) {
                    return None;
                }
            }

            let start_idx = i * num_days;
            let latest_test_timestamp = self.testdata[start_idx].last_timestamp;

            let today_offset = offset_from_today(latest_test_timestamp, self.timestamp);
            let data_range = start_idx..start_idx + test.valid_data as usize;
            let adjusted_range =
                adjust_selection_range(data_range, desired_range.clone(), today_offset);

            if adjusted_range.is_empty() {
                return None;
            }

            let aggregates = Aggregates::from_data(
                self.commithashes_bytes,
                &mut failing_commits,
                &self.testdata[adjusted_range],
            );

            Some(aggregates.map(|aggregates| Test {
                container: self,
                data: test,
                aggregates,
            }))
        });
        Ok(tests)
    }
}

impl fmt::Debug for TestAnalytics<'_> {
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
    container: &'parsed TestAnalytics<'data>,
    data: &'data raw::Test,

    aggregates: Aggregates,
}

impl<'data> Test<'data, '_> {
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

    pub fn flags(&self) -> Result<SmallVec<&str, 4>, TestAnalyticsError> {
        self.container
            .flags_set
            .resolve(self.container.string_bytes, self.data.flag_set_offset)
    }

    /// Returns the calculated aggregates.
    pub fn aggregates(&self) -> &Aggregates {
        &self.aggregates
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

    pub failing_commits: usize,
}

impl Aggregates {
    fn from_data(
        commithashes_bytes: &[u8],
        all_failing_commits: &mut HashSet<CommitHash>,
        data: &[raw::TestData],
    ) -> Result<Self, TestAnalyticsError> {
        let mut total_pass_count = 0;
        let mut total_fail_count = 0;
        let mut total_skip_count = 0;
        let mut total_flaky_fail_count = 0;
        let mut total_duration = 0.;

        for testdata in data {
            total_pass_count += testdata.total_pass_count as u32;
            total_fail_count += testdata.total_fail_count as u32;
            total_skip_count += testdata.total_skip_count as u32;
            total_flaky_fail_count += testdata.total_flaky_fail_count as u32;
            total_duration += testdata.total_duration as f64;

            let failing_commits =
                CommitHashesSet::read_raw(commithashes_bytes, testdata.failing_commits_set)?;
            all_failing_commits.extend(failing_commits);
        }

        let failing_commits = all_failing_commits.len();
        all_failing_commits.clear();

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

        Ok(Aggregates {
            total_pass_count,
            total_fail_count,
            total_skip_count,
            total_flaky_fail_count,

            failure_rate,
            flake_rate,

            avg_duration,

            failing_commits,
        })
    }
}

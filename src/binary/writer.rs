use std::io::Write;
use std::mem;

use indexmap::IndexSet;
use raw::TestData;
use timestamps::{adjust_selection_range, offset_from_today, shift_data};
use watto::{Pod, StringTable};

use crate::testrun;

use super::*;

/// The [`TestAnalytics`] File Writer.
#[derive(Debug)]
pub struct TestAnalyticsWriter {
    timestamp: u32,
    num_days: usize,
    tests: IndexSet<raw::Test>,
    testdata: Vec<raw::TestData>,
    string_table: StringTable,
}

impl TestAnalyticsWriter {
    /// Creates a new Writer.
    pub fn new(num_days: usize, timestamp: u32) -> Self {
        Self {
            timestamp,
            num_days,
            tests: IndexSet::new(),
            testdata: vec![],
            string_table: Default::default(),
        }
    }

    /// Turns an existing parsed [`TestAnalytics`] file into a writer.
    pub fn from_existing_format(
        data: &TestAnalytics,
        timestamp: u32,
    ) -> Result<Self, TestAnalyticsError> {
        let tests = IndexSet::from_iter(data.tests.iter().cloned());

        let string_table = StringTable::from_bytes(data.string_bytes)
            .map_err(|_| TestAnalyticsErrorKind::InvalidStringReference)?;

        Ok(Self {
            timestamp: timestamp.max(data.timestamp),
            num_days: data.header.num_days as usize,
            tests,
            testdata: data.testdata.into(),
            string_table,
        })
    }

    /// Merges the two parsed [`TestAnalytics`] into a writer.
    pub fn merge(
        a: &TestAnalytics,
        b: &TestAnalytics,
        timestamp: u32,
    ) -> Result<Self, TestAnalyticsError> {
        // merging the smaller into the larger is usually the more performant thing to do:
        let (larger, smaller) =
            if (b.header.num_days, b.header.num_tests) > (a.header.num_tests, a.header.num_tests) {
                (b, a)
            } else {
                (a, b)
            };
        let timestamp = timestamp.max(a.timestamp).max(b.timestamp);

        let mut writer = Self::from_existing_format(larger, timestamp)?;

        // we just assume a 75% overlap, or 25% new unique entries:
        let expected_new = smaller.header.num_tests as usize / 4;
        writer.tests.reserve(expected_new);
        let expected_reserve = expected_new * writer.num_days;
        writer.testdata.reserve(expected_reserve);

        for (smaller_idx, test) in smaller.tests.iter().enumerate() {
            let testsuite = StringTable::read(smaller.string_bytes, test.testsuite_offset as usize)
                .map_err(|_| TestAnalyticsErrorKind::InvalidStringReference)?;
            let name = StringTable::read(smaller.string_bytes, test.name_offset as usize)
                .map_err(|_| TestAnalyticsErrorKind::InvalidStringReference)?;

            let testsuite_offset = writer.string_table.insert(testsuite) as u32;
            let name_offset = writer.string_table.insert(name) as u32;
            let (idx, inserted) = writer.tests.insert_full(raw::Test {
                testsuite_offset,
                name_offset,
            });

            let data_idx = idx * writer.num_days;
            let smaller_idx = smaller_idx * smaller.header.num_days as usize;
            let smaller_timestamp = smaller.testdata[smaller_idx].last_timestamp;

            let larger_timestamp = if inserted {
                let expected_size = writer.tests.len() * writer.num_days;
                writer
                    .testdata
                    .resize_with(expected_size, TestData::default);

                smaller_timestamp
            } else {
                writer.testdata[data_idx].last_timestamp
            };

            let (smaller_range, today_offset) = if smaller_timestamp > larger_timestamp {
                // smaller has more recent data buckets, so we shift things around:
                let today_offset = offset_from_today(larger_timestamp, smaller_timestamp);
                let range = data_idx..data_idx + writer.num_days;

                shift_data(&mut writer.testdata[range], today_offset);

                let smaller_range = adjust_selection_range(
                    smaller_idx..smaller_idx + smaller.header.num_days as usize,
                    0..writer.num_days,
                    today_offset,
                );
                (smaller_range, 0)
            } else {
                let today_offset = offset_from_today(smaller_timestamp, larger_timestamp);
                let smaller_range = adjust_selection_range(
                    smaller_idx..smaller_idx + smaller.header.num_days as usize,
                    0..writer.num_days,
                    today_offset,
                );

                (smaller_range, today_offset)
            };

            let overlap_len = smaller_range.end - smaller_range.start;
            let idx_start = data_idx + today_offset;
            let larger_range = idx_start..idx_start + overlap_len;

            let larger_data = &mut writer.testdata[larger_range];
            let smaller_data = &smaller.testdata[smaller_range];

            for (larger, smaller) in larger_data.iter_mut().zip(smaller_data) {
                larger.total_pass_count += smaller.total_pass_count;
                larger.total_fail_count += smaller.total_fail_count;
                larger.total_skip_count += smaller.total_skip_count;
                larger.total_flaky_fail_count += smaller.total_flaky_fail_count;
                larger.total_duration += smaller.total_duration;

                if smaller.last_timestamp >= larger.last_timestamp {
                    larger.last_timestamp = smaller.last_timestamp;
                    larger.last_duration = smaller.last_duration;
                }
            }
        }

        Ok(writer)
    }

    /// Does garbage collection by rewriting test records and throwing away those with expired data.
    ///
    /// This also makes sure that the data records are being truncated or extended to `num_days`.
    /// In case no `num_days` adjustment is necessary, this will only rewrite all records when the number of expired records
    /// exceeds `threshold`, which defaults to 25% of the records.
    pub fn rewrite(
        &mut self,
        mut num_days: usize,
        garbage_threshold: Option<usize>,
    ) -> Result<bool, TestAnalyticsError> {
        let needs_resize = num_days != self.num_days;
        let threshold = garbage_threshold.unwrap_or(self.tests.len() / 4);
        let record_liveness: Vec<_> = (0..self.tests.len())
            .map(|idx| {
                let data_idx = idx * self.num_days;
                let test_timestamp = self.testdata[data_idx].last_timestamp;
                let today_offset = offset_from_today(test_timestamp, self.timestamp);
                today_offset < num_days
            })
            .collect();

        let live_records = record_liveness.iter().filter(|live| **live).count();
        let dead_records = self.tests.len() - live_records;

        if !(needs_resize || dead_records > threshold) {
            return Ok(false);
        }

        mem::swap(&mut num_days, &mut self.num_days);
        let string_table = mem::take(&mut self.string_table);
        let tests = mem::take(&mut self.tests);
        let testdata = mem::take(&mut self.testdata);

        let expected_size = live_records * self.num_days;
        self.tests.reserve(live_records);
        self.testdata.reserve(expected_size);

        for ((old_idx, test), record_live) in tests.iter().enumerate().zip(record_liveness) {
            if !record_live {
                continue;
            }

            let testsuite =
                StringTable::read(string_table.as_bytes(), test.testsuite_offset as usize)
                    .map_err(|_| TestAnalyticsErrorKind::InvalidStringReference)?;
            let name = StringTable::read(string_table.as_bytes(), test.name_offset as usize)
                .map_err(|_| TestAnalyticsErrorKind::InvalidStringReference)?;

            let testsuite_offset = self.string_table.insert(testsuite) as u32;
            let name_offset = self.string_table.insert(name) as u32;
            let (_new_idx, inserted) = self.tests.insert_full(raw::Test {
                testsuite_offset,
                name_offset,
            });
            assert!(inserted); // the records are already unique, and we re-insert those

            let overlap_days = num_days.min(self.num_days);
            let old_idx = old_idx * num_days;

            let old_range = old_idx..old_idx + overlap_days;
            self.testdata
                .extend_from_slice(&testdata[old_range.clone()]);

            let expected_size = self.tests.len() * self.num_days;
            self.testdata.resize_with(expected_size, TestData::default);
        }

        Ok(true)
    }

    /// Writes the data for the given [`Testrun`](testrun::Testrun) into this aggregation.
    pub fn add_test_run(&mut self, test: &testrun::Testrun) {
        let testsuite_offset = self.string_table.insert(&test.testsuite) as u32;
        let name_offset = self.string_table.insert(&test.name) as u32;
        let (idx, inserted) = self.tests.insert_full(raw::Test {
            testsuite_offset,
            name_offset,
        });

        let data_idx = idx * self.num_days;
        if inserted {
            let expected_size = self.tests.len() * self.num_days;
            self.testdata.resize_with(expected_size, TestData::default);
        } else {
            let range = data_idx..data_idx + self.num_days;
            let test_timestamp = self.testdata[data_idx].last_timestamp;
            let today_offset = offset_from_today(test_timestamp, self.timestamp);
            shift_data(&mut self.testdata[range.clone()], today_offset);
        }

        let testdata = &mut self.testdata[data_idx];
        testdata.total_duration += test.duration as f32;

        if testdata.last_timestamp <= self.timestamp {
            testdata.last_timestamp = self.timestamp;
            testdata.last_duration = test.duration as f32;
        }

        match test.outcome {
            testrun::Outcome::Pass => testdata.total_pass_count += 1,
            testrun::Outcome::Error | testrun::Outcome::Failure => testdata.total_fail_count += 1,
            testrun::Outcome::Skip => testdata.total_skip_count += 1,
        }
    }

    /// Serialize the converted data.
    ///
    /// This writes the [`TestAnalytics`] binary format into the given [`Write`].
    pub fn serialize<W: Write>(self, writer: &mut W) -> std::io::Result<()> {
        let mut writer = watto::Writer::new(writer);

        let string_bytes = self.string_table.into_bytes();

        let header = raw::Header {
            magic: raw::TA_MAGIC,
            version: super::format::TA_VERSION,
            timestamp: self.timestamp,

            num_days: self.num_days as u32,
            num_tests: self.tests.len() as u32,

            string_bytes: string_bytes.len() as u32,
        };

        writer.write_all(header.as_bytes())?;
        writer.align_to(8)?;

        for test in self.tests.into_iter() {
            writer.write_all(test.as_bytes())?;
        }
        writer.align_to(8)?;

        writer.write_all(self.testdata.as_bytes())?;
        writer.align_to(8)?;

        writer.write_all(&string_bytes)?;

        Ok(())
    }
}

use std::io::Write;
use std::mem;
use std::ops::AddAssign;

use indexmap::IndexSet;
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

    total_pass_count: Vec<u16>,
    total_fail_count: Vec<u16>,
    total_skip_count: Vec<u16>,
    total_flaky_fail_count: Vec<u16>,
    total_duration: Vec<f32>,

    last_timestamp: Vec<u32>,
    last_duration: Vec<f32>,

    string_table: StringTable,
}

impl TestAnalyticsWriter {
    /// Creates a new Writer.
    pub fn new(num_days: usize, timestamp: u32) -> Self {
        Self {
            timestamp,
            num_days,
            tests: IndexSet::new(),

            total_pass_count: vec![],
            total_fail_count: vec![],
            total_skip_count: vec![],
            total_flaky_fail_count: vec![],
            total_duration: vec![],

            last_timestamp: vec![],
            last_duration: vec![],

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
            timestamp,
            num_days: data.header.num_days as usize,
            tests,
            total_pass_count: data.total_pass_count.into(),
            total_fail_count: data.total_fail_count.into(),
            total_skip_count: data.total_skip_count.into(),
            total_flaky_fail_count: data.total_flaky_fail_count.into(),
            total_duration: data.total_duration.into(),
            last_timestamp: data.last_timestamp.into(),
            last_duration: data.last_duration.into(),
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

        let mut writer = Self::from_existing_format(larger, timestamp)?;

        // we just assume a 75% overlap, or 25% new unique entries:
        let expected_new = smaller.header.num_tests as usize / 4;
        writer.tests.reserve(expected_new);
        let expected_reserve = expected_new * writer.num_days;
        writer.total_pass_count.reserve(expected_reserve);
        writer.total_fail_count.reserve(expected_reserve);
        writer.total_skip_count.reserve(expected_reserve);
        writer.total_flaky_fail_count.reserve(expected_reserve);
        writer.total_duration.reserve(expected_reserve);

        writer.last_timestamp.reserve(expected_reserve);
        writer.last_duration.reserve(expected_reserve);

        for (smaller_idx, test) in smaller.tests.iter().enumerate() {
            let name = StringTable::read(smaller.string_bytes, test.name_offset as usize)
                .map_err(|_| TestAnalyticsErrorKind::InvalidStringReference)?;

            let name_offset = writer.string_table.insert(name) as u32;
            let (idx, inserted) = writer.tests.insert_full(raw::Test { name_offset });

            let data_idx = idx * writer.num_days;
            let smaller_idx = smaller_idx * smaller.header.num_days as usize;
            let smaller_timestamp = smaller.last_timestamp[smaller_idx];

            let last_timestamp = if inserted {
                let expected_size = writer.tests.len() * writer.num_days;
                writer.total_pass_count.resize(expected_size, 0);
                writer.total_fail_count.resize(expected_size, 0);
                writer.total_skip_count.resize(expected_size, 0);
                writer.total_flaky_fail_count.resize(expected_size, 0);
                writer.total_duration.resize(expected_size, 0.);

                writer.last_timestamp.resize(expected_size, 0);
                writer.last_duration.resize(expected_size, 0.);

                smaller_timestamp
            } else {
                writer.last_timestamp[data_idx]
            };

            let today_offset = offset_from_today(last_timestamp, smaller_timestamp);
            let smaller_range = adjust_selection_range(
                smaller_idx..smaller_idx + smaller.header.num_days as usize,
                0..writer.num_days,
                -today_offset.abs(),
            );
            let overlap_len = smaller_range.end - smaller_range.start;
            // smaller has more recent data buckets, so we shift things around:
            let larger_range = if today_offset < 0 {
                let range = data_idx..data_idx + writer.num_days;
                shift_data(&mut writer.total_pass_count[range.clone()], today_offset);
                shift_data(&mut writer.total_fail_count[range.clone()], today_offset);
                shift_data(&mut writer.total_skip_count[range.clone()], today_offset);
                shift_data(
                    &mut writer.total_flaky_fail_count[range.clone()],
                    today_offset,
                );
                shift_data(&mut writer.total_duration[range.clone()], today_offset);
                shift_data(&mut writer.last_timestamp[range.clone()], today_offset);
                shift_data(&mut writer.last_duration[range.clone()], today_offset);

                data_idx..data_idx + overlap_len
            } else {
                let idx_start = data_idx + today_offset as usize;
                idx_start..idx_start + overlap_len
            };

            add_assign_slice(
                &mut writer.total_pass_count[larger_range.clone()],
                &smaller.total_pass_count[smaller_range.clone()],
            );
            add_assign_slice(
                &mut writer.total_fail_count[larger_range.clone()],
                &smaller.total_fail_count[smaller_range.clone()],
            );
            add_assign_slice(
                &mut writer.total_skip_count[larger_range.clone()],
                &smaller.total_skip_count[smaller_range.clone()],
            );
            add_assign_slice(
                &mut writer.total_flaky_fail_count[larger_range.clone()],
                &smaller.total_flaky_fail_count[smaller_range.clone()],
            );
            add_assign_slice(
                &mut writer.total_duration[larger_range.clone()],
                &smaller.total_duration[smaller_range.clone()],
            );

            let larger_last_timestamp = &mut writer.last_timestamp[larger_range.clone()]; // llt
            let larger_last_duration = &mut writer.last_duration[larger_range.clone()]; // lld
            let smaller_last_timestamp = &smaller.last_timestamp[smaller_range.clone()]; // slt
            let smaller_last_duration = &smaller.last_duration[smaller_range.clone()]; // sld
            let iter = larger_last_timestamp
                .iter_mut()
                .zip(larger_last_duration.iter_mut())
                .zip(smaller_last_timestamp)
                .zip(smaller_last_duration);
            for (((llt, lld), slt), sld) in iter {
                if *llt <= *slt {
                    *llt = *slt;
                    *lld = *sld;
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
                let today_offset = offset_from_today(self.last_timestamp[data_idx], self.timestamp);
                today_offset >= 0 || (-today_offset as usize) < num_days
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
        let total_pass_count = mem::take(&mut self.total_pass_count);
        let total_fail_count = mem::take(&mut self.total_fail_count);
        let total_skip_count = mem::take(&mut self.total_skip_count);
        let total_flaky_fail_count = mem::take(&mut self.total_flaky_fail_count);
        let total_duration = mem::take(&mut self.total_duration);
        let last_timestamp = mem::take(&mut self.last_timestamp);
        let last_duration = mem::take(&mut self.last_duration);

        let expected_size = live_records * self.num_days;
        self.tests.reserve(live_records);
        self.total_pass_count.reserve(expected_size);
        self.total_fail_count.reserve(expected_size);
        self.total_skip_count.reserve(expected_size);
        self.total_flaky_fail_count.reserve(expected_size);
        self.total_duration.reserve(expected_size);
        self.last_timestamp.reserve(expected_size);
        self.last_duration.reserve(expected_size);

        for ((old_idx, test), record_live) in tests.iter().enumerate().zip(record_liveness) {
            if !record_live {
                continue;
            }
            let name = StringTable::read(string_table.as_bytes(), test.name_offset as usize)
                .map_err(|_| TestAnalyticsErrorKind::InvalidStringReference)?;

            let name_offset = self.string_table.insert(name) as u32;
            let (_new_idx, inserted) = self.tests.insert_full(raw::Test { name_offset });
            assert!(inserted); // the records are already unique, and we re-insert those

            let overlap_days = num_days.min(self.num_days);
            let old_idx = old_idx * num_days;

            let old_range = old_idx..old_idx + overlap_days;
            self.total_pass_count
                .extend_from_slice(&total_pass_count[old_range.clone()]);
            self.total_fail_count
                .extend_from_slice(&total_fail_count[old_range.clone()]);
            self.total_skip_count
                .extend_from_slice(&total_skip_count[old_range.clone()]);
            self.total_flaky_fail_count
                .extend_from_slice(&total_flaky_fail_count[old_range.clone()]);
            self.total_duration
                .extend_from_slice(&total_duration[old_range.clone()]);
            self.last_timestamp
                .extend_from_slice(&last_timestamp[old_range.clone()]);
            self.last_duration
                .extend_from_slice(&last_duration[old_range.clone()]);

            let expected_size = self.tests.len() * self.num_days;
            self.total_pass_count.resize(expected_size, 0);
            self.total_fail_count.resize(expected_size, 0);
            self.total_skip_count.resize(expected_size, 0);
            self.total_flaky_fail_count.resize(expected_size, 0);
            self.total_duration.resize(expected_size, 0.);
            self.last_timestamp.resize(expected_size, 0);
            self.last_duration.resize(expected_size, 0.);
        }

        Ok(true)
    }

    /// Writes the data for the given [`Testrun`](testrun::Testrun) into this aggregation.
    pub fn add_test_run(&mut self, test: &testrun::Testrun) {
        let name_offset = self.string_table.insert(&test.name) as u32;
        let (idx, inserted) = self.tests.insert_full(raw::Test { name_offset });

        let data_idx = idx * self.num_days;
        if inserted {
            let expected_size = self.tests.len() * self.num_days;
            self.total_pass_count.resize(expected_size, 0);
            self.total_fail_count.resize(expected_size, 0);
            self.total_skip_count.resize(expected_size, 0);
            self.total_flaky_fail_count.resize(expected_size, 0);
            self.total_duration.resize(expected_size, 0.);

            self.last_timestamp.resize(expected_size, 0);
            self.last_duration.resize(expected_size, 0.);
        } else {
            let range = data_idx..data_idx + self.num_days;
            let today_offset = offset_from_today(self.last_timestamp[data_idx], self.timestamp);
            shift_data(&mut self.total_pass_count[range.clone()], today_offset);
            shift_data(&mut self.total_fail_count[range.clone()], today_offset);
            shift_data(&mut self.total_skip_count[range.clone()], today_offset);
            shift_data(
                &mut self.total_flaky_fail_count[range.clone()],
                today_offset,
            );
            shift_data(&mut self.total_duration[range.clone()], today_offset);
            shift_data(&mut self.last_timestamp[range.clone()], today_offset);
            shift_data(&mut self.last_duration[range.clone()], today_offset);
        }

        self.total_duration[data_idx] += test.duration as f32;

        if self.last_timestamp[data_idx] <= self.timestamp {
            self.last_timestamp[data_idx] = self.timestamp;
            self.last_duration[data_idx] = test.duration as f32;
        }

        match test.outcome {
            testrun::Outcome::Pass => self.total_pass_count[data_idx] += 1,
            testrun::Outcome::Error | testrun::Outcome::Failure => {
                self.total_fail_count[data_idx] += 1
            }
            testrun::Outcome::Skip => self.total_skip_count[data_idx] += 1,
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

        writer.write_all(self.total_pass_count.as_bytes())?;
        writer.align_to(8)?;

        writer.write_all(self.total_fail_count.as_bytes())?;
        writer.align_to(8)?;

        writer.write_all(self.total_skip_count.as_bytes())?;
        writer.align_to(8)?;

        writer.write_all(self.total_flaky_fail_count.as_bytes())?;
        writer.align_to(8)?;

        writer.write_all(self.total_duration.as_bytes())?;
        writer.align_to(8)?;

        writer.write_all(self.last_timestamp.as_bytes())?;
        writer.align_to(8)?;

        writer.write_all(self.last_duration.as_bytes())?;
        writer.align_to(8)?;

        writer.write_all(&string_bytes)?;

        Ok(())
    }
}

fn add_assign_slice<'a, T>(a: &'a mut [T], b: &'a [T])
where
    T: AddAssign<&'a T> + 'a,
{
    for (a, b) in a.iter_mut().zip(b) {
        *a += b;
    }
}

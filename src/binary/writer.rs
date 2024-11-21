use std::io::Write;

use indexmap::IndexSet;
use timestamps::{offset_from_today, shift_data};
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

    /// Turns an existing parsed [`TestAnalytics`] file into a mutable writer.
    pub fn from_existing_format(
        data: &TestAnalytics,
        timestamp: u32,
    ) -> Result<Self, TestAnalyticsError> {
        let tests = IndexSet::from_iter(data.tests.iter().cloned());

        // TODO: I should really move this to `watto`
        let mut string_table = StringTable::new();
        let mut next_offset = 0;
        while next_offset < data.string_bytes.len() {
            let string = StringTable::read(data.string_bytes, next_offset)
                .map_err(|_| TestAnalyticsErrorKind::InvalidStringReference)?;
            string_table.insert(string);
            // TODO: this should really be `subslice_range` which is currently nightly-only
            next_offset = unsafe { string.as_ptr().byte_offset_from(data.string_bytes.as_ptr()) }
                as usize
                + string.len();
        }

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
            let today_offset = offset_from_today(self.last_timestamp[data_idx], self.timestamp);
            let range = data_idx..data_idx + self.num_days;
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

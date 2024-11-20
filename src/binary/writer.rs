use std::io::Write;

use indexmap::IndexSet;
use watto::{Pod, StringTable};

use crate::testrun;

use super::*;

/// The [`TestAnalytics`] File Writer.
#[derive(Debug)]
pub struct TestAnalyticsWriter {
    num_days: u32,

    tests: IndexSet<raw::Test>,

    total_pass_count: Vec<u16>,
    total_fail_count: Vec<u16>,
    total_skip_count: Vec<u16>,
    total_flaky_fail_count: Vec<u16>,
    total_duration: Vec<f32>,

    string_table: StringTable,
}

impl TestAnalyticsWriter {
    /// Creates a new Writer.
    pub fn new(num_days: u32) -> Self {
        Self {
            num_days,
            tests: IndexSet::new(),

            total_pass_count: vec![],
            total_fail_count: vec![],
            total_skip_count: vec![],
            total_flaky_fail_count: vec![],
            total_duration: vec![],

            string_table: Default::default(),
        }
    }

    pub fn add_test_run(&mut self, test: &testrun::Testrun) {
        let name_offset = self.string_table.insert(&test.name) as u32;
        let (idx, inserted) = self.tests.insert_full(raw::Test { name_offset });

        if inserted {
            let expected_size = self.tests.len() * self.num_days as usize;
            self.total_pass_count.resize(expected_size, 0);
            self.total_fail_count.resize(expected_size, 0);
            self.total_skip_count.resize(expected_size, 0);
            self.total_flaky_fail_count.resize(expected_size, 0);
            self.total_duration.resize(expected_size, 0.);
        }

        let data_idx = idx * self.num_days as usize;
        self.total_duration[data_idx] += test.duration as f32;
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

            num_days: self.num_days,
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

        writer.write_all(&string_bytes)?;

        Ok(())
    }
}

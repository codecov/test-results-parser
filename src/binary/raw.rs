use watto::Pod;

/// The magic file preamble, encoded as little-endian `CCTA`.
pub const TA_MAGIC: u32 = u32::from_le_bytes(*b"CCTA");

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct Header {
    /// The file magic representing the file format and endianness.
    pub magic: u32,
    /// The file format version.
    pub version: u32,
    /// Timestamp when the file was last touched.
    pub timestamp: u32,
    /// Number of tests within the file.
    pub num_tests: u32,
    /// Number of days worth of aggregated data.
    pub num_days: u32,
    /// Length of the `FlagsSet` table.
    pub flags_set_len: u32,
    /// Length of the `CommitHashesSet` table.
    pub commithashes_bytes: u32,
    /// Length of the string table.
    pub string_bytes: u32,
}
unsafe impl Pod for Header {}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Test {
    /// Offset of the Testsuite name within the string table.
    pub testsuite_offset: u32,
    /// Offset of the Test name within the string table.
    pub name_offset: u32,
    /// Offset of the Flag Set within the `FlagsSet` table.
    pub flag_set_offset: u32,
    /// The number of valid data entries.
    pub valid_data: u32,
}
unsafe impl Pod for Test {}

#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct TestData {
    pub last_timestamp: u32,
    pub last_duration: f32,

    pub total_pass_count: u16,
    pub total_fail_count: u16,
    pub total_skip_count: u16,
    pub total_flaky_fail_count: u16,
    pub total_duration: f32,

    pub failing_commits_set: u32,
}
unsafe impl Pod for TestData {}

#[derive(Debug, Clone, Copy, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[repr(C)]
pub struct CommitHash(pub [u8; 20]);
unsafe impl Pod for CommitHash {}

#[cfg(test)]
mod tests {
    use std::mem;

    use super::*;

    #[test]
    fn test_sizeof() {
        assert_eq!(mem::size_of::<Header>(), 32);
        assert_eq!(mem::align_of::<Header>(), 4);

        assert_eq!(mem::size_of::<Test>(), 16);
        assert_eq!(mem::align_of::<Test>(), 4);

        assert_eq!(mem::size_of::<TestData>(), 24);
        assert_eq!(mem::align_of::<TestData>(), 4);

        assert_eq!(mem::size_of::<CommitHash>(), 20);
        assert_eq!(mem::align_of::<CommitHash>(), 1);
    }
}

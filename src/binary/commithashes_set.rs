use watto::OffsetSet;

use super::*;

#[derive(Debug)]
pub struct CommitHashesSet {
    inner: OffsetSet<raw::CommitHash>,
    temp_hashes: Vec<raw::CommitHash>,
}

impl CommitHashesSet {
    pub fn new() -> Self {
        let mut inner = OffsetSet::<raw::CommitHash>::default();
        // NOTE: this reserves offset `0` for the empty set
        inner.insert(&[]);

        Self {
            inner,
            temp_hashes: Default::default(),
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TestAnalyticsError> {
        let inner = OffsetSet::<raw::CommitHash>::from_bytes(bytes)
            .map_err(|_| TestAnalyticsErrorKind::InvalidCommitSetReference)?;
        Ok(Self {
            inner,
            temp_hashes: Default::default(),
        })
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.inner.into_bytes()
    }

    pub fn read_raw(bytes: &[u8], offset: u32) -> Result<&[raw::CommitHash], TestAnalyticsError> {
        if offset == 0 {
            return Ok(&[]);
        }
        Ok(OffsetSet::<raw::CommitHash>::read(bytes, offset as usize)
            .map_err(|_| TestAnalyticsErrorKind::InvalidCommitSetReference)?)
    }

    pub fn read(&self, offset: u32) -> &[raw::CommitHash] {
        Self::read_raw(self.inner.as_bytes(), offset).unwrap()
    }

    /// Appends the `commit_hashes` to the existing set referenced by `existing_offset`.
    ///
    /// This returns a new offset in case any new commit hash was added, or returns
    /// the `existing_offset` unmodified in case the existing set already includes
    /// all the `commit_hashes`.
    pub fn append_intersection(
        &mut self,
        existing_offset: u32,
        commit_hashes: &[raw::CommitHash],
    ) -> u32 {
        let existing_hashes =
            OffsetSet::<raw::CommitHash>::read(self.inner.as_bytes(), existing_offset as usize)
                .unwrap();

        self.temp_hashes.extend_from_slice(existing_hashes);
        self.temp_hashes.extend_from_slice(commit_hashes);
        self.temp_hashes.sort();
        self.temp_hashes.dedup();

        let offset = self.inner.insert(&self.temp_hashes);
        self.temp_hashes.clear();

        offset as u32
    }
}

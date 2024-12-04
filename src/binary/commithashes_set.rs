use super::TestAnalyticsError;

#[derive(Debug, Default)]
pub struct CommitHashesSet {
    buffer: Vec<u8>,
}

impl CommitHashesSet {
    pub fn from_bytes(buffer: &[u8]) -> Result<Self, TestAnalyticsError> {
        // TODO: TestAnalyticsErrorKind::InvalidCommitSetReference
        Ok(Self {
            buffer: buffer.into(),
        })
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buffer
    }
}

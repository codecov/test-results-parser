use thiserror::Error;

#[cfg(doc)]
use super::*;

/// The kind of a [`TestAnalyticsError`].
#[derive(Debug, Clone, Copy, Error)]
#[non_exhaustive]
pub enum TestAnalyticsErrorKind {
    /// The file header could not be read.
    #[error("could not read header")]
    InvalidHeader,
    /// The cache file header does not contain the correct magic bytes.
    #[error("invalid magic: {0}")]
    InvalidMagic(u32),
    /// The cache file header contains an invalid version.
    #[error("wrong version: {0}")]
    WrongVersion(u32),
    /// One of the tables could not be read from the file.
    #[error("could not read data tables")]
    InvalidTables,
    /// The header claimed an incorrect number of string bytes.
    #[error("expected {expected} string bytes, found {found}")]
    UnexpectedStringBytes {
        /// Expected number of string bytes.
        expected: usize,
        /// Number of string bytes actually found in the cache file.
        found: usize,
    },
    /// The string reference was invalid
    #[error("could not resolve string reference")]
    InvalidStringReference,
    /// The flag set reference was invalid
    #[error("could not resolve flag set reference")]
    InvalidFlagSetReference,
    /// The commit set reference was invalid
    #[error("could not resolve commit set reference")]
    InvalidCommitSetReference,
}

/// An error encountered during [`TestAnalytics`] creation or parsing.
#[derive(Debug, Error)]
#[error("{kind}")]
pub struct TestAnalyticsError {
    pub(crate) kind: TestAnalyticsErrorKind,
    #[source]
    pub(crate) source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

impl TestAnalyticsError {
    /// Returns the corresponding [`TestAnalyticsErrorKind`] for this error.
    pub fn kind(&self) -> TestAnalyticsErrorKind {
        self.kind
    }
}

impl From<TestAnalyticsErrorKind> for TestAnalyticsError {
    fn from(kind: TestAnalyticsErrorKind) -> Self {
        Self { kind, source: None }
    }
}

use std::ops::Deref;

use anyhow::Result;
use pyo3::{FromPyObject, IntoPyObject};
use serde::{Deserialize, Serialize};
use thiserror::Error;
// String that is validated to be less than 1000 characters

#[derive(
    Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize, FromPyObject, IntoPyObject,
)]
#[serde(transparent)]
#[pyo3(transparent)]
pub struct ValidatedString {
    value: String,
}

#[derive(Error, Debug)]
pub enum ValidatedStringError {
    #[error("String is too long")]
    StringTooLong(String),
}

impl ValidatedString {
    pub fn from_string(value: String) -> Result<Self, ValidatedStringError> {
        if value.len() > 1000 {
            return Err(ValidatedStringError::StringTooLong(value));
        }
        Ok(Self { value })
    }
}

impl Deref for ValidatedString {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl TryFrom<String> for ValidatedString {
    type Error = ValidatedStringError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_string(value)
    }
}

impl TryFrom<&str> for ValidatedString {
    type Error = ValidatedStringError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::from_string(value.to_string())
    }
}

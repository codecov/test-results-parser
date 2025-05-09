use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct WarningInfo {
    pub message: String,
    pub location: u64,
}

impl WarningInfo {
    pub fn new(message: String, location: u64) -> Self {
        Self { message, location }
    }
}

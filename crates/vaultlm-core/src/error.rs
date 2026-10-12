//! API-style errors: an HTTP-like status (so the UI's existing error handling keeps working),
//! a message, and optional extra JSON (e.g. the on-disk copy for a 409 save conflict).
use serde_json::Value;

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct ApiError {
    pub status: u16,
    pub message: String,
    pub extra: Option<Value>,
}

impl ApiError {
    pub fn new(status: u16, message: impl Into<String>) -> Self {
        Self { status, message: message.into(), extra: None }
    }
    pub fn bad_request(m: impl Into<String>) -> Self { Self::new(400, m) }
    pub fn forbidden(m: impl Into<String>) -> Self { Self::new(403, m) }
    pub fn not_found(m: impl Into<String>) -> Self { Self::new(404, m) }
    pub fn conflict(m: impl Into<String>) -> Self { Self::new(409, m) }
    pub fn with_extra(mut self, extra: Value) -> Self { self.extra = Some(extra); self }
}

impl From<std::io::Error> for ApiError {
    fn from(e: std::io::Error) -> Self {
        let status = match e.kind() {
            std::io::ErrorKind::NotFound => 404,
            std::io::ErrorKind::PermissionDenied => 403,
            _ => 500,
        };
        Self::new(status, e.to_string())
    }
}
impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> Self { Self::bad_request(e.to_string()) }
}
impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self { Self::new(500, e.to_string()) }
}

pub type Result<T> = std::result::Result<T, ApiError>;

//! Typed validation errors for user-supplied parameters (REAS-RUST-005).

use std::fmt;

/// A parameter that failed validation, with the field it belongs to.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub struct ValidationError {
    pub field: &'static str,
    pub message: String,
}

impl ValidationError {
    pub fn new(field: &'static str, message: impl Into<String>) -> Self {
        Self {
            field,
            message: message.into(),
        }
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

/// Shorthand for `Err(ValidationError::new(field, message))`.
pub fn invalid<T>(field: &'static str, message: impl Into<String>) -> Result<T, ValidationError> {
    Err(ValidationError::new(field, message))
}

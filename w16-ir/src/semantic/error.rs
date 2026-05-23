// w16-ir\src\semantic\error.rs
//
//! Ошибки semantic verifier.

use std::fmt;

/// Ошибка семантической проверки.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticError {
    /// Человеческое описание ошибки.
    pub message: String,
}

impl SemanticError {
    /// Создать semantic error с сообщением.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for SemanticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SemanticError {}

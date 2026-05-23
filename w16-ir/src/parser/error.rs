//! w16-ir/src/parser/error.rs
//!
//! Ошибки parser.
//!
//! Сейчас ошибка содержит только сообщение и span. Позже сюда можно добавить
//! expected/found токены, recoverable diagnostics и привязку к line/column.

use std::fmt;

use crate::lexer::Span;

/// Ошибка синтаксического анализа.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// Человеческое описание ошибки.
    pub message: String,
    /// Byte span проблемного токена.
    pub span: Span,
}

impl ParseError {
    /// Создать parser error с сообщением и span.
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at {}..{}",
            self.message, self.span.start, self.span.end
        )
    }
}

impl std::error::Error for ParseError {}

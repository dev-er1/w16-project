// example-langs\src\frontend\lexer\lexer_error.rs
//
//! # Ошибки лексического анализа
use crate::frontend::lexer::token::Span;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LexerErrors {
    UnexpectedChar(char),
    UnterminatedString,
    UnterminatedChar,
    UnterminatedBlockComment,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexerError {
    pub error: LexerErrors,
    pub pos: Span,
}

impl LexerError {
    pub fn new(error: LexerErrors, pos: Span) -> Self {
        Self { error, pos }
    }
}

impl fmt::Display for LexerErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedChar(c) => write!(f, "unexpected character '{}'", c.escape_debug()),
            Self::UnterminatedString => write!(f, "unterminated string literal"),
            Self::UnterminatedChar => write!(f, "unterminated character literal"),
            Self::UnterminatedBlockComment => write!(f, "unterminated block comment (/* ... */)"),
        }
    }
}

impl fmt::Display for LexerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.error)
    }
}
// w16-langs\w16-cc\src\frontend\preprocessor\error.rs
//
//! Ошибки препроцессора.

use std::fmt;

/// Вид ошибки препроцессора.
#[derive(Debug, Clone)]
pub enum PreprocErrorKind {
    /// Файл из `#include` не найден.
    IncludeNotFound(String),

    /// Ошибка чтения файла из `#include`.
    IncludeReadError { path: String, reason: String },

    /// Рекурсивный `#include` (цикл включения файлов).
    IncludeCycle(String),

    /// `#include` с некорректным синтаксисом.
    MalformedInclude(String),

    /// `#define` с некорректным синтаксисом.
    MalformedDefine(String),

    /// Вложенность `#if`/`#ifdef` превысила лимит.
    TooDeepNesting,
}

impl fmt::Display for PreprocErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IncludeNotFound(path) =>
                write!(f, "included file not found: `{path}`"),
            Self::IncludeReadError { path, reason } =>
                write!(f, "cannot read `{path}`: {reason}"),
            Self::IncludeCycle(path) =>
                write!(f, "include cycle detected: `{path}` is already being processed"),
            Self::MalformedInclude(line) =>
                write!(f, "malformed #include directive: `{line}`"),
            Self::MalformedDefine(line) =>
                write!(f, "malformed #define directive: `{line}`"),
            Self::TooDeepNesting =>
                write!(f, "#if/#ifdef nesting exceeds maximum depth (64)"),
        }
    }
}

/// Ошибка препроцессора с номером строки.
#[derive(Debug, Clone)]
pub struct PreprocError {
    /// Номер строки в исходном файле (1-based).
    pub line: usize,
    /// Путь к файлу в котором возникла ошибка.
    pub file: String,
    pub kind: PreprocErrorKind,
}

impl PreprocError {
    pub fn new(line: usize, file: impl Into<String>, kind: PreprocErrorKind) -> Self {
        Self { line, file: file.into(), kind }
    }
}

impl fmt::Display for PreprocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: preprocessor error: {}", self.file, self.line, self.kind)
    }
}
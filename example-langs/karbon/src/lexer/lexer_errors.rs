//! example-langs\karbon\src\lexer\lexer_errors.rs
//!
//! # Ошибки токенизации
//!
//! Ошибки токениазции кода, и красивый вывод
use crate::lexer::token::Span;

/// Ошибки
#[derive(Debug, Clone, PartialEq)]
pub enum LexerErrors {
    /// Неожиданный символ, который не входит в алфавит языка
    UnexpectedChar(char),

    /// Незакрытая кавычка.
    ///
    /// ### Пример
    /// ```text
    /// str var idk = "a
    /// a
    /// a
    /// a
    /// a
    ///  // А тут нету закрывающей '"'
    /// ```
    UnterminatedString(Span),

    /// Ошибка в формате числа (например, две точки в дробном числе)
    InvalidNumberFormat(String),

    /// Пустой системный атрибут (написали `@` но не указали имя)
    ///
    /// ### Пример
    /// ```text
    /// var hello = @; // Просто '@', нету имени атрибута
    /// ```
    EmptyAttribute,

    /// Неизвестный символ после амперсанда или звездочки (для памяти)
    InvalidMemoryOperator,

    /// Ошибка при чтении комментария (если будут многострочные)
    ///
    /// ### Пример
    /// ```text
    /// i8 var idk = 10; /* Эта переменная... // Тут не закрыт комментарий, не хватает закрывающей '*/'
    /// ```
    UnterminatedComment,
}

pub struct LexerError {
    pub span: Span,                 // Позиция в исходном коде
    pub kind_of_error: LexerErrors, // Тип ошибки
}

impl LexerError {
    #[inline]
    pub fn new(span: Span, kind_of_error: LexerErrors) -> Self {
        Self {
            span,
            kind_of_error,
        }
    }
    pub fn report_lexer_error(error: LexerError, source: &str) {
        let normal_error_message = match error.kind_of_error {
            LexerErrors::UnexpectedChar(ch) => format!("unexpected char: '{ch}'"),
            LexerErrors::UnterminatedString(_) => "unterminated string".to_string(),
            LexerErrors::InvalidNumberFormat(str) => format!("imvalid number format: '{str}'"),
            LexerErrors::EmptyAttribute => "empty attribute".to_string(),
            LexerErrors::InvalidMemoryOperator => "invalid memory operator".to_string(),
            LexerErrors::UnterminatedComment => "unterminated comment".to_string(),
        };
        let spaces = " ".repeat((error.span.start - 1) as usize); // Пробелы перед стрелочками
        let carets = "^".repeat((error.span.end - error.span.start).max(1) as usize); // Количество стрелочек '^'
        let line_content = source
            .lines()
            .nth((error.span.line - 1) as usize)
            .unwrap_or(""); // Строка где находится ошибка
        let gutter = " |".to_string();
        let padding = " ".repeat(gutter.len() - 1);
        println!("\x1b[31m\x1b[1mLexer error\x1b[0m: \x1b[1m{normal_error_message}\x1b[0m");
        println!(
            "\x1b[4m{:?}:{:?}\x1b[0m..\x1b[4m{:?}\x1b[0m (line:start..end)",
            error.span.line, error.span.start, error.span.end
        );
        println!("\x1b[96m{padding} {gutter}\x1b[0m");
        println!(
            "\x1b[96m{padding}{}{gutter}\x1b[0m {line_content}",
            error.span.line
        );
        println!("\x1b[96m{padding}\x1b[0m{spaces}\x1b[31m\x1b[1m{carets}\x1b[0m");
    }
}

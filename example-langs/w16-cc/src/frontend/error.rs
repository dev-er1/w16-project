// example-langs\w16-cc\src\frontend\error.rs
//
//! # Вывод ошибок и диагностика
//! 
//! Сюда приходят все ошибки, и диагностика, и всё выводится на экран.
use std::fmt;

use crate::frontend::{lexer::{lexer_error::LexerError, token::Span}, parser::ParseError, semantic::error::SemanticError};

#[derive(Debug)]
pub enum WhichError {
    LexerError(LexerError),
    ParserError(ParseError),
    SemanticError(SemanticError)
}

#[derive(Debug)]
pub struct Error {
    pub error: WhichError,
    pub msg: Option<String>,
    pub diagnostic: Option<String>
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.error {
            WhichError::LexerError(err) => write!(f, "{err}"),
            WhichError::ParserError(err) => write!(f, "{err}"),
            WhichError::SemanticError(err) => write!(f, "{err}"),
        }
    }
}

impl Error {
    /// Позиция
    pub fn span(&self) -> Span {
        match &self.error {
            WhichError::LexerError(err) => err.pos,
            WhichError::ParserError(err) => err.pos,
            WhichError::SemanticError(err) => err.span,
        }
    }

    /// Вывод человекочитаемой ошибки и диагностики.(идея взята из rustc и clang)
    pub fn report_error(&self, source_code: &str) {
        let (line_num, col_num) = self.span().start_line_and_col;
        
        // Главное сообщение об ошибке
        println!("\x1b[1;31merror\x1b[0m: \x1b[1m{self}\x1b[0m");
        println!("line and col: {line_num}:{col_num}");

        if line_num > 0 {
            if let Some(source_line) = source_code.lines().nth((line_num - 1) as usize) {
                let line_num_str = line_num.to_string();
                let padding = " ".repeat(line_num_str.len());
                
                // Пустой разделитель сверху
                println!(" {padding} \x1b[1;34m|\x1b[0m");
                
                // Строка кода с номером строки
                println!(" \x1b[1;34m{line_num}\x1b[0m \x1b[1;34m|\x1b[0m {source_line}");
                
                // Указатель на ошибку (стрелочки ^~~~)
                print!(" {padding} \x1b[1;34m|\x1b[0m ");
                if col_num > 0 {
                    print!("{}", " ".repeat((col_num - 1) as usize));
                }
                println!("\x1b[1;31m^———\x1b[0m");
            }
        }

        // Дополнительные заметки
        if let Some(custom_msg) = &self.msg {
            println!("  \x1b[1;36mnote\x1b[0m: {custom_msg}");
        }
        if let Some(diag) = &self.diagnostic {
            println!("  \x1b[1;32mhelp\x1b[0m: {diag}");
        }
    }

    /// Получить [`Error`] из [`LexerError`]
    pub fn from_lexer(err: LexerError) -> Self {
        Self {
            error: WhichError::LexerError(err),
            msg: None,
            diagnostic: None,
        }
    }

    /// Получить [`Error`] из [`ParseError`]
    pub fn from_parser(err: ParseError) -> Self {
        Self {
            error: WhichError::ParserError(err),
            msg: None,
            diagnostic: None,
        }
    }

    /// Получить [`Error`] из [`SemanticError`]
    pub fn from_semantic(err: SemanticError) -> Self {
        Self {
            error: WhichError::SemanticError(err),
            msg: None,
            diagnostic: None,
        }
    }
}
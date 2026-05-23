pub mod argparser;
pub mod cli_error;
pub mod lexer;
pub mod string_table;

use crate::{
    lexer::{Lexer, lexer_errors::LexerError},
    string_table::StringTable,
};

/// Функция для запуска кода(с отладкой)
pub fn run_with_debug(code: &str) -> Result<(), LexerError> {
    let string_table = &mut StringTable::new();
    let mut lexer = Lexer::new(code, string_table);
    let (tokens, lexer_errors) = lexer.tokenize();
    if !lexer_errors.is_empty() {
        for error in lexer_errors {
            LexerError::report_lexer_error(error, code);
        }
        return Ok(());
    }
    println!("Source code:\n{code}");
    println!("TOKENS:\n{tokens:#?}");
    Ok(())
}

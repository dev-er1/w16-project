pub mod lexer;
pub mod string_pool;
pub mod error;
pub mod parser;

pub use lexer::Lexer;
pub use parser::Parser;

use crate::frontend::{lexer::{token::Token}, parser::{node::TranslationUnit}, string_pool::StringTable};

// ---------------------------------------------------------
// Frontend Context
// ---------------------------------------------------------

pub struct W16CFrontend<'a> {
    pub source: &'a str,
    pub string_table: StringTable,
}

impl<'a> W16CFrontend<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            string_table: StringTable::new(),
        }
    }

    /// Получить токены
    pub fn get_tokens(&mut self) -> Result<Vec<Token>, Vec<error::Error>> {
        let lexer =
            Lexer::new(
                self.source,
                std::mem::take(&mut self.string_table),
            );

        let (tokens, table) =
            lexer.tokenize().map_err(|errs| {
                errs.into_iter()
                    .map(error::Error::from_lexer)
                    .collect::<Vec<_>>()
            })?;

        self.string_table = table;

        Ok(tokens)
    }

    /// Получить AST (не нужно передавать токены).
    pub fn get_ast(&mut self) -> Result<TranslationUnit, Vec<error::Error>> {
        let tokens = self.get_tokens()?;

        Parser::new(tokens)
            .parse()
            .map_err(|err| {
                vec![error::Error::from_parser(err)]
            })
    }

    /// Получить AST (нужно передать токены).
    pub fn get_ast_from_tokens(tokens: Vec<Token>) -> Result<TranslationUnit, Vec<error::Error>> {
        Parser::new(tokens)
            .parse()
            .map_err(|err| {
                vec![error::Error::from_parser(err)]
            })
    }
}
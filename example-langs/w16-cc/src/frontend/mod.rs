pub mod lexer;
pub mod string_pool;
pub mod error;
pub mod parser;
pub mod semantic;

pub use lexer::Lexer;
pub use parser::Parser;

use crate::frontend::{error::Error, lexer::token::Token, parser::node::TranslationUnit, semantic::{checker::Checker, resolver::Resolver}, string_pool::StringTable};

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
    pub fn get_tokens(&mut self) -> Result<Vec<Token>, Vec<Error>> {
        let lexer =
            Lexer::new(
                self.source,
                std::mem::take(&mut self.string_table),
            );

        let (tokens, table) =
            lexer.tokenize().map_err(|errs| {
                errs.into_iter()
                    .map(Error::from_lexer)
                    .collect::<Vec<_>>()
            })?;

        self.string_table = table;

        Ok(tokens)
    }

    /// Получить AST (не нужно передавать токены).
    pub fn get_ast(&mut self) -> Result<TranslationUnit, Vec<Error>> {
        let tokens = self.get_tokens()?;

        Parser::new(tokens)
            .parse()
            .map_err(|err| {
                vec![Error::from_parser(err)]
            })
    }

    pub fn parse(tokens: Vec<Token>) -> Result<TranslationUnit, Vec<Error>> {
        Parser::new(tokens)
            .parse()
            .map_err(|err| {
                vec![Error::from_parser(err)]
            })
    }

    /// Семантический анализ
    pub fn analyse(ast: TranslationUnit) -> Vec<Error> {
        let mut resolver = Resolver::new();
        let resolve_errors = resolver.resolve(&ast);

        let mut checker = Checker::new(&resolver.symbols);

        let mut errors: Vec<Error> =
            resolve_errors.into_iter().map(Error::from_semantic).collect();

        // checker.check возвращает Vec<SemanticError>, а не Result
        let check_errors = checker.check(&ast);
        errors.extend(check_errors.into_iter().map(Error::from_semantic));

        errors
    }
}
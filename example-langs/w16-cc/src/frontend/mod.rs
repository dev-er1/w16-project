//! # Frontend языка
//! 
//! В frontend-е находятся:
//! - [`preprocessor`]. Препроцессор, проходит по исходному коду **до лексера**.
//! - [`Lexer`]. Лексический анализ, проходит по коду, и токенизирует его.
//! - [`Parser`]. Парсер, парсит массив токенов(тот что лексер сделал), и строит абстрактное синтаксическое дерево(AST).
//! - [`semantic`]. Семантика. 2 прохода семантики, проверяет AST.
//! 
//! ## Пайплайн в frontend-е
//! ```text
//! preprocessor
//!   |
//!   V
//! lexer
//!   |
//!   V
//! parser
//!   |
//!   V
//! semantic
//! ```
pub mod lexer;
pub mod string_pool;
pub mod error;
pub mod parser;
pub mod semantic;
pub mod preprocessor;

pub use lexer::Lexer;
pub use parser::Parser;

use crate::frontend::{
    error::Error,
    lexer::token::Token,
    parser::node::TranslationUnit,
    preprocessor::{preprocess, PreprocOptions},
    semantic::{checker::Checker, resolver::Resolver},
    string_pool::StringTable,
};

// =========================================================
// Система шагов компиляции (для гибкости и тестов)
// =========================================================

/// Трейт, связывающий входной тип данных с результатом его обработки.
pub trait CompileStep {
    type Output;
    fn run(self, table: &mut StringTable, source: &str) -> Result<Self::Output, Vec<Error>>;
}

/// Шаг 1: Исходный код -> Токены.
///
/// Запускает препроцессор, потом лексер.
pub struct SourceStep {
    /// Опции препроцессора (пути include, предопределённые дефайны).
    pub opts: PreprocOptions,
}

impl CompileStep for SourceStep {
    type Output = Vec<Token>;

    fn run(self, table: &mut StringTable, source: &str) -> Result<Self::Output, Vec<Error>> {
        // Шаг 1а: препроцессор — работает на уровне текста.
        let preprocessed = preprocess(source, "<source>", &self.opts)
            .map_err(|e| vec![Error::from_preproc(e)])?;

        // Шаг 1б: лексер — принимает препроцессированный текст.
        let current_table = std::mem::take(table);
        let (tokens, updated_table) = W16CFrontend::lex_str(&preprocessed, current_table)?;
        *table = updated_table;
        Ok(tokens)
    }
}

/// Шаг 2: Вектор токенов -> AST (TranslationUnit).
impl CompileStep for Vec<Token> {
    type Output = TranslationUnit;

    fn run(self, _table: &mut StringTable, _source: &str) -> Result<Self::Output, Vec<Error>> {
        W16CFrontend::parse(self)
    }
}

/// Шаг 3: Ссылка на AST -> Валидация (проверка семантики).
impl<'a> CompileStep for &'a TranslationUnit {
    type Output = ();

    fn run(self, _table: &mut StringTable, _source: &str) -> Result<Self::Output, Vec<Error>> {
        let errors = W16CFrontend::analyse(self);
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

// =========================================================
// Frontend Context
// =========================================================

pub struct W16CFrontend<'a> {
    pub source: &'a str,
    pub string_table: StringTable,
    pub preproc_opts: PreprocOptions,
}

impl<'a> W16CFrontend<'a> {
    /// Создаёт новый контекст фронтенда с настройками по умолчанию.
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            string_table: StringTable::new(),
            preproc_opts: PreprocOptions::new(),
        }
    }

    /// Создаёт контекст с явными опциями препроцессора.
    pub fn with_opts(source: &'a str, preproc_opts: PreprocOptions) -> Self {
        Self { source, string_table: StringTable::new(), preproc_opts }
    }

    /// Главная сквозная точка входа.
    /// Запускает: препроцессор -> лексер -> парсер -> семантика.
    pub fn compile_all(&mut self) -> Result<TranslationUnit, Vec<Error>> {
        // 1. Препроцессор + Лексер
        let tokens = self.run_step(SourceStep { opts: self.preproc_opts.clone() })?;

        // 2. Парсер
        let ast = self.run_step(tokens)?;

        // 3. Семантика
        self.run_step(&ast)?;

        Ok(ast)
    }

    /// Универсальный обработчик шагов.
    pub fn run_step<T>(&mut self, step: T) -> Result<T::Output, Vec<Error>>
    where
        T: CompileStep,
    {
        step.run(&mut self.string_table, self.source)
    }

    // ---------------------------------------------------------
    // Изолированные ассоциированные функции (чистый конвейер)
    // ---------------------------------------------------------

    /// Только препроцессор — возвращает препроцессированный текст.
    pub fn preprocess_only(&self) -> Result<String, Vec<Error>> {
        preprocess(self.source, "<source>", &self.preproc_opts)
            .map_err(|e| vec![Error::from_preproc(e)])
    }

    /// Лексер по готовой строке (используется внутри SourceStep).
    pub fn lex_str(preprocessed: &str, table: StringTable) -> Result<(Vec<Token>, StringTable), Vec<Error>> {
        // Лексер требует 'a lifetime совпадающий с source — создаём временный лексер.
        Lexer::new(preprocessed, table)
            .tokenize()
            .map_err(|errs| errs.into_iter().map(Error::from_lexer).collect())
    }

    /// Изолированный лексический анализ исходного source (без препроцессора).
    pub fn lex(source: &'a str, table: StringTable) -> Result<(Vec<Token>, StringTable), Vec<Error>> {
        Lexer::new(source, table)
            .tokenize()
            .map_err(|errs| errs.into_iter().map(Error::from_lexer).collect())
    }

    /// Изолированный синтаксический анализ.
    pub fn parse(tokens: Vec<Token>) -> Result<TranslationUnit, Vec<Error>> {
        Parser::new(tokens)
            .parse()
            .map_err(|err| vec![Error::from_parser(err)])
    }

    /// Изолированный семантический анализ (резолвер + чекер).
    pub fn analyse(ast: &TranslationUnit) -> Vec<Error> {
        let mut resolver = Resolver::new();
        let resolve_errors = resolver.resolve(ast);

        let mut checker = Checker::new(&resolver.symbols);

        let mut errors: Vec<Error> = resolve_errors
            .into_iter()
            .map(Error::from_semantic)
            .collect();

        let check_errors = checker.check(ast);
        errors.extend(check_errors.into_iter().map(Error::from_semantic));

        errors
    }
}
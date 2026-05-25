pub mod lexer;
pub mod string_pool;
pub mod error;
pub mod parser;
pub mod semantic;

pub use lexer::Lexer;
pub use parser::Parser;

use crate::frontend::{error::Error, lexer::token::Token, parser::node::TranslationUnit, semantic::{checker::Checker, resolver::Resolver}, string_pool::StringTable};

// =========================================================
// Система шагов компиляции (для гибкости и тестов)
// =========================================================

/// Трейт, связывающий входной тип данных с результатом его обработки.
pub trait CompileStep {
    type Output;
    fn run(self, table: &mut StringTable, source: &str) -> Result<Self::Output, Vec<Error>>;
}

/// Шаг 1: Исходный код -> Токены
pub struct SourceStep;
impl CompileStep for SourceStep {
    type Output = Vec<Token>;

    fn run(self, table: &mut StringTable, source: &str) -> Result<Self::Output, Vec<Error>> {
        // Временно забираем таблицу строк для лексера
        let current_table = std::mem::take(table);
        let (tokens, updated_table) = W16CFrontend::lex(source, current_table)?;
        // Возвращаем обновленную таблицу назад
        *table = updated_table;
        Ok(tokens)
    }
}

/// Шаг 2: Вектор токенов -> AST (TranslationUnit)
impl CompileStep for Vec<Token> {
    type Output = TranslationUnit;

    fn run(self, _table: &mut StringTable, _source: &str) -> Result<Self::Output, Vec<Error>> {
        W16CFrontend::parse(self)
    }
}

/// Шаг 3: Ссылка на AST -> Валидация (проверка семантики)
impl<'a> CompileStep for &'a TranslationUnit {
    type Output = (); // Возвращает пустоту в случае успеха, либо вектор ошибок

    fn run(self, _table: &mut StringTable, _source: &str) -> Result<Self::Output, Vec<Error>> {
        let errors = W16CFrontend::analyse(self);
        if errors.is_empty() {
            Ok(())
        } else {
            // Приводим Vec<Error> к ожидаемому Result::Err
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
}

impl<'a> W16CFrontend<'a> {
    /// Создает новый контекст фронтенда
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            string_table: StringTable::new(),
        }
    }

    /// Главная сквозная точка входа. Запускает полный цикл:
    /// Лексический анализ -> Синтаксический анализ -> Семантический анализ.
    pub fn compile_all(&mut self) -> Result<TranslationUnit, Vec<Error>> {
        // 1. Лексер
        let tokens = self.run_step(SourceStep)?;
        
        // 2. Парсер
        let ast = self.run_step(tokens)?;
        
        // 3. Семантика
        self.run_step(&ast)?;

        Ok(ast)
    }

    /// Универсальный обработчик шагов. Автоматически выводит возвращаемый тип
    /// в зависимости от того, что передано в качестве аргумента `step`.
    pub fn run_step<T>(&mut self, step: T) -> Result<T::Output, Vec<Error>>
    where
        T: CompileStep,
    {
        step.run(&mut self.string_table, self.source)
    }

    // ---------------------------------------------------------
    // Изолированные ассоциированные функции (Чистый конвейер)
    // ---------------------------------------------------------

    /// Изолированный лексический анализ
    pub fn lex(source: &'a str, table: StringTable) -> Result<(Vec<Token>, StringTable), Vec<Error>> {
        let lexer = Lexer::new(source, table);
        lexer.tokenize().map_err(|errs| {
            errs.into_iter()
                .map(Error::from_lexer)
                .collect::<Vec<_>>()
        })
    }

    /// Изолированный синтаксический анализ (Парсер)
    pub fn parse(tokens: Vec<Token>) -> Result<TranslationUnit, Vec<Error>> {
        Parser::new(tokens)
            .parse()
            .map_err(|err| vec![Error::from_parser(err)])
    }

    /// Изолированный семантический анализ (Резолвер + Чекер)
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
//! # Модуль parser.
//!
//! Parser получает токены от lexer и строит HIR AST. На этом уровне мы проверяем
//! только грамматику: корректность типов, областей видимости и сигнатур функций
//! остаётся задачей semantic verifier.

mod error;
mod hir_parser;

/// Ошибка синтаксического анализа.
pub use error::ParseError;
/// HIR parser и удобная функция `parse_hir_module`.
pub use hir_parser::{Parser, parse_hir_module};

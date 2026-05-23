//! # Модуль semantic verifier.
//!
//! Parser проверяет только форму текста. Semantic verifier проверяет смысл:
//! объявлены ли имена, совпадают ли типы, корректны ли вызовы функций и return.

mod error;
mod hir;

/// Ошибка семантической проверки.
pub use error::SemanticError;
/// Проверить HIR-модуль.
pub use hir::verify_hir_module;

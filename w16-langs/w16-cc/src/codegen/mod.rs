// w16-langs\w16-cc\src\codegen\mod.rs
//
//! # Трансляция W16-CC AST в W16-HIR.
//!
//! ## Два выхода
//!
//! - [`AstTranslator`] — строит `hir::Module` напрямую (AST -> HIR AST).
//!   Используется когда нужно запустить через `w16_lib::W16::run_hir_ast`.
//!
//! - [`TextEmitter`] — генерирует текстовый W16-HIR (`String`).
//!   Удобен для отладки и сохранения `.w16h`-файлов.
//!
//! ## Ограничения текущей версии
//! - `struct`/`union` не поддерживаются (нет в HIR без указателей).
//! - Препроцессорные директивы игнорируются (пропускаются парсером).
//! - `goto`/метки не поддерживаются — HIR не имеет unstructured jumps.
//! - `switch` раскрывается в цепочку `if/else`.
//! - `do-while` транслируется напрямую в `hir::Stmt::DoWhile`.
//! - Все целые типы C -> `hir::Type::I64` или `U64` в зависимости от знака.
//! - `float`/`double` -> `hir::Type::F64`.
//! - `char` -> `hir::Type::U64` (байт).

pub mod translator;
pub mod emitter;

pub use translator::AstTranslator;
pub use emitter::TextEmitter;

use crate::frontend::lexer::token::Span;

/// Ошибка трансляции.
#[derive(Debug, Clone)]
pub struct TranslationError {
    pub span: Span,
    pub message: String,
}

impl TranslationError {
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Self { span, message: message.into() }
    }
}

impl std::fmt::Display for TranslationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (l, c) = self.span.start_line_and_col;
        write!(f, "[{l}:{c}] translation error: {}", self.message)
    }
}

pub type TranslationResult<T> = Result<T, TranslationError>;

// ---------------------------------------------------------------------------
// Маппинг типов C -> HIR
// ---------------------------------------------------------------------------

use crate::types::Type as CType;
use w16_ir::hir::Type as HirType;

/// Переводит тип C в тип HIR.
/// Знаковые целые -> `I64`, беззнаковые -> `U64`, вещественные -> `F64`.
pub fn map_type(ty: &CType) -> HirType {
    match ty {
        CType::Void => HirType::Unit,
        CType::Bool => HirType::Bool,

        // Знаковые целые
        CType::Char | CType::SignedChar
        | CType::Short | CType::Int
        | CType::Long  | CType::LongLong => HirType::I64,

        // Беззнаковые целые
        CType::UnsignedChar | CType::UnsignedShort
        | CType::UnsignedInt | CType::UnsignedLong
        | CType::UnsignedLongLong => HirType::U64,

        // Вещественные
        CType::Float | CType::Double | CType::LongDouble => HirType::F64,

        // Всё остальное (комплексные, atomic, массив) — U64 как заглушка
        _ => HirType::U64,
    }
}
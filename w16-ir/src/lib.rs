//! # W16-IR
//!
//! Промежуточное представление.

/// Компиляция MIR-а, в байт-код.
pub mod compiler_to_bytecode;
/// Типизированное HIR-представление программы.
pub mod hir;
/// Лексер текстового IR: превращает исходный текст в поток токенов.
pub mod lexer;
/// Представление MIR.
pub mod mir;
/// MIR-этап.
pub mod mir_f;
/// Parser HIR: превращает токены в `hir::Module`.
pub mod parser;
/// Семантические проверки HIR.
pub mod semantic;
/// Переводчик HIR в MIR.
pub mod translator;

pub use compiler_to_bytecode::compile_mir_to_bytecode;
pub use parser::parse_hir_module;
pub use semantic::verify_hir_module;
pub use translator::lowerer::lower_hir_to_mir;

use crate::hir::Module;
use w16_core::Bytecode;

pub fn compile_hir_to_bytecode(hir_source: &str) -> Result<Bytecode, String> {
    // 1. Парсим HIR
    let hir_module = parse_hir_module(hir_source).map_err(|e| format!("Parser error: {e}"))?;

    // 2. Проверяем семантику
    verify_hir_module(&hir_module).map_err(|errors| format!("Semantic errors: {errors:?}"))?;

    // 3. Переводим в MIR
    let mut mir_module = lower_hir_to_mir(&hir_module);

    mir_f::mir_optimizer::optimize_module(&mut mir_module);

    // 4. Компилируем в bytecode (берём первую функцию)
    let bytecode = compile_mir_to_bytecode(&mir_module, 0)
        .map_err(|e| format!("\x1b[31m\x1b[1mError\x1b[0m: \x1b[1m{e}\x1b[0m"))?;

    Ok(bytecode)
}

pub fn lower_text_hir_to_hir_module(hir_source: &str) -> Result<Module, String> {
    let hir_module = parse_hir_module(hir_source).map_err(|e| format!("Parser error: {e}"))?;

    verify_hir_module(&hir_module).map_err(|errors| format!("Semantic errors: {errors:?}"))?;

    Ok(hir_module)
}

// w16-lib\src\lib.rs
//
//! Удобная библиотека для встраивания W16 в другие проекты.
//!
//! `w16-lib` не реализует свой компилятор или runtime. Этот крейт является
//! стабильным фасадом поверх `w16-ir` и `w16-core`: принимает программу на
//! разных уровнях представления, доводит её до bytecode и запускает через VM
//! или JIT. AOT сюда намеренно не входит, потому что AOT-бэкенд пока считается
//! экспериментальным.

use std::{error::Error, fmt};

pub use w16_core::{Bytecode, ConstantPool, Instruction, OpCode, REGISTER_COUNT, VMError};
pub use w16_ir::{
    hir,
    lexer::{Span, Token, TokenKind},
    mir,
};

/// Доступные стабильные способы запуска W16 bytecode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    /// Запуск через регистровую VM.
    Interpreter,

    /// Запуск через Cranelift JIT.
    Jit,

    /// Запуск через Orca VM (экспериментальная ВМ).
    Orca,
}

/// Настройки компиляции HIR/MIR в bytecode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompileOptions {
    /// FunctionId точки входа. Обычно `@main` оказывается функцией `0`.
    pub entry_function_id: usize,

    /// Запускать MIR-оптимизации перед генерацией bytecode.
    pub optimize_mir: bool,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            entry_function_id: 0,
            optimize_mir: true,
        }
    }
}

/// Настройки запуска уже скомпилированного bytecode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunOptions {
    /// VM или JIT.
    pub mode: ExecutionMode,
    /// Размер памяти для интерпретатора. JIT сейчас использует внутренний
    /// буфер памяти из `w16-core`.
    pub memory_size: usize,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            mode: ExecutionMode::Interpreter,
            memory_size: 1024 * 1024,
        }
    }
}

/// Полный результат запуска программы.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunResult {
    /// Значения всех 256 регистров после завершения программы.
    pub registers: [u64; REGISTER_COUNT],
}

impl RunResult {
    /// Регистр возврата W16 ABI.
    pub fn r0(&self) -> u64 {
        self.registers[0]
    }

    /// Вернуть конкретный регистр.
    pub fn register(&self, index: usize) -> Option<u64> {
        self.registers.get(index).copied()
    }
}

/// Ошибка фасадного API.
#[derive(Debug)]
pub enum W16Error {
    /// Ошибка lexer-а.
    Lex(String),
    /// Ошибка parser-а.
    Parse(String),
    /// Ошибка semantic verifier-а для HIR.
    Semantic(String),
    /// Ошибка MIR verifier-а.
    MirVerify(String),
    /// Ошибка генерации bytecode.
    Bytecode(String),
    /// Ошибка интерпретатора.
    Vm(VMError),
    /// Ошибка JIT.
    Jit(String),
}

impl fmt::Display for W16Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lex(err) => write!(f, "lexer error: {err}"),
            Self::Parse(err) => write!(f, "parser error: {err}"),
            Self::Semantic(err) => write!(f, "semantic error: {err}"),
            Self::MirVerify(err) => write!(f, "MIR verifier error: {err}"),
            Self::Bytecode(err) => write!(f, "bytecode compiler error: {err}"),
            Self::Vm(err) => write!(f, "VM error: {err}"),
            Self::Jit(err) => write!(f, "JIT error: {err}"),
        }
    }
}

impl Error for W16Error {}

impl From<VMError> for W16Error {
    fn from(value: VMError) -> Self {
        Self::Vm(value)
    }
}

/// Удобный builder для повторного использования настроек.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct W16 {
    compile: CompileOptions,
    run: RunOptions,
}

impl W16 {
    /// Создать фасад с настройками по умолчанию.
    pub fn new() -> Self {
        Self::default()
    }

    /// Выбрать VM или JIT.
    pub fn with_mode(mut self, mode: ExecutionMode) -> Self {
        self.run.mode = mode;
        self
    }

    /// Выбрать размер памяти для интерпретатора.
    pub fn with_memory_size(mut self, memory_size: usize) -> Self {
        self.run.memory_size = memory_size;
        self
    }

    /// Выбрать FunctionId точки входа.
    pub fn with_entry_function(mut self, entry_function_id: usize) -> Self {
        self.compile.entry_function_id = entry_function_id;
        self
    }

    /// Включить или отключить MIR-оптимизации.
    pub fn with_mir_optimization(mut self, enabled: bool) -> Self {
        self.compile.optimize_mir = enabled;
        self
    }

    /// Принять HIR в текстовом формате и запустить.
    pub fn run_hir_text(&self, source: &str) -> Result<RunResult, W16Error> {
        let bytecode = compile_hir_text_with_options(source, self.compile)?;
        run_bytecode_with_options(&bytecode, self.run)
    }

    /// Принять HIR как поток токенов и запустить.
    pub fn run_hir_tokens(&self, tokens: Vec<Token>) -> Result<RunResult, W16Error> {
        let bytecode = compile_hir_tokens_with_options(tokens, self.compile)?;
        run_bytecode_with_options(&bytecode, self.run)
    }

    /// Принять HIR AST и запустить.
    pub fn run_hir_ast(&self, module: &hir::Module) -> Result<RunResult, W16Error> {
        let bytecode = compile_hir_ast_with_options(module, self.compile)?;
        run_bytecode_with_options(&bytecode, self.run)
    }

    /// Принять MIR AST и запустить.
    pub fn run_mir_ast(&self, module: &mir::MIRModule) -> Result<RunResult, W16Error> {
        let bytecode = compile_mir_ast_with_options(module, self.compile)?;
        run_bytecode_with_options(&bytecode, self.run)
    }

    pub fn hir_to_bytecode(&self, module: &str) -> Result<Bytecode, W16Error> {
        match compile_hir_text_with_options(module, self.compile) {
            Ok(bc) => Ok(bc),
            Err(e) => Err(e),
        }
    }

    /// Принять готовый bytecode и запустить.
    #[inline(always)]
    pub fn run_bytecode(&self, bytecode: &Bytecode) -> Result<RunResult, W16Error> {
        run_bytecode_with_options(bytecode, self.run)
    }

    /// Вывести все этапы компиляции
    pub fn debug_full_pipeline(source: &str) -> Result<String, W16Error> {
        let mut output = String::new();

        // Tokens
        output.push_str(&debug_hir_tokens(source)?);
        output.push('\n');

        // HIR AST
        output.push_str(&debug_hir_ast(source)?);
        output.push('\n');

        // MIR AST (без оптимизаций)
        output.push_str(&debug_mir_ast(source)?);
        output.push('\n');

        // MIR AST (с оптимизациями)
        output.push_str(&debug_mir_ast_optimized(source)?);
        output.push('\n');

        // Bytecode
        output.push_str(&debug_bytecode(source)?);

        Ok(output)
    }
}

/// Разбить HIR-текст на токены.
pub fn tokenize_hir(source: &str) -> Result<Vec<Token>, W16Error> {
    w16_ir::lexer::Lexer::new(source)
        .tokenize()
        .map_err(|err| W16Error::Lex(err.to_string()))
}

/// Распарсить HIR-текст в AST и проверить семантику.
pub fn parse_hir_text(source: &str) -> Result<hir::Module, W16Error> {
    let module =
        w16_ir::parse_hir_module(source).map_err(|err| W16Error::Parse(err.to_string()))?;
    verify_hir_ast(&module)?;
    Ok(module)
}

/// Распарсить заранее полученные токены HIR в AST и проверить семантику.
pub fn parse_hir_tokens(tokens: Vec<Token>) -> Result<hir::Module, W16Error> {
    let mut parser = w16_ir::parser::Parser::new(tokens);
    let module = parser
        .parse_module()
        .map_err(|err| W16Error::Parse(err.to_string()))?;
    verify_hir_ast(&module)?;
    Ok(module)
}

/// Проверить HIR AST semantic verifier-ом.
pub fn verify_hir_ast(module: &hir::Module) -> Result<(), W16Error> {
    w16_ir::verify_hir_module(module).map_err(|errors| W16Error::Semantic(format!("{errors:?}")))
}

/// Понизить HIR AST в MIR AST.
pub fn lower_hir_ast_to_mir(module: &hir::Module) -> Result<mir::MIRModule, W16Error> {
    verify_hir_ast(module)?;
    Ok(w16_ir::lower_hir_to_mir(module))
}

/// Скомпилировать HIR-текст в bytecode с настройками по умолчанию.
pub fn compile_hir_text(source: &str) -> Result<Bytecode, W16Error> {
    compile_hir_text_with_options(source, CompileOptions::default())
}

/// Скомпилировать HIR-текст в bytecode.
pub fn compile_hir_text_with_options(
    source: &str,
    options: CompileOptions,
) -> Result<Bytecode, W16Error> {
    let module = parse_hir_text(source)?;
    compile_hir_ast_with_options(&module, options)
}

/// Скомпилировать поток HIR-токенов в bytecode с настройками по умолчанию.
#[inline(always)]
pub fn compile_hir_tokens(tokens: Vec<Token>) -> Result<Bytecode, W16Error> {
    compile_hir_tokens_with_options(tokens, CompileOptions::default())
}

/// Скомпилировать поток HIR-токенов в bytecode.
pub fn compile_hir_tokens_with_options(
    tokens: Vec<Token>,
    options: CompileOptions,
) -> Result<Bytecode, W16Error> {
    let module = parse_hir_tokens(tokens)?;
    compile_hir_ast_with_options(&module, options)
}

/// Скомпилировать HIR AST в bytecode с настройками по умолчанию.
#[inline(always)]
pub fn compile_hir_ast(module: &hir::Module) -> Result<Bytecode, W16Error> {
    compile_hir_ast_with_options(module, CompileOptions::default())
}

/// Скомпилировать HIR AST в bytecode.
pub fn compile_hir_ast_with_options(
    module: &hir::Module,
    options: CompileOptions,
) -> Result<Bytecode, W16Error> {
    let mir = lower_hir_ast_to_mir(module)?;
    compile_mir_ast_with_options(&mir, options)
}

/// Скомпилировать MIR AST в bytecode с настройками по умолчанию.
#[inline(always)]
pub fn compile_mir_ast(module: &mir::MIRModule) -> Result<Bytecode, W16Error> {
    compile_mir_ast_with_options(module, CompileOptions::default())
}

/// ## Скомпилировать MIR AST в bytecode.
///
/// Функция принимает `&MIRModule`, но внутри клонирует модуль, потому что
/// optimizer меняет MIR на месте. Это делает API безопасным для вызывающего
/// кода: переданный AST не портится.
pub fn compile_mir_ast_with_options(
    module: &mir::MIRModule,
    options: CompileOptions,
) -> Result<Bytecode, W16Error> {
    let mut module = module.clone();
    if options.optimize_mir {
        w16_ir::mir_f::mir_optimizer::optimize_module(&mut module);
    }
    verify_mir_ast(&module)?;
    w16_ir::compile_mir_to_bytecode(&module, options.entry_function_id)
        .map_err(|err| W16Error::Bytecode(err.to_string()))
}

/// Проверить MIR AST verifier-ом.
pub fn verify_mir_ast(module: &mir::MIRModule) -> Result<(), W16Error> {
    w16_ir::mir_f::mir_verify::verify_module(module)
        .map_err(|errors| W16Error::MirVerify(format!("{errors:?}")))
}

/// Запустить HIR-текст через VM.
pub fn run_hir_text(source: &str) -> Result<RunResult, W16Error> {
    W16::new().run_hir_text(source)
}

/// Запустить HIR-текст через выбранный режим.
pub fn run_hir_text_as(source: &str, mode: ExecutionMode) -> Result<RunResult, W16Error> {
    W16::new().with_mode(mode).run_hir_text(source)
}

/// Запустить поток HIR-токенов через VM.
pub fn run_hir_tokens(tokens: Vec<Token>) -> Result<RunResult, W16Error> {
    W16::new().run_hir_tokens(tokens)
}

/// Запустить HIR AST через VM.
pub fn run_hir_ast(module: &hir::Module) -> Result<RunResult, W16Error> {
    W16::new().run_hir_ast(module)
}

/// Запустить MIR AST через VM.
pub fn run_mir_ast(module: &mir::MIRModule) -> Result<RunResult, W16Error> {
    W16::new().run_mir_ast(module)
}

/// Запустить готовый bytecode через VM.
pub fn run_bytecode(bytecode: &Bytecode) -> Result<RunResult, W16Error> {
    W16::new().run_bytecode(bytecode)
}

/// Запустить готовый bytecode с явными настройками.
pub fn run_bytecode_with_options(
    bytecode: &Bytecode,
    options: RunOptions,
) -> Result<RunResult, W16Error> {
    let registers = match options.mode {
        ExecutionMode::Interpreter => w16_core::run(bytecode, options.memory_size)?,
        ExecutionMode::Jit => {
            w16_core::run_by_jit(bytecode).map_err(|err| W16Error::Jit(err.to_string()))?
        },
        ExecutionMode::Orca => {
            let mut orca_vm = w16_core::interpreter::evms::OrcaEvm::new(1024);

            unsafe { orca_vm.run_unchecked(bytecode); orca_vm.registers }
        }
    };

    Ok(RunResult { registers })
}

// ====================================================================================
// Дебаг

/// Вывести HIR токены в debug формате
pub fn debug_hir_tokens(source: &str) -> Result<String, W16Error> {
    let tokens = tokenize_hir(source)?;
    let mut output = String::new();
    output.push_str("=== HIR TOKENS ===\n");
    for (idx, token) in tokens.iter().enumerate() {
        output.push_str(&format!("[{idx}] {token:?}\n"));
    }
    Ok(output)
}

/// Вывести HIR AST в debug формате
pub fn debug_hir_ast(source: &str) -> Result<String, W16Error> {
    let module = parse_hir_text(source)?;
    let mut output = String::new();
    output.push_str("=== HIR AST ===\n");
    output.push_str(&format!("{module:#?}\n"));
    Ok(output)
}

/// Вывести MIR AST в debug формате
pub fn debug_mir_ast(source: &str) -> Result<String, W16Error> {
    let hir = parse_hir_text(source)?;
    let mir = lower_hir_ast_to_mir(&hir)?;
    let mut output = String::new();
    output.push_str("=== MIR AST (before optimizer) ===\n");
    output.push_str(&format!("{mir:#?}\n"));
    Ok(output)
}

/// Вывести MIR AST с оптимизациями
pub fn debug_mir_ast_optimized(source: &str) -> Result<String, W16Error> {
    let hir = parse_hir_text(source)?;
    let mut mir = w16_ir::lower_hir_to_mir(&hir);
    w16_ir::mir_f::mir_optimizer::optimize_module(&mut mir);
    let mut output = String::new();
    output.push_str("=== MIR AST (optimized) ===\n");
    output.push_str(&format!("{mir:#?}\n"));
    Ok(output)
}

/// Вывести Bytecode инструкции в debug формате
pub fn debug_bytecode(source: &str) -> Result<String, W16Error> {
    let bytecode = compile_hir_text(source)?;
    let mut output = String::new();
    output.push_str("=== BYTECODE ===\n");
    output.push_str(&format!("Instructions: {}\n", bytecode.instructions.len()));
    for (idx, inst) in bytecode.instructions.iter().enumerate() {
        output.push_str(&format!("[{idx}] {inst:?}\n"));
    }
    output.push_str(&format!(
        "\nConstant Pool: {} bytes\n",
        bytecode.constant_pool.data.len()
    ));
    Ok(output)
}
// ====================================================================================

#[cfg(test)]
mod tests {
    use super::*;

    const PROGRAM: &str = r#"
        module test {
            fn @main() -> u64 {
                let $x: u64 = 10
                let $y: u64 = 32
                return $x + $y
            }
        }
    "#;

    #[test]
    fn runs_hir_text() {
        let result = run_hir_text(PROGRAM).unwrap();
        assert_eq!(result.r0(), 42);
    }

    #[test]
    fn runs_hir_tokens() {
        let tokens = tokenize_hir(PROGRAM).unwrap();
        let result = run_hir_tokens(tokens).unwrap();
        assert_eq!(result.r0(), 42);
    }

    #[test]
    fn runs_hir_ast() {
        let ast = parse_hir_text(PROGRAM).unwrap();
        let result = run_hir_ast(&ast).unwrap();
        assert_eq!(result.r0(), 42);
    }

    #[test]
    fn runs_mir_ast() {
        let ast = parse_hir_text(PROGRAM).unwrap();
        let mir = lower_hir_ast_to_mir(&ast).unwrap();
        let result = run_mir_ast(&mir).unwrap();
        assert_eq!(result.r0(), 42);
    }

    #[test]
    fn runs_bytecode() {
        let bytecode = compile_hir_text(PROGRAM).unwrap();
        let result = run_bytecode(&bytecode).unwrap();
        assert_eq!(result.r0(), 42);
    }

    #[test]
    fn builder_can_use_jit() {
        let result = W16::new()
            .with_mode(ExecutionMode::Jit)
            .run_hir_text(PROGRAM)
            .unwrap();
        assert_eq!(result.r0(), 42);
    }
}

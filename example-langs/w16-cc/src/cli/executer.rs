// example-langs\w16-cc\src\cli\executer.rs
//
//! # Исполнитель: диспетчеризует команду в пайплайн компилятора.
//!
//! Пайплайн:
//!   C source -> W16CFrontend (lex + parse + semantic)
//!            -> AstTranslator (C AST -> hir::Module)
//!            -> [TextEmitter | w16_lib::W16 | w16c]

use std::time::Instant;

use w16_cc::{
    W16CFrontend,
    codegen::{AstTranslator, TextEmitter},
    frontend::semantic::{resolver::Resolver, checker::Checker},
};
use w16_lib::{ExecutionMode, W16};

use super::cmd::{Command, CommandKind, RunMode};
use super::error::{CLIError, CliErrorKind, err_file_not_found};
use super::help;

// ANSI
const BOLD: &str = "\x1b[1m";
const GREEN: &str = "\x1b[32m";
const CYAN: &str = "\x1b[36m";
const GREY: &str = "\x1b[90m";
const RESET: &str = "\x1b[0m";

pub struct Executer;

impl Executer {
    pub fn execute(cmd: Command) -> Result<(), CLIError> {
        match cmd.kind {
            CommandKind::Compile => Self::compile(cmd),
            CommandKind::Run => Self::run(cmd),
            CommandKind::EmitHir => Self::emit_hir(cmd),
            CommandKind::Check => Self::check(cmd),
            CommandKind::Version => Self::version(),
            CommandKind::Help => { help::print(); Ok(()) }
        }
    }

    // -----------------------------------------------------------------------
    // compile
    // -----------------------------------------------------------------------

    fn compile(cmd: Command) -> Result<(), CLIError> {
        let file = cmd.file.expect("parser guarantees file for `compile`");
        let source = read_file(&file)?;

        let hir = frontend_to_hir(&source, &file)?;

        // Выходное имя: --out или stem входного файла
        let out_name = cmd.flags.out.unwrap_or_else(|| {
            std::path::Path::new(&file)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("output")
                .to_owned()
        });

        let target = target_lexicon::Triple::host();
        let bc = w16_lib::compile_hir_ast(&hir)
            .map_err(|e| CLIError::new(CliErrorKind::Runtime(e.to_string())))?;

        w16c::compile_to_executable(&bc, &out_name, target)
            .map_err(|e| CLIError::new(CliErrorKind::Runtime(e.to_string())))?;

        println!("{BOLD}{GREEN}compiled{RESET} {GREY}->{RESET} {CYAN}{out_name}{RESET}");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // run
    // -----------------------------------------------------------------------

    fn run(cmd: Command) -> Result<(), CLIError> {
        let file = cmd.file.expect("parser guarantees file for `run`");
        let source = read_file(&file)?;
        let hir = frontend_to_hir(&source, &file)?;

        let mode = match cmd.flags.run_mode {
            RunMode::Interpreter => ExecutionMode::Interpreter,
            RunMode::Jit => ExecutionMode::Jit,
        };

        let start = cmd.flags.show_time.then(Instant::now);

        W16::new()
            .with_mode(mode)
            .run_hir_ast(&hir)
            .map_err(|e| CLIError::new(CliErrorKind::Runtime(e.to_string())))?;

        if let Some(t) = start {
            eprintln!("{BOLD}{GREEN}Finished{RESET} in {BOLD}{:.4?}{RESET}", t.elapsed());
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // emit-hir
    // -----------------------------------------------------------------------

    fn emit_hir(cmd: Command) -> Result<(), CLIError> {
        let file = cmd.file.expect("parser guarantees file for `emit-hir`");
        let source = read_file(&file)?;
        let hir = frontend_to_hir(&source, &file)?;
        let text = TextEmitter::emit(&hir);

        match cmd.flags.out {
            Some(out_path) => {
                std::fs::write(&out_path, &text)
                    .map_err(|e| CLIError::new(CliErrorKind::Io(e.to_string())))?;
                eprintln!("{BOLD}{GREEN}emitted{RESET} {GREY}->{RESET} {CYAN}{out_path}{RESET}");
            }
            None => print!("{text}"),
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // check
    // -----------------------------------------------------------------------

    fn check(cmd: Command) -> Result<(), CLIError> {
        let file = cmd.file.expect("parser guarantees file for `check`");
        let source = read_file(&file)?;

        let mut frontend = W16CFrontend::new(&source);

        let ast = frontend.compile_all().map_err(|errs| {
            CLIError::new(CliErrorKind::CompilerError(format_frontend_errors(&errs)))
        })?;

        // Resolver
        let mut resolver = Resolver::new();
        let resolve_errors = resolver.resolve(&ast);
        if !resolve_errors.is_empty() {
            return Err(CLIError::new(CliErrorKind::CompilerError(
                format_semantic_errors(&resolve_errors),
            )));
        }

        // Checker
        let mut checker = Checker::new(&resolver.symbols);
        let check_errors = checker.check(&ast);
        if !check_errors.is_empty() {
            return Err(CLIError::new(CliErrorKind::CompilerError(
                format_semantic_errors(&check_errors),
            )));
        }

        println!("{BOLD}{GREEN}Ok{RESET} — no errors in `{file}`");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // version
    // -----------------------------------------------------------------------

    fn version() -> Result<(), CLIError> {
        println!("w16cc {}", env!("CARGO_PKG_VERSION"));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Пайплайн C -> hir::Module
// ---------------------------------------------------------------------------

fn frontend_to_hir(
    source: &str,
    file_path: &str,
) -> Result<w16_ir::hir::Module, CLIError> {
    let mut frontend = W16CFrontend::new(source);

    // Парсинг
    let ast = frontend.compile_all().map_err(|errs| {
        CLIError::new(CliErrorKind::CompilerError(format_frontend_errors(&errs)))
    })?;

    // Семантика
    let mut resolver = Resolver::new();
    let resolve_errors = resolver.resolve(&ast);
    if !resolve_errors.is_empty() {
        return Err(CLIError::new(CliErrorKind::CompilerError(
            format_semantic_errors(&resolve_errors),
        )));
    }

    let mut checker = Checker::new(&resolver.symbols);
    let check_errors = checker.check(&ast);
    if !check_errors.is_empty() {
        return Err(CLIError::new(CliErrorKind::CompilerError(
            format_semantic_errors(&check_errors),
        )));
    }

    // Трансляция в HIR
    let module_name = std::path::Path::new(file_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("module");

    let mut translator = AstTranslator::new(&frontend.string_table);
    translator.translate(&ast, module_name)
        .map_err(|e| CLIError::new(CliErrorKind::CodegenError(e.message)))
}

// ---------------------------------------------------------------------------
// Хелперы
// ---------------------------------------------------------------------------

fn read_file(path: &str) -> Result<String, CLIError> {
    if !std::path::Path::new(path).exists() {
        return Err(err_file_not_found(path));
    }
    std::fs::read_to_string(path)
        .map_err(|e| CLIError::new(CliErrorKind::Io(e.to_string())))
}

fn format_frontend_errors(errs: &[w16_cc::frontend::error::Error]) -> String {
    errs.iter()
        .map(|e| e.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_semantic_errors(
    errs: &[w16_cc::frontend::semantic::error::SemanticError],
) -> String {
    errs.iter()
        .map(|e| e.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}
// w16-cli\src\executer.rs
//
//! Исполнитель: диспетчеризует валидированную [`Command`] в действия рантайма.
//!
//! Единственное место во всём CLI, которое обращается к `w16_lib`.
//! Никакого разбора аргументов, никакого форматирования вывода — только диспетч.
//! Проверки файловой системы тоже живут здесь, а не в парсере.

use std::time::Instant;

use w16_lib::{
    ExecutionMode, W16, debug_bytecode, debug_hir_ast, debug_hir_tokens, debug_mir_ast,
    debug_mir_ast_optimized, run_hir_text_as,
};

use crate::cmd::{Command, CommandKind, DbgStage, RunMode};
use crate::error::{CLIError, CliErrorKind, err_file_not_found};
use crate::help;

pub struct Executer;

impl Executer {
    pub fn execute(cmd: Command) -> Result<(), CLIError> {
        match cmd.kind {
            CommandKind::Run => Self::run(cmd),
            CommandKind::Build => Self::build(cmd),
            CommandKind::Dbg(stage) => Self::dbg(stage, cmd),
            CommandKind::Version => Self::version(),
            CommandKind::Help => {
                help::print();
                Ok(())
            }
        }
    }

    fn run(cmd: Command) -> Result<(), CLIError> {
        // Наличие файла гарантировано парсером.
        let file = cmd.file.expect("парсер гарантирует file для `run`");

        if !std::path::Path::new(&file).exists() {
            return Err(err_file_not_found(&file));
        }

        let source = read_file(&file)?;

        let mode = match cmd.flags.run_mode {
            RunMode::Interpreter => ExecutionMode::Interpreter,
            RunMode::Jit => ExecutionMode::Jit,
            RunMode::OrcaVM => ExecutionMode::Orca
        };

        // Замер времени: оборачиваем только сам вызов рантайма.
        let start = cmd.flags.show_time.then(Instant::now);

        run_hir_text_as(&source, mode)
            .map_err(|e| CLIError::new(CliErrorKind::Runtime(e.to_string())))?;

        if let Some(t) = start {
            eprintln!(
                "\x1b[1;32mFinished\x1b[0m in \x1b[1m{:.4?}\x1b[0m",
                t.elapsed()
            );
        }

        Ok(())
    }

    fn build(cmd: Command) -> Result<(), CLIError> {
        let file = cmd.file.expect("парсер гарантирует file для `build`");

        if !std::path::Path::new(&file).exists() {
            return Err(err_file_not_found(&file));
        }

        let source = read_file(&file)?;

        // Получаем архитектуру этого устройства
        let target = target_lexicon::Triple::host();

        let w16 = W16::new();

        let bc = w16
            .hir_to_bytecode(&source)
            .map_err(|e| CLIError::new(CliErrorKind::Runtime(e.to_string())))?;

        // --- Отрезаем расширение ---
        // Превращаем строку в Path, берем file_stem, переводим обратно в String.
        // Если что-то пойдет не так (например, имя файла пустое), оставляем исходное имя как запасной вариант.
        let output_name = std::path::Path::new(&file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&file)
            .to_string();

        // Передаем &output_name вместо исходного &file
        w16c::compile_to_executable(&bc, &output_name, target)
            .map_err(|e| CLIError::new(CliErrorKind::Runtime(e.to_string())))?;

        Ok(())
    }

    fn dbg(stage: DbgStage, cmd: Command) -> Result<(), CLIError> {
        let file = cmd.file.expect("парсер гарантирует file для `dbg`");

        if !std::path::Path::new(&file).exists() {
            return Err(err_file_not_found(&file));
        }

        let source = read_file(&file)?;

        let result = match stage {
            DbgStage::Tokens => debug_hir_tokens(&source),
            DbgStage::Hir => debug_hir_ast(&source),
            DbgStage::Mir => debug_mir_ast(&source),
            DbgStage::MirOpt => debug_mir_ast_optimized(&source),
            DbgStage::Bytecode => debug_bytecode(&source),
            DbgStage::Full => W16::debug_full_pipeline(&source),
        }.map_err(|e| CLIError::new(CliErrorKind::Runtime(e.to_string())))?;

        println!("{result}");
        Ok(())
    }

    fn version() -> Result<(), CLIError> {
        println!("w16 {}", env!("CARGO_PKG_VERSION"));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Хелперы
// ---------------------------------------------------------------------------

/// Читает файл в строку, заворачивая ошибку I/O в `CLIError`.
fn read_file(path: &str) -> Result<String, CLIError> {
    std::fs::read_to_string(path).map_err(|e| CLIError::new(CliErrorKind::Runtime(e.to_string())))
}

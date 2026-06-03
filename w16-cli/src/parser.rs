// w16-cli\src\parser.rs
//
//! Парсер: поток токенов -> валидированная [`Command`].
//!
//! Берёт плоский `Vec<Token>` от токенизатора и применяет семантические
//! правила (обязательные аргументы, конфликты флагов и т.д.), возвращая
//! либо готовую к исполнению [`Command`], либо [`CLIError`].
//!
//! Проверка существования файла на диске намеренно здесь отсутствует —
//! это side-effect, который принадлежит `executer.rs`.

use crate::cmd::{Command, CommandFlags, CommandKind, DbgStage, RunMode};
use crate::error::{
    CLIError, err_conflicting_flags, err_missing_arg, err_unknown_command, err_unknown_stage,
};
use crate::tokenizer::Token;

pub struct Parser;

impl Parser {
    /// Разбирает поток токенов в [`Command`].
    pub fn parse(tokens: &[Token]) -> Result<Command, CLIError> {
        // Нет токенов вообще -> показываем справку.
        let subcommand = match tokens.first() {
            Some(Token::SubCommand(s)) => s.as_str(),
            _ => return Ok(Command::new(CommandKind::Help)),
        };

        match subcommand {
            "run" => parse_run(tokens),
            "build" => parse_build(tokens),
            "dbg" => parse_dbg(tokens),
            "version" | "--version" | "-V" => Ok(Command::new(CommandKind::Version)),
            "help" | "--help" | "-h" => Ok(Command::new(CommandKind::Help)),
            unknown => Err(err_unknown_command(unknown)),
        }
    }
}

// ---------------------------------------------------------------------------
// Парсеры отдельных команд
// ---------------------------------------------------------------------------

fn parse_run(tokens: &[Token]) -> Result<Command, CLIError> {
    let file = extract_positional(tokens, 0).ok_or_else(|| err_missing_arg("run", "file.w16h"))?;

    let wants_interp = has_short(tokens, "-i");
    let wants_jit = has_short(tokens, "-j");
    let wants_orca = has_long(tokens, "--orca");

    if wants_interp && wants_jit && wants_orca {
        return Err(err_conflicting_flags("-i", "-j", "--orca"));
    }

    let run_mode = match (wants_interp, wants_interp, wants_orca) {
        (true, false, false) => RunMode::Interpreter,
        (false, true, false) => RunMode::Jit,
        (false, false, true) => RunMode::OrcaVM,
        _ => RunMode::Interpreter
    };
    let show_time = has_long(tokens, "--time");

    Ok(Command::new(CommandKind::Run)
        .with_file(file)
        .with_flags(CommandFlags {
            run_mode,
            show_time,
        }))
}

fn parse_build(tokens: &[Token]) -> Result<Command, CLIError> {
    let file =
        extract_positional(tokens, 0).ok_or_else(|| err_missing_arg("build", "file.w16h"))?;

    Ok(Command::new(CommandKind::Build).with_file(file))
}

fn parse_dbg(tokens: &[Token]) -> Result<Command, CLIError> {
    // Ожидаем: dbg <stage> <file.w16h>
    let stage_str = extract_positional(tokens, 0).ok_or_else(|| err_missing_arg("dbg", "stage"))?;

    let stage = parse_stage(&stage_str).ok_or_else(|| err_unknown_stage(&stage_str))?;

    let file = extract_positional(tokens, 1).ok_or_else(|| err_missing_arg("dbg", "file.w16h"))?;

    Ok(Command::new(CommandKind::Dbg(stage)).with_file(file))
}

// ---------------------------------------------------------------------------
// Разбор стадии дебага
// ---------------------------------------------------------------------------

fn parse_stage(s: &str) -> Option<DbgStage> {
    match s {
        "tokens" => Some(DbgStage::Tokens),
        "hir" => Some(DbgStage::Hir),
        "mir" => Some(DbgStage::Mir),
        "mir-opt" => Some(DbgStage::MirOpt),
        "bytecode" => Some(DbgStage::Bytecode),
        "full" => Some(DbgStage::Full),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Хелперы для работы с токенами
// ---------------------------------------------------------------------------

/// Возвращает n-й позиционный аргумент (0-индекс, субкоманда не считается).
fn extract_positional(tokens: &[Token], n: usize) -> Option<String> {
    tokens
        .iter()
        .filter(|t| matches!(t, Token::Positional(_)))
        .nth(n)
        .map(|t| t.as_str().to_owned())
}

/// Проверяет наличие короткого флага в потоке токенов.
fn has_short(tokens: &[Token], flag: &str) -> bool {
    tokens
        .iter()
        .any(|t| matches!(t, Token::ShortFlag(s) if s == flag))
}

/// Проверяет наличие длинного флага в потоке токенов.
fn has_long(tokens: &[Token], flag: &str) -> bool {
    tokens
        .iter()
        .any(|t| matches!(t, Token::LongFlag(s) if s == flag))
}

/// Возвращает значение длинного Key-Value флага, если он присутствует.
#[allow(dead_code)]
fn long_flag_value<'a>(tokens: &'a [Token], flag: &str) -> Option<&'a str> {
    let mut iter = tokens.iter().peekable();
    while let Some(tok) = iter.next()
        && matches!(tok, Token::LongFlag(s) if s == flag)
    {
        if let Some(Token::FlagValue(val)) = iter.peek() {
            return Some(val.as_str());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Тесты
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::Tokenizer;

    fn parse_args(args: &[&str]) -> Result<Command, CLIError> {
        let argv: Vec<String> = std::iter::once("w16")
            .chain(args.iter().copied())
            .map(str::to_owned)
            .collect();
        let tokens = Tokenizer::tokenize_argv(&argv);
        Parser::parse(&tokens)
    }

    #[test]
    fn test_version() {
        let cmd = parse_args(&["version"]).unwrap();
        assert_eq!(cmd.kind, CommandKind::Version);
    }

    #[test]
    fn test_help_empty() {
        let cmd = parse_args(&[]).unwrap();
        assert_eq!(cmd.kind, CommandKind::Help);
    }

    #[test]
    fn test_run_missing_file() {
        let err = parse_args(&["run"]).unwrap_err();
        assert!(matches!(
            err.kind,
            crate::error::CliErrorKind::MissingArgument { .. }
        ));
    }

    #[test]
    fn test_run_show_time() {
        // Инжектируем токены напрямую — без реального файла на диске.
        let tokens = vec![
            Token::SubCommand("run".into()),
            Token::Positional("main.w16h".into()),
            Token::LongFlag("--time".into()),
        ];
        let cmd = Parser::parse(&tokens).unwrap();
        assert!(cmd.flags.show_time);
    }

    #[test]
    fn test_run_conflicting_flags() {
        let tokens = vec![
            Token::SubCommand("run".into()),
            Token::Positional("fake.w16h".into()),
            Token::ShortFlag("-i".into()),
            Token::ShortFlag("-j".into()),
        ];
        let err = Parser::parse(&tokens).unwrap_err();
        assert!(matches!(
            err.kind,
            crate::error::CliErrorKind::ConflictingFlags(..)
        ));
    }

    #[test]
    fn test_dbg_valid_stage() {
        let tokens = vec![
            Token::SubCommand("dbg".into()),
            Token::Positional("full".into()),
            Token::Positional("main.w16h".into()),
        ];
        let cmd = Parser::parse(&tokens).unwrap();
        assert_eq!(cmd.kind, CommandKind::Dbg(DbgStage::Full));
    }

    #[test]
    fn test_dbg_unknown_stage() {
        let tokens = vec![
            Token::SubCommand("dbg".into()),
            Token::Positional("tokns".into()),
            Token::Positional("main.w16h".into()),
        ];
        let err = Parser::parse(&tokens).unwrap_err();
        assert!(matches!(
            err.kind,
            crate::error::CliErrorKind::UnknownStage(..)
        ));
    }

    #[test]
    fn test_unknown_command() {
        let err = parse_args(&["fly"]).unwrap_err();
        assert!(
            matches!(err.kind, crate::error::CliErrorKind::UnknownCommand(ref s) if s == "fly")
        );
    }
}

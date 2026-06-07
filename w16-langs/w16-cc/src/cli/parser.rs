// w16-langs\w16-cc\src\cli\parser.rs
//
//! Парсер аргументов: поток токенов -> [`Command`].

use super::cmd::{Command, CommandFlags, CommandKind, RunMode};
use super::error::{CLIError, err_conflicting_flags, err_missing_arg, err_unknown_command};
use super::tokenizer::Token;

pub struct Parser;

impl Parser {
    pub fn parse(tokens: &[Token]) -> Result<Command, CLIError> {
        let subcommand = match tokens.first() {
            Some(Token::SubCommand(s)) => s.as_str(),
            _ => return Ok(Command::new(CommandKind::Help)),
        };

        match subcommand {
            "compile" => parse_compile(tokens),
            "run" => parse_run(tokens),
            "emit-hir" => parse_emit_hir(tokens),
            "check" => parse_check(tokens),
            "version" | "--version" | "-v" => Ok(Command::new(CommandKind::Version)),
            "help"    | "--help"    | "-h" => Ok(Command::new(CommandKind::Help)),
            unknown => Err(err_unknown_command(unknown)),
        }
    }
}

// ---------------------------------------------------------------------------
// Парсеры команд
// ---------------------------------------------------------------------------

fn parse_compile(tokens: &[Token]) -> Result<Command, CLIError> {
    let file = extract_positional(tokens, 0)
        .ok_or_else(|| err_missing_arg("compile", "file.c"))?;

    let out = long_flag_value(tokens, "--out").map(str::to_owned);

    Ok(Command::new(CommandKind::Compile)
        .with_file(file)
        .with_flags(CommandFlags { out, ..Default::default() }))
}

fn parse_run(tokens: &[Token]) -> Result<Command, CLIError> {
    let file = extract_positional(tokens, 0)
        .ok_or_else(|| err_missing_arg("run", "file.c"))?;

    let wants_interp = has_short(tokens, "-i");
    let wants_jit = has_short(tokens, "-j");
    if wants_interp && wants_jit {
        return Err(err_conflicting_flags("-i", "-j"));
    }

    let run_mode = if wants_jit { RunMode::Jit } else { RunMode::Interpreter };
    let show_time = has_long(tokens, "--time");

    Ok(Command::new(CommandKind::Run)
        .with_file(file)
        .with_flags(CommandFlags { run_mode, show_time, out: None }))
}

fn parse_emit_hir(tokens: &[Token]) -> Result<Command, CLIError> {
    let file = extract_positional(tokens, 0)
        .ok_or_else(|| err_missing_arg("emit-hir", "file.c"))?;

    let out = long_flag_value(tokens, "--out").map(str::to_owned);

    Ok(Command::new(CommandKind::EmitHir)
        .with_file(file)
        .with_flags(CommandFlags { out, ..Default::default() }))
}

fn parse_check(tokens: &[Token]) -> Result<Command, CLIError> {
    let file = extract_positional(tokens, 0)
        .ok_or_else(|| err_missing_arg("check", "file.c"))?;

    Ok(Command::new(CommandKind::Check).with_file(file))
}

// ---------------------------------------------------------------------------
// Хелперы
// ---------------------------------------------------------------------------

fn extract_positional(tokens: &[Token], n: usize) -> Option<String> {
    tokens.iter()
        .filter(|t| matches!(t, Token::Positional(_)))
        .nth(n)
        .map(|t| t.as_str().to_owned())
}

fn has_short(tokens: &[Token], flag: &str) -> bool {
    tokens.iter().any(|t| matches!(t, Token::ShortFlag(s) if s == flag))
}

fn has_long(tokens: &[Token], flag: &str) -> bool {
    tokens.iter().any(|t| matches!(t, Token::LongFlag(s) if s == flag))
}

fn long_flag_value<'a>(tokens: &'a [Token], flag: &str) -> Option<&'a str> {
    let mut iter = tokens.iter().peekable();
    while let Some(tok) = iter.next() {
        if matches!(tok, Token::LongFlag(s) if s == flag) {
            if let Some(Token::FlagValue(val)) = iter.peek() {
                return Some(val.as_str());
            }
        }
    }
    None
}
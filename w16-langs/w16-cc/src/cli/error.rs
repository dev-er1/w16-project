// w16-langs\w16-cc\src\cli\error.rs
//
//! Единая система ошибок CLI.
use std::fmt;
use super::cmd::COMMANDS;

// ---------------------------------------------------------------------------
// Расстояние Левенштейна
// ---------------------------------------------------------------------------

pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in 0..=m { dp[i][0] = i; }
    for j in 0..=n { dp[0][j] = j; }
    for i in 1..=m {
        for j in 1..=n {
            dp[i][j] = if a[i-1] == b[j-1] {
                dp[i-1][j-1]
            } else {
                1 + dp[i-1][j-1].min(dp[i-1][j]).min(dp[i][j-1])
            };
        }
    }
    dp[m][n]
}

pub fn did_you_mean<'a>(input: &str, candidates: &[&'a str], threshold: usize) -> Option<&'a str> {
    candidates.iter()
        .map(|&c| (c, levenshtein(input, c)))
        .filter(|(_, d)| *d <= threshold)
        .min_by_key(|(_, d)| *d)
        .map(|(c, _)| c)
}

// ---------------------------------------------------------------------------
// Диагностика
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticLevel { Note, Hint }

impl fmt::Display for DiagnosticLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Note => write!(f, "note"),
            Self::Hint => write!(f, "hint"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub message: String,
}

impl Diagnostic {
    pub fn note(msg: impl Into<String>) -> Self { Self { level: DiagnosticLevel::Note, message: msg.into() } }
    pub fn hint(msg: impl Into<String>) -> Self { Self { level: DiagnosticLevel::Hint, message: msg.into() } }
}

// ---------------------------------------------------------------------------
// Вид ошибки
// ---------------------------------------------------------------------------

#[warn(dead_code, unused)]
#[derive(Debug, Clone)]
pub enum CliErrorKind {
    UnknownCommand(String),
    MissingArgument { cmd: &'static str, what: &'static str },
    ConflictingFlags(String, String),
    FileNotFound(String),
    UnknownStage(String),
    /// Ошибка фронтенда компилятора (lexer / parser / semantic).
    CompilerError(String),
    /// Ошибка трансляции или кодогенерации.
    CodegenError(String),
    /// Ошибка I/O (чтение/запись файла).
    Io(String),
    /// Ошибка рантайма W16.
    Runtime(String),
}

impl fmt::Display for CliErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownCommand(c) => write!(f, "unknown command `{c}`"),
            Self::MissingArgument { cmd, what } => write!(f, "command `{cmd}` requires an argument: {what}"),
            Self::ConflictingFlags(a, b) => write!(f, "conflicting flags `{a}` and `{b}`"),
            Self::FileNotFound(p) => write!(f, "file not found: `{p}`"),
            Self::UnknownStage(s) => write!(f, "unknown stage `{s}`"),
            Self::CompilerError(m) => write!(f, "compiler error: {m}"),
            Self::CodegenError(m) => write!(f, "codegen error: {m}"),
            Self::Io(m) => write!(f, "I/O error: {m}"),
            Self::Runtime(m) => write!(f, "runtime error: {m}"),
        }
    }
}

// ---------------------------------------------------------------------------
// CLIError
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CLIError {
    pub kind: CliErrorKind,
    pub diagnostics: Vec<Diagnostic>,
    pub usage: Option<&'static str>,
}

impl CLIError {
    pub fn new(kind: CliErrorKind) -> Self {
        Self { kind, diagnostics: Vec::new(), usage: None }
    }
    pub fn with_note(mut self, msg: impl Into<String>) -> Self {
        self.diagnostics.push(Diagnostic::note(msg)); self
    }
    pub fn with_hint(mut self, msg: impl Into<String>) -> Self {
        self.diagnostics.push(Diagnostic::hint(msg)); self
    }
    pub fn with_usage_of(mut self, cmd_name: &str) -> Self {
        self.usage = COMMANDS.iter().find(|c| c.name == cmd_name).map(|c| c.usage);
        self
    }

    /// Вывести полный отчёт в stderr. Не завершает процесс.
    pub fn report(&self) {
        eprintln!("{BOLD}{RED}error{RESET}: {}", self.kind);
        for d in &self.diagnostics {
            let color = match d.level {
                DiagnosticLevel::Note => YELLOW,
                DiagnosticLevel::Hint => CYAN,
            };
            eprintln!("  {BOLD}{color}{}{RESET}: {}", d.level, d.message);
        }
        if let Some(usage) = self.usage {
            eprintln!("  {BOLD}{GREY}usage{RESET}: {usage}");
        }
    }
}

impl fmt::Display for CLIError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)
    }
}

// ANSI
const BOLD: &str = "\x1b[1m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";
const CYAN: &str = "\x1b[36m";
const GREY: &str = "\x1b[90m";
const RESET: &str = "\x1b[0m";

// ---------------------------------------------------------------------------
// Конструкторы
// ---------------------------------------------------------------------------

fn command_names() -> Vec<&'static str> {
    COMMANDS.iter().map(|c| c.name).collect()
}

pub fn err_unknown_command(cmd: impl Into<String>) -> CLIError {
    let name = cmd.into();
    let names = command_names();
    let mut err = CLIError::new(CliErrorKind::UnknownCommand(name.clone()));
    if let Some(s) = did_you_mean(&name, &names, 2) {
        err = err.with_hint(format!("did you mean `{s}`?"));
    }
    err.with_hint("run `w16cc help` to see available commands")
}

pub fn err_missing_arg(cmd: &'static str, what: &'static str) -> CLIError {
    CLIError::new(CliErrorKind::MissingArgument { cmd, what }).with_usage_of(cmd)
}

pub fn err_conflicting_flags(a: &str, b: &str) -> CLIError {
    CLIError::new(CliErrorKind::ConflictingFlags(a.into(), b.into()))
        .with_note("only one execution mode can be active at a time")
        .with_usage_of("run")
}

pub fn err_file_not_found(path: impl Into<String>) -> CLIError {
    let p = path.into();
    CLIError::new(CliErrorKind::FileNotFound(p.clone()))
        .with_hint(format!("check that `{p}` exists and the path is correct"))
}
// w16-cli\src\error.rs
//
//! Модель ошибок.
//!
//! `CLIError` — единственный публичный тип ошибки для всего CLI.
//! Содержит типизированный `CliErrorKind`, ноль или больше `Diagnostic`-ов
//! (заметки / подсказки), и опциональную строку `usage` из реестра `COMMANDS`.

use std::fmt;
use crate::cmd::COMMANDS;

// ---------------------------------------------------------------------------
// Расстояние Левенштейна
// ---------------------------------------------------------------------------

/// Вычисляет редакционное расстояние между двумя строками.
/// Используется для подсказок "did you mean X?".
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());

    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in 0..=m { dp[i][0] = i; }
    for j in 0..=n { dp[0][j] = j; }

    for i in 1..=m {
        for j in 1..=n {
            dp[i][j] = if a[i - 1] == b[j - 1] {
                dp[i - 1][j - 1]
            } else {
                1 + dp[i - 1][j - 1]  // замена
                    .min(dp[i - 1][j]) // удаление
                    .min(dp[i][j - 1]) // вставка
            };
        }
    }

    dp[m][n]
}

/// Возвращает ближайший кандидат из `candidates` к `input`,
/// если расстояние не превышает `threshold`. Иначе — `None`.
pub fn did_you_mean<'a>(input: &str, candidates: &[&'a str], threshold: usize) -> Option<&'a str> {
    candidates
        .iter()
        .map(|&c| (c, levenshtein(input, c)))
        .filter(|(_, d)| *d <= threshold)
        .min_by_key(|(_, d)| *d)
        .map(|(c, _)| c)
}

// ---------------------------------------------------------------------------
// Диагностика
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticLevel {
    /// Поясняет почему возникла ошибка.
    Note,
    /// Подсказывает как её исправить.
    Hint,
}

impl fmt::Display for DiagnosticLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiagnosticLevel::Note => write!(f, "note"),
            DiagnosticLevel::Hint => write!(f, "hint"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub message: String,
}

impl Diagnostic {
    pub fn note(msg: impl Into<String>) -> Self {
        Self { level: DiagnosticLevel::Note, message: msg.into() }
    }

    pub fn hint(msg: impl Into<String>) -> Self {
        Self { level: DiagnosticLevel::Hint, message: msg.into() }
    }
}

// ---------------------------------------------------------------------------
// Вид ошибки
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum CliErrorKind {
    /// Пользователь ввёл команду, которую CLI не знает.
    UnknownCommand(String),

    /// Не передан обязательный позиционный аргумент.
    MissingArgument { cmd: &'static str, what: &'static str },

    /// Одновременно переданы взаимоисключающие флаги.
    ConflictingFlags(String, String),

    /// Указанный путь не существует на диске.
    FileNotFound(String),

    /// Неизвестная стадия дебага.
    UnknownStage(String),

    /// Ошибка, пришедшая из рантайма w16.
    Runtime(String),
}

impl fmt::Display for CliErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliErrorKind::UnknownCommand(cmd) =>
                write!(f, "unknown command `{cmd}`"),
            CliErrorKind::MissingArgument { cmd, what } =>
                write!(f, "command `{cmd}` requires an argument: {what}"),
            CliErrorKind::ConflictingFlags(a, b) =>
                write!(f, "conflicting flags `{a}` and `{b}`"),
            CliErrorKind::FileNotFound(path) =>
                write!(f, "file not found: `{path}`"),
            CliErrorKind::UnknownStage(stage) =>
                write!(f, "unknown debug stage `{stage}`"),
            CliErrorKind::Runtime(msg) =>
                write!(f, "{msg}"),
        }
    }
}

// ---------------------------------------------------------------------------
// CLIError — единственный публичный тип
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CLIError {
    pub kind: CliErrorKind,
    /// Ноль или больше заметок / подсказок, выводятся под основным сообщением.
    pub diagnostics: Vec<Diagnostic>,
    /// Строка использования из реестра `COMMANDS`, выводится последней.
    pub usage: Option<&'static str>,
}

impl CLIError {
    pub fn new(kind: CliErrorKind) -> Self {
        Self { kind, diagnostics: Vec::new(), usage: None }
    }

    pub fn with_note(mut self, msg: impl Into<String>) -> Self {
        self.diagnostics.push(Diagnostic::note(msg));
        self
    }

    pub fn with_hint(mut self, msg: impl Into<String>) -> Self {
        self.diagnostics.push(Diagnostic::hint(msg));
        self
    }

    /// Найти строку `usage` для команды `cmd_name` в реестре `COMMANDS` и прикрепить её.
    pub fn with_usage_of(mut self, cmd_name: &str) -> Self {
        self.usage = COMMANDS.iter()
            .find(|c| c.name == cmd_name)
            .map(|c| c.usage);
        self
    }

    /// Вывести полный отчёт об ошибке в stderr. **Не завершает процесс.**
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

// ANSI-цвета — приватны для этого модуля.
const BOLD: &str = "\x1b[1m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";
const CYAN: &str = "\x1b[36m";
const GREY: &str = "\x1b[90m";
const RESET: &str = "\x1b[0m";

// ---------------------------------------------------------------------------
// Конструкторы-хелперы
// ---------------------------------------------------------------------------

/// Генерирует срез имён команд из реестра — чтобы не дублировать вручную.
fn command_names() -> Vec<&'static str> {
    COMMANDS.iter().map(|c| c.name).collect()
}

/// Допустимые стадии дебага — для подсказок.
const DBG_STAGES: &[&str] = &["tokens", "hir", "mir", "mir-opt", "bytecode", "full"];

pub fn err_unknown_command(cmd: impl Into<String>) -> CLIError {
    let name = cmd.into();
    let names = command_names();
    let mut err = CLIError::new(CliErrorKind::UnknownCommand(name.clone()));

    if let Some(suggestion) = did_you_mean(&name, &names, 2) {
        err = err.with_hint(format!("did you mean `{suggestion}`?"));
    }

    err.with_hint("run `w16 help` to see available commands")
}

pub fn err_missing_arg(cmd: &'static str, what: &'static str) -> CLIError {
    CLIError::new(CliErrorKind::MissingArgument { cmd, what })
        .with_usage_of(cmd)
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

pub fn err_unknown_stage(stage: impl Into<String>) -> CLIError {
    let name = stage.into();
    let mut err = CLIError::new(CliErrorKind::UnknownStage(name.clone()));

    if let Some(suggestion) = did_you_mean(&name, DBG_STAGES, 2) {
        err = err.with_hint(format!("did you mean `{suggestion}`?"));
    }

    err.with_hint(format!(
        "available stages: {}",
        DBG_STAGES.join(", ")
    ))
    .with_usage_of("dbg")
}

// ---------------------------------------------------------------------------
// Тесты
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein_exact() {
        assert_eq!(levenshtein("run", "run"), 0);
    }

    #[test]
    fn test_levenshtein_one_edit() {
        assert_eq!(levenshtein("rn", "run"), 1);
        assert_eq!(levenshtein("bild", "build"), 1);
    }

    #[test]
    fn test_did_you_mean_hit() {
        let names = command_names();
        assert_eq!(did_you_mean("rn",   &names, 2), Some("run"));
        assert_eq!(did_you_mean("buid", &names, 2), Some("build"));
    }

    #[test]
    fn test_did_you_mean_miss() {
        let names = command_names();
        assert_eq!(did_you_mean("zzz", &names, 2), None);
    }

    #[test]
    fn test_unknown_stage_suggests() {
        let err = err_unknown_stage("tokns");
        let hints: Vec<_> = err.diagnostics.iter()
            .filter(|d| d.level == DiagnosticLevel::Hint)
            .map(|d| d.message.as_str())
            .collect();
        assert!(hints.iter().any(|h| h.contains("tokens")));
    }

    #[test]
    fn test_missing_arg_has_usage() {
        let err = err_missing_arg("run", "file.w16h");
        assert!(err.usage.is_some());
    }

    #[test]
    fn test_conflicting_flags_has_usage() {
        let err = err_conflicting_flags("-i", "-j");
        assert!(err.usage.is_some());
    }
}
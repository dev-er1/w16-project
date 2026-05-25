// w16-cli\src\cmd.rs
//
//! Модель команды.
//!
//! Всё что нужно знать остальным модулям CLI о разобранной команде — здесь.
//! Никакой логики разбора аргументов, никакого I/O — только данные.
 
// ---------------------------------------------------------------------------
// Реестр команд
// ---------------------------------------------------------------------------
 
/// Одна запись в глобальном реестре команд.
pub struct CommandUsage {
    pub name: &'static str,
    pub args: &'static str,
    pub description: &'static str,
    /// Полная строка использования, выводится при неправильном вызове команды.
    pub usage: &'static str,
    pub flags: &'static [FlagUsage],
}
 
/// Один флаг, привязанный к команде.
pub struct FlagUsage {
    pub flag: &'static str,
    pub description: &'static str,
}
 
/// Глобальный реестр всех команд CLI.
///
/// `help` итерирует этот срез — ничего не захардкожено в тексте.
pub const COMMANDS: &[CommandUsage] = &[
    CommandUsage {
        name: "run",
        args: "<file>",
        description: "Run a file",
        usage: "w16 run <file> [-i | -j] [--time]",
        flags: &[
            FlagUsage { flag: "-i", description: "Interpreter / VM mode (default)" },
            FlagUsage { flag: "-j", description: "JIT-compilation mode" },
            FlagUsage { flag: "--time", description: "Print execution time after run"  },
        ],
    },
    CommandUsage {
        name: "build",
        args: "<file>",
        description: "Compile a file",
        usage: "w16 build <file>",
        flags: &[],
    },
    CommandUsage {
        name: "dbg",
        args: "<stage> <file>",
        description: "Dump an internal compilation stage",
        usage: "w16 dbg <stage> <file>",
        flags: &[
            FlagUsage { flag: "tokens", description: "HIR token stream" },
            FlagUsage { flag: "hir", description: "HIR AST" },
            FlagUsage { flag: "mir",  description: "MIR AST (before optimizer)" },
            FlagUsage { flag: "mir-opt", description: "MIR AST (after optimizer)" },
            FlagUsage { flag: "bytecode",description: "Compiled bytecode instructions"},
            FlagUsage { flag: "full", description: "All stages combined" },
        ],
    },
    CommandUsage {
        name: "version",
        args: "",
        description: "Print the W16 runtime version",
        usage: "w16 version",
        flags: &[],
    },
    CommandUsage {
        name: "help",
        args: "",
        description: "Print this help message",
        usage: "w16 help",
        flags: &[],
    },
];
 
// ---------------------------------------------------------------------------
// Модель команды
// ---------------------------------------------------------------------------
 
/// Режим выполнения для команды `run`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RunMode {
    /// Интерпретатор / VM (по умолчанию).
    #[default]
    Interpreter,
    /// JIT-компиляция.
    Jit,
}
 
/// Стадия дебага для команды `dbg`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbgStage {
    /// Поток токенов HIR.
    Tokens,
    /// HIR AST.
    Hir,
    /// MIR AST до оптимизатора.
    Mir,
    /// MIR AST после оптимизатора.
    MirOpt,
    /// Скомпилированный bytecode.
    Bytecode,
    /// Все стадии подряд.
    Full,
}
 
/// Опциональные флаги команды, которые не помещаются в `CommandKind`.
#[derive(Debug, Clone, Default)]
pub struct CommandFlags {
    pub run_mode: RunMode,
    /// Вывести время выполнения после запуска.
    pub show_time: bool,
}
 
/// Дискриминант — что пользователь хочет сделать.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandKind {
    Run,
    Build,
    /// Дамп внутренней стадии компиляции.
    Dbg(DbgStage),
    Version,
    Help
}

/// Полностью разобранная и валидированная команда, готовая к исполнению.
///
/// ```text
/// w16 run main.w16h -j
///     ^^^     ↑     /|\   -> kind = CommandKind::Run
///         ^^^^^^^^^  |    -> file = Some("main.w16h")
///                   ^^^   -> flags = CommandFlags { run_mode: Jit }
/// ```
#[derive(Debug, Clone)]
pub struct Command {
    pub kind: CommandKind,
    /// Путь к входному файлу, если команда его требует.
    pub file: Option<String>,
    pub flags: CommandFlags,
}

impl Command {
    pub fn new(kind: CommandKind) -> Self {
        Self { kind, file: None, flags: CommandFlags::default() }
    }

    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    pub fn with_flags(mut self, flags: CommandFlags) -> Self {
        self.flags = flags;
        self
    }
}
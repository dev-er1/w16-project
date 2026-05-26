// example-langs\w16-cc\src\cli\cmd.rs
//
//! Модель команды w16cc CLI.

// ---------------------------------------------------------------------------
// Реестр команд
// ---------------------------------------------------------------------------

pub struct CommandUsage {
    pub name: &'static str,
    pub args: &'static str,
    pub description: &'static str,
    pub usage: &'static str,
    pub flags: &'static [FlagUsage],
}

pub struct FlagUsage {
    pub flag: &'static str,
    pub description: &'static str,
}

pub const COMMANDS: &[CommandUsage] = &[
    CommandUsage {
        name: "compile",
        args: "<file>",
        description: "Compile a C file to a native executable",
        usage: "w16cc compile <file> [--out <name>]",
        flags: &[
            FlagUsage { flag: "--out", description: "Output file name (default: input stem)" },
        ],
    },
    CommandUsage {
        name: "run",
        args: "<file>",
        description: "Compile and run a C file",
        usage: "w16cc run <file> [-i | -j]",
        flags: &[
            FlagUsage { flag: "-i", description: "Interpreter / VM mode (default)" },
            FlagUsage { flag: "-j", description: "JIT-compilation mode"            },
            FlagUsage { flag: "--time", description: "Print execution time"            },
        ],
    },
    CommandUsage {
        name: "emit-hir",
        args: "<file>",
        description: "Compile C code to W16-HIR text and print to stdout",
        usage: "w16cc emit-hir <file.c> [--out <file.w16h>]",
        flags: &[
            FlagUsage { flag: "--out", description: "Write HIR to file instead of stdout" },
        ],
    },
    CommandUsage {
        name: "check",
        args: "<file>",
        description: "Parse and type-check a C file without generating code",
        usage: "w16cc check <file>",
        flags: &[],
    },
    CommandUsage {
        name: "version",
        args: "",
        description: "Print W16CC version",
        usage: "w16cc version",
        flags: &[],
    },
    CommandUsage {
        name: "help",
        args: "",
        description: "Print this help message",
        usage: "w16cc help",
        flags: &[],
    },
];

// ---------------------------------------------------------------------------
// Модель команды
// ---------------------------------------------------------------------------

/// Режим выполнения для `run`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RunMode {
    #[default]
    Interpreter,
    Jit,
}

/// Флаги общие для нескольких команд.
#[derive(Debug, Clone, Default)]
pub struct CommandFlags {
    pub run_mode: RunMode,
    pub show_time: bool,
    /// Путь к выходному файлу (`--out`).
    pub out: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandKind {
    /// Скомпилировать в нативный исполняемый файл.
    Compile,
    /// Скомпилировать и запустить.
    Run,
    /// Сгенерировать текстовый HIR.
    EmitHir,
    /// Только проверка (парсинг + семантика).
    Check,
    Version,
    Help,
}

/// Полностью разобранная команда.
#[derive(Debug, Clone)]
pub struct Command {
    pub kind: CommandKind,
    /// Входной C-файл (если нужен командой).
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
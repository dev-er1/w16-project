//! example-lanmgs\src\cli_error.rs
//!
//! # Ошибки CLI
//!
//! Ошибки парсинга аргументов, и всего что не связано с ядром языка

pub enum CLIErrors {
    ItsADir(String),
    UnknownCommand(String),
    ErrorWhileReadingFile(String),
}

pub fn report_cli_error(error: &CLIErrors) {
    let normal_error_message = match error {
        CLIErrors::ItsADir(path) => format!("'{path}' is dir, not file"),
        CLIErrors::UnknownCommand(command) => format!("unknown command: '{command}'"),
        CLIErrors::ErrorWhileReadingFile(err) => format!("error while reading file: '{err}'"),
    };
    println!("\x1b[31m\x1b[1mCLI Error\x1b[0m: \x1b[1m{normal_error_message}\x1b[0m");
    println!();
}

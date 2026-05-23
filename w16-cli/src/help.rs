// w16-cli\src\help.rs
//
//! Рендерер справки.
//!
//! Итерирует [`crate::cmd::COMMANDS`] — никакого хардкода в тексте.

use crate::cmd::COMMANDS;

pub fn print() {
    println!("CLI for \x1b[1mW16\x1b[0m\n");
    println!("\x1b[1mUsage\x1b[0m:");
    println!("    w16 <command> [args] [flgs]\n");
    println!("Commands:");

    for entry in COMMANDS {
        let args_part = if entry.args.is_empty() {
            String::new()
        } else {
            format!(" {}", entry.args)
        };

        println!("    {:<8}{args_part:<18}{}", entry.name, entry.description);

        for flag in entry.flags {
            println!("        {:<16}{}", flag.flag, flag.description);
        }
    }
}
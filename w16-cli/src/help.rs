// w16-cli\src\help.rs
//
//! Рендерер справки.
//!
//! Итерирует [`crate::cmd::COMMANDS`] — никакого хардкода в тексте.

use crate::cmd::COMMANDS;

pub fn print() {
    println!("CLI for \x1b[1mW16\x1b[0m\n");
    println!("\x1b[1;4mUsage\x1b[0m:");
    println!("    w16 <command> [args] [flgs]\n");
    println!("\x1b[1;4mCommands\x1b[0m:");

    for entry in COMMANDS {
        let args_part = if entry.args.is_empty() {
            String::new()
        } else {
            format!(" {}", entry.args)
        };

        println!(
            "    \x1b[1;36m{:<8}\x1b[0m{args_part:<18}{}",
            entry.name, entry.description
        );

        for flag in entry.flags {
            println!(
                "        \x1b[2;3m{:<22}\x1b[0m\x1b[38;5;232m{}\x1b[0m",
                flag.flag, flag.description
            );
        }
    }
}

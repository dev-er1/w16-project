// example-langs\w16-cc\src\cli\help.rs
//
//! Рендерер справки — итерирует COMMANDS, никакого хардкода.

use super::cmd::COMMANDS;

// ANSI
const BOLD: &str = "\x1b[1m";
const GREEN: &str = "\x1b[32m";
const CYAN: &str = "\x1b[36m";
const YELLOW: &str = "\x1b[33m";
const GREY: &str = "\x1b[90m";
const RESET: &str = "\x1b[0m";

pub fn print() {
    println!("{BOLD}w16cc{RESET} — W16 C11 compiler\n");

    println!("{BOLD}Usage:{RESET}");
    println!("    {CYAN}w16cc{RESET} {GREEN}<command>{RESET} {YELLOW}[arguments]{RESET} {GREY}[flags]{RESET}\n");

    println!("{BOLD}Commands:{RESET}");
    for entry in COMMANDS {
        let args_part = if entry.args.is_empty() {
            String::new()
        } else {
            format!(" {YELLOW}{}{RESET}", entry.args)
        };
        println!(
            "    {CYAN}{:<10}{RESET}{args_part:<28}{}",
            entry.name,
            entry.description,
        );
        for flag in entry.flags {
            println!(
                "        {GREY}{:<16}{RESET}{}",
                flag.flag,
                flag.description,
            );
        }
    }
}
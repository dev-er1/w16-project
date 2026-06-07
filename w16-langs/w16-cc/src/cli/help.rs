// w16-langs\w16-cc\src\cli\help.rs
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
    println!("{BOLD}wcc{RESET} — W16 C11 compiler\n");

    println!("{BOLD}Usage:{RESET}");
    println!("    {CYAN}wcc{RESET} {GREEN}<command>{RESET} {YELLOW}[arguments]{RESET} {GREY}[flags]{RESET}\n");

    println!("{BOLD}Commands:{RESET}");
    for entry in COMMANDS {
        // Шаг 1: Форматируем часть с аргументами БЕЗ цветов, чтобы точно знать её видимую длину
        let raw_args = if entry.args.is_empty() {
            String::new()
        } else {
            format!(" {}", entry.args)
        };
        
        let colored_args = if entry.args.is_empty() {
            String::new()
        } else {
            format!(" {YELLOW}{}{RESET}", entry.args)
        };

        // Вычисляем, сколько пробелов нужно добавить до колонки описания.
        // 24 — это желаемый отступ для описания от начала строки (4 пробела отступа + 20 на команду/аргументы)
        let padding = 24_usize.saturating_sub(entry.name.len() + raw_args.len());
        
        println!(
            "    {CYAN}{}{RESET}{}{:.<30}",
            entry.name,
            colored_args,
            format!("{:>width$}{}", "", entry.description, width = padding)
        );

        // Шаг 4: Выравнивание флагов
        for flag in entry.flags {
            // Флаги выравниваем аналогично: сначала считаем отступ без учета ANSI-кодов
            let flag_padding = 20_usize.saturating_sub(flag.flag.len());
            
            println!(
                "        {GREY}\x1b[3m{}{RESET}{:width$}{GREY}{}{RESET}",
                flag.flag,
                "",
                flag.description,
                width = flag_padding
            );
        }
    }
}
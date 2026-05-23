//! CLI всего языка karbon
use std::{fs, path::PathBuf};

use karbon::argparser::main_parser::Clapo;
use karbon::{cli_error, run_with_debug};

const VERSION: &str = "0.0.1";

fn main() {
    let args = match Clapo::parse() {
        Some(v) => v,
        None => {
            print_help();
            return;
        }
    };
    match args.subcommand.as_deref() {
        Some(path) => {
            let mut file = PathBuf::new();
            file.push(path);
            if !file.is_file() && file.exists() {
                cli_error::report_cli_error(&cli_error::CLIErrors::ItsADir(path.to_string()));
            }
            let code = match fs::read_to_string(file) {
                Ok(code) => code,
                Err(error) => {
                    cli_error::report_cli_error(&cli_error::CLIErrors::ErrorWhileReadingFile(
                        error.to_string(),
                    ));
                    return;
                }
            };
            if !args.switches.is_empty() {
                match args.switches[0].as_str() {
                    "--debug" => {
                        let _ = run_with_debug(code.as_str());
                    }
                    _ => print_help(),
                }
            }
        }
        None => print_help(),
    }
}
fn print_help() {
    println!("\x1b[1mKarbon\x1b[0m v{VERSION}");
    println!("Usage: karbon.exe [\x1b[97moption\x1b[0m]");
    println!("\n\x1b[97mOptions\x1b[0m:");
    println!("  [file] \x1b[2m--debug\x1b[0m == Run file \x1b[4mwith debug mode\x1b[0m");
    println!("  [file] == Run file");
    println!("   repl == Activate the REPL-mode")
}

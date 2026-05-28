// w16-cli\src\main.rs
//
//! Точка входа. Владеет кодом выхода — больше ничего.

mod cmd;
mod error;
mod executer;
mod help;
mod parser;
mod tokenizer;

use executer::Executer;
use parser::Parser;
use tokenizer::Tokenizer;

fn main() {
    let tokens = Tokenizer::tokenize();

    let command = Parser::parse(&tokens).unwrap_or_else(|e| {
        e.report();
        std::process::exit(1);
    });

    if let Err(e) = Executer::execute(command) {
        e.report();
    }
}

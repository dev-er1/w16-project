//! CLI компилятора W16-CC.
//!
//! W16-CC транслирует C11 в W16-HIR и позволяет сразу запускать
//! результат через VM или JIT, либо сохранять как текстовый `.w16h`.

mod cli {
    pub mod cmd;
    pub mod error;
    pub mod executer;
    pub mod help;
    pub mod tokenizer;
    pub mod parser;
}

use cli::{
    executer::Executer,
    parser::Parser,
    tokenizer::Tokenizer,
};

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
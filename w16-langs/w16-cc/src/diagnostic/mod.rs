//! Rule-based модель для диагностик.
/*use crate::frontend::{error::{Error, WhichError}, lexer::{lexer_error::{LexerError, LexerErrors}, token::Token}};

pub struct DiagnosticModel {
    pub tokenized_code: Vec<Token>,
    pub error: Error
}

pub struct DiagnosticSolution {
    pub final_code: Vec<Token>
}

impl DiagnosticModel {
    pub fn new(tokenized_code: Vec<Token>, error: Error) -> Self {
        Self { tokenized_code, error }
    }

    pub fn get_solution(&self) -> DiagnosticSolution {
        match self.error.error {
            WhichError::LexerError(v) => {
                match v.error {
                    LexerErrors::UnexpectedChar(v) => {
                        /* TODO */
                    }
                }
            }
            _ => unimplemented!()
        }
    }
}*/
pub mod types;
pub mod value;
pub mod frontend;
pub mod diagnostic;
pub mod codegen;

pub use frontend::W16CFrontend;
use w16_lib::RunResult;

pub struct W16C<'a> {
    pub src: &'a str
}

impl <'a>W16C<'a> {
    pub fn new(src: &'a str) -> Self {
        Self { src }
    }

    pub fn execute_code_by_vm(&self) -> RunResult {
        let mut c_frontend = W16CFrontend::new(self.src);

        let binding = c_frontend.compile_all();
        let ast = match &binding {
            Ok(ast) => ast,
            Err(e) => {
                for error in e {
                    error.report_error(self.src);
                }
                panic!("\x1b[1;31mSome error\x1b[0m.");
            }
        };

        let mut w16_translator = codegen::AstTranslator::new(&c_frontend.string_table);

        let w16_hir = match w16_translator.translate(ast, "main") {
            Ok(module) => module,
            Err(fucking_err) => {
                println!("{fucking_err:?}");
                panic!("\x1b[1;31mSome error\x1b[0m.");
            }
        };

        let result = match w16_lib::run_hir_ast(&w16_hir) {
            Ok(win) => win,
            Err(e) => {
                println!("{e}");
                panic!("\x1b[1;31mSome error\x1b[0m.");
            }
        };

        result
    }
}
// example-langs\w16-cc\tests\codegen\errors.rs
//
//! Тесты что фронтенд ловит ошибки до кодогенерации.
//! Каждый тест проверяет что компиляция завершается с ошибкой.

use w16_cc::W16CFrontend;
use w16_cc::codegen::AstTranslator;

fn compile_err(src: &str) {
    let mut frontend = W16CFrontend::new(src);
    // Если фронтенд пропустил — пробуем codegen
    match frontend.compile_all() {
        Err(_) => return, // фронтенд поймал — ок
        Ok(ast) => {
            // Если codegen тоже не поймал — тест провалился
            let result = AstTranslator::new(&frontend.string_table)
                .translate(&ast, "test");
            match result {
                Err(_) => {} // codegen поймал — ок
                Ok(module) => {
                    // Проверяем HIR семантику как последний рубеж
                    if w16_ir::semantic::verify_hir_module(&module).is_ok() {
                        panic!("expected compilation error, but all stages passed for:\n{src}");
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Ошибки парсера
// ---------------------------------------------------------------------------

#[test]
fn test_missing_semicolon() {
    compile_err("int main() { int x = 0 }");
}

#[test]
fn test_missing_closing_brace() {
    compile_err("int main() {");
}

#[test]
fn test_missing_closing_paren() {
    compile_err("int main( { return 0; }");
}

// ---------------------------------------------------------------------------
// Ошибки семантики (resolver)
// ---------------------------------------------------------------------------

#[test]
fn test_undeclared_variable() {
    compile_err("int main() { return x; }");
}

#[test]
fn test_undeclared_function() {
    compile_err("int main() { return foo(); }");
}

#[test]
fn test_redeclared_variable_same_scope() {
    compile_err("int main() { int x = 0; int x = 1; return x; }");
}

#[test]
fn test_break_outside_loop() {
    compile_err("int main() { break; return 0; }");
}

#[test]
fn test_continue_outside_loop() {
    compile_err("int main() { continue; return 0; }");
}

// ---------------------------------------------------------------------------
// goto не поддерживается транслятором
// ---------------------------------------------------------------------------

#[test]
fn test_goto_not_supported() {
    // goto проходит фронтенд, но транслятор должен вернуть ошибку
    let src = "int main() { goto end; end: return 0; }";
    let mut frontend = W16CFrontend::new(src);
    if let Ok(ast) = frontend.compile_all() {
        let result = AstTranslator::new(&frontend.string_table)
            .translate(&ast, "test");
        assert!(result.is_err(), "goto should fail in translator");
    }
    // Если фронтенд поймал — тоже ок
}
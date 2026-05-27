// example-langs\w16-cc\tests\codegen\arithmetic.rs
//
//! Тесты арифметики и литералов через весь пайплайн C -> HIR -> run.

use w16_cc::W16CFrontend;
use w16_cc::codegen::AstTranslator;
use w16_ir::hir::Module;

// ---------------------------------------------------------------------------
// Хелперы
// ---------------------------------------------------------------------------

/// Прогоняет C-код через фронтенд и транслятор, возвращает HIR-модуль.
fn compile(src: &str) -> Module {
    let mut frontend = W16CFrontend::new(src);
    let ast = frontend.compile_all()
        .unwrap_or_else(|errs| {
            panic!("frontend errors:\n{}", errs.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n"));
        });
    AstTranslator::new(&frontend.string_table)
        .translate(&ast, "test")
        .unwrap_or_else(|e| panic!("codegen error: {}", e.message))
}

/// Компилирует и проверяет что HIR-семантика не выдаёт ошибок.
fn compile_ok(src: &str) -> Module {
    let module = compile(src);
    w16_ir::semantic::verify_hir_module(&module)
        .unwrap_or_else(|errs| {
            panic!("HIR semantic errors:\n{}", errs.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n"));
        });
    module
}

// ---------------------------------------------------------------------------
// Литералы
// ---------------------------------------------------------------------------

#[test]
fn test_return_int_literal() {
    compile_ok("int main() { return 42; }");
}

#[test]
fn test_return_zero() {
    compile_ok("int main() { return 0; }");
}

#[test]
fn test_return_negative() {
    compile_ok("int main() { return -1; }");
}

#[test]
fn test_unsigned_literal() {
    compile_ok("unsigned int main() { return 67; }");
}

#[test]
fn test_local_int_var() {
    compile_ok("int main() { int x = 42; return x; }");
}

#[test]
fn test_local_var_zero_init() {
    compile_ok("int main() { int x = 0; return x; }");
}

// ---------------------------------------------------------------------------
// Арифметика
// ---------------------------------------------------------------------------

#[test]
fn test_addition() {
    compile_ok("int main() { int x = 1 + 2; return x; }");
}

#[test]
fn test_subtraction() {
    compile_ok("int main() { int x = 10 - 3; return x; }");
}

#[test]
fn test_multiplication() {
    compile_ok("int main() { int x = 6 * 7; return x; }");
}

#[test]
fn test_division() {
    compile_ok("int main() { int x = 10 / 2; return x; }");
}

#[test]
fn test_remainder() {
    compile_ok("int main() { int x = 10 % 3; return x; }");
}

#[test]
fn test_compound_arithmetic() {
    compile_ok("int main() { int x = 2 + 3 * 4; return x; }");
}

#[test]
fn test_compound_assign_add() {
    compile_ok("int main() { int x = 0; x += 5; return x; }");
}

#[test]
fn test_compound_assign_mul() {
    compile_ok("int main() { int x = 3; x *= 2; return x; }");
}

// ---------------------------------------------------------------------------
// Побитовые
// ---------------------------------------------------------------------------

#[test]
fn test_shift_left() {
    compile_ok("int main() { int x = 1 << 3; return x; }");
}

#[test]
fn test_shift_right() {
    compile_ok("int main() { int x = 8 >> 2; return x; }");
}

#[test]
fn test_bitwise_and() {
    compile_ok("int main() { int x = 0xff & 0x0f; return x; }");
}

#[test]
fn test_bitwise_or() {
    compile_ok("int main() { int x = 0x0f | 0xf0; return x; }");
}

// ---------------------------------------------------------------------------
// Инкремент / декремент
// ---------------------------------------------------------------------------

#[test]
fn test_pre_increment() {
    compile_ok("int main() { int x = 0; ++x; return x; }");
}

#[test]
fn test_post_increment() {
    compile_ok("int main() { int x = 0; x++; return x; }");
}

#[test]
fn test_pre_decrement() {
    compile_ok("int main() { int x = 5; --x; return x; }");
}

// ---------------------------------------------------------------------------
// Несколько переменных
// ---------------------------------------------------------------------------

#[test]
fn test_multiple_vars() {
    compile_ok("int main() { int a = 1; int b = 2; int c = a + b; return c; }");
}

#[test]
fn test_reassign() {
    compile_ok("int main() { int x = 1; x = 42; return x; }");
}
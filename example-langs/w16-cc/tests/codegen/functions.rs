// example-langs\w16-cc\tests\codegen\functions.rs
//
//! Тесты функций, вызовов и рекурсии.

use w16_cc::W16CFrontend;
use w16_cc::codegen::AstTranslator;
use w16_ir::hir::Module;

fn compile_ok(src: &str) -> Module {
    let mut frontend = W16CFrontend::new(src);
    let ast = frontend.compile_all()
        .unwrap_or_else(|errs| {
            panic!("frontend errors:\n{}", errs.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n"));
        });
    let module = AstTranslator::new(&frontend.string_table)
        .translate(&ast, "test")
        .unwrap_or_else(|e| panic!("codegen error: {}", e.message));
    w16_ir::semantic::verify_hir_module(&module)
        .unwrap_or_else(|errs| {
            panic!("HIR semantic errors:\n{}", errs.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n"));
        });
    module
}

// ---------------------------------------------------------------------------
// Простые функции
// ---------------------------------------------------------------------------

#[test]
fn test_void_function() {
    compile_ok("void noop() {} int main() { noop(); return 0; }");
}

#[test]
fn test_function_with_return() {
    compile_ok("int answer() { return 42; } int main() { return answer(); }");
}

#[test]
fn test_function_one_param() {
    compile_ok("int double_it(int x) { return x + x; } int main() { return double_it(5); }");
}

#[test]
fn test_function_two_params() {
    compile_ok("
        int add(int a, int b) { return a + b; }
        int main() { return add(3, 4); }
    ");
}

#[test]
fn test_function_three_params() {
    compile_ok("
        int sum3(int a, int b, int c) { return a + b + c; }
        int main() { return sum3(1, 2, 3); }
    ");
}

// ---------------------------------------------------------------------------
// Вызовы функций в выражениях
// ---------------------------------------------------------------------------

#[test]
fn test_call_in_expr() {
    compile_ok("
        int double_it(int x) { return x + x; }
        int main() {
            int r = double_it(3) + double_it(4);
            return r;
        }
    ");
}

#[test]
fn test_nested_calls() {
    compile_ok("
        int inc(int x) { return x + 1; }
        int main() { return inc(inc(inc(0))); }
    ");
}

// ---------------------------------------------------------------------------
// Рекурсия
// ---------------------------------------------------------------------------

#[test]
fn test_factorial() {
    compile_ok("
        int factorial(int n) {
            if (n <= 1) return 1;
            return n * factorial(n - 1);
        }
        int main() { return factorial(5); }
    ");
}

#[test]
fn test_fibonacci() {
    compile_ok("
        int fib(int n) {
            if (n <= 1) return n;
            return fib(n - 1) + fib(n - 2);
        }
        int main() { return fib(7); }
    ");
}

#[test]
fn test_mutual_recursion() {
    // Проверяем что два прохода (collect_globals) позволяют взаимную рекурсию
    compile_ok("
        int is_even(int n);
        int is_odd(int n) { if (n == 0) return 0; return is_even(n - 1); }
        int is_even(int n) { if (n == 0) return 1; return is_odd(n - 1); }
        int main() { return is_even(4); }
    ");
}

// ---------------------------------------------------------------------------
// Несколько функций в модуле
// ---------------------------------------------------------------------------

#[test]
fn test_multiple_functions() {
    compile_ok("
        int square(int x) { return x * x; }
        int cube(int x) { return x * square(x); }
        int main() { return cube(3); }
    ");
}

#[test]
fn test_function_with_loop() {
    compile_ok("
        int sum_to(int n) {
            int s = 0;
            for (int i = 1; i <= n; i++) s = s + i;
            return s;
        }
        int main() { return sum_to(10); }
    ");
}

// ---------------------------------------------------------------------------
// Проверка HIR-структуры
// ---------------------------------------------------------------------------

#[test]
fn test_function_count_in_module() {
    let module = compile_ok("
        int foo() { return 1; }
        int bar() { return 2; }
        int main() { return foo() + bar(); }
    ");
    assert_eq!(module.functions.len(), 3);
}

#[test]
fn test_function_names_in_module() {
    let module = compile_ok("
        int helper() { return 0; }
        int main() { return helper(); }
    ");
    let names: Vec<&str> = module.functions.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"helper"));
    assert!(names.contains(&"main"));
}
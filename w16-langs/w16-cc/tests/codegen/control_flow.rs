// w16-langs\w16-cc\tests\codegen\control_flow.rs
//
//! Тесты управляющих конструкций через весь пайплайн C -> HIR.

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
// if / else
// ---------------------------------------------------------------------------

#[test]
fn test_if_no_else() {
    compile_ok("int main() { int x = 0; if (x) x = 1; return x; }");
}

#[test]
fn test_if_else() {
    compile_ok("int main() { int x = 1; if (x) x = 2; else x = 3; return x; }");
}

#[test]
fn test_nested_if() {
    compile_ok("
        int main() {
            int x = 1;
            if (x) {
                if (x) x = 2;
            }
            return x;
        }
    ");
}

#[test]
fn test_if_comparison() {
    compile_ok("int main() { int x = 5; if (x > 3) x = 1; return x; }");
}

#[test]
fn test_if_equality() {
    compile_ok("int main() { int x = 0; if (x == 0) x = 42; return x; }");
}

// ---------------------------------------------------------------------------
// while
// ---------------------------------------------------------------------------

#[test]
fn test_while_basic() {
    compile_ok("
        int main() {
            int i = 0;
            while (i < 10) i = i + 1;
            return i;
        }
    ");
}

#[test]
fn test_while_with_block() {
    compile_ok("
        int main() {
            int i = 0;
            int s = 0;
            while (i < 5) {
                s = s + i;
                i = i + 1;
            }
            return s;
        }
    ");
}

#[test]
fn test_while_break() {
    compile_ok("
        int main() {
            int i = 0;
            while (1) {
                if (i >= 5) break;
                i = i + 1;
            }
            return i;
        }
    ");
}

#[test]
fn test_while_continue() {
    compile_ok("
        int main() {
            int i = 0;
            int s = 0;
            while (i < 10) {
                i = i + 1;
                if (i == 5) continue;
                s = s + 1;
            }
            return s;
        }
    ");
}

// ---------------------------------------------------------------------------
// for
// ---------------------------------------------------------------------------

#[test]
fn test_for_basic() {
    compile_ok("
        int main() {
            int s = 0;
            for (int i = 0; i < 10; i++) s = s + i;
            return s;
        }
    ");
}

#[test]
fn test_for_empty_init() {
    compile_ok("
        int main() {
            int i = 0;
            for (; i < 5; i++) {}
            return i;
        }
    ");
}

#[test]
fn test_for_infinite_break() {
    compile_ok("
        int main() {
            int i = 0;
            for (;;) {
                i = i + 1;
                if (i == 10) break;
            }
            return i;
        }
    ");
}

// ---------------------------------------------------------------------------
// do-while
// ---------------------------------------------------------------------------

#[test]
fn test_do_while_basic() {
    compile_ok("
        int main() {
            int i = 0;
            do {
                i = i + 1;
            } while (i < 5);
            return i;
        }
    ");
}

#[test]
fn test_do_while_executes_once() {
    // do-while должен выполниться хотя бы раз даже если условие ложно с начала
    compile_ok("
        int main() {
            int i = 0;
            do {
                i = i + 1;
            } while (0);
            return i;
        }
    ");
}

// ---------------------------------------------------------------------------
// switch
// ---------------------------------------------------------------------------

#[test]
fn test_switch_basic() {
    compile_ok("
        int main() {
            int x = 2;
            int r = 0;
            switch (x) {
                case 1: r = 10; break;
                case 2: r = 20; break;
                default: r = 99;
            }
            return r;
        }
    ");
}

#[test]
fn test_switch_default_only() {
    compile_ok("
        int main() {
            int x = 5;
            int r = 0;
            switch (x) {
                default: r = 42;
            }
            return r;
        }
    ");
}

// ---------------------------------------------------------------------------
// Ternary
// ---------------------------------------------------------------------------

#[test]
fn test_ternary() {
    compile_ok("int main() { int x = 1; int r = x ? 10 : 20; return r; }");
}

#[test]
fn test_nested_ternary() {
    compile_ok("int main() { int x = 2; int r = x == 1 ? 1 : x == 2 ? 2 : 3; return r; }");
}
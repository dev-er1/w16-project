// w16-ir\tests\hir_pipeline_tests.rs
//
//! End-to-end тесты HIR-пайплайна.
//!
//! Эти тесты проверяют не отдельную структуру MIR, а поведение целой цепочки:
//! текстовый HIR -> parser -> semantic verifier -> MIR lowering -> optimizer
//! -> bytecode -> VM/JIT. MIR-тесты живут отдельно; здесь важно поймать ошибки,
//! при которых корректная программа начинает возвращать неправильное значение.

use w16_core::REGISTER_COUNT;

fn compile(src: &str) -> w16_core::Bytecode {
    w16_ir::compile_hir_to_bytecode(src)
        .unwrap_or_else(|err| panic!("HIR pipeline failed:\n{err}\n\nsource:\n{src}"))
}

fn run_vm(src: &str) -> [u64; REGISTER_COUNT] {
    let bytecode = compile(src);
    w16_core::run(&bytecode, 1024 * 1024)
        .unwrap_or_else(|err| panic!("VM failed: {err}\n\nsource:\n{src}"))
}

fn run_jit(src: &str) -> [u64; REGISTER_COUNT] {
    let bytecode = compile(src);
    w16_core::run_by_jit(&bytecode)
        .unwrap_or_else(|err| panic!("JIT failed: {err}\n\nsource:\n{src}"))
}

fn assert_vm_r0(src: &str, expected: u64) {
    let regs = run_vm(src);
    assert_eq!(regs[0], expected);
}

fn assert_vm_and_jit_r0(src: &str, expected: u64) {
    let vm_regs = run_vm(src);
    let jit_regs = run_jit(src);

    assert_eq!(vm_regs[0], expected, "VM returned an unexpected value");
    assert_eq!(jit_regs[0], expected, "JIT returned an unexpected value");
}

#[test]
fn hir_arithmetic_precedence_and_return_value() {
    assert_vm_and_jit_r0(
        r#"
        module test {
            fn @main() -> u64 {
                let $x: u64 = 2 + 3 * 4
                let $y: u64 = ($x - 5) * 10
                return $y
            }
        }
        "#,
        90,
    );
}

#[test]
fn hir_if_else_merges_then_value() {
    assert_vm_and_jit_r0(
        r#"
        module test {
            fn @main() -> u64 {
                let $x: u64 = 10

                if ($x > 5) {
                    $x = $x + 7
                } else {
                    $x = 1
                }

                return $x
            }
        }
        "#,
        17,
    );
}

#[test]
fn hir_if_else_merges_else_value() {
    assert_vm_and_jit_r0(
        r#"
        module test {
            fn @main() -> u64 {
                let $x: u64 = 3

                if ($x > 5) {
                    $x = 100
                } else {
                    $x = $x + 4
                }

                return $x
            }
        }
        "#,
        7,
    );
}

#[test]
fn hir_if_without_else_keeps_previous_value() {
    assert_vm_and_jit_r0(
        r#"
        module test {
            fn @main() -> u64 {
                let $x: u64 = 42

                if ($x < 10) {
                    $x = 1
                }

                return $x
            }
        }
        "#,
        42,
    );
}

#[test]
fn hir_while_sum_1_to_10() {
    assert_vm_and_jit_r0(
        r#"
        module test {
            fn @main() -> u64 {
                let $i: u64 = 1
                let $sum: u64 = 0

                while ($i <= 10) {
                    $sum = $sum + $i
                    $i = $i + 1
                }

                return $sum
            }
        }
        "#,
        55,
    );
}

#[test]
fn hir_nested_if_inside_while() {
    assert_vm_and_jit_r0(
        r#"
        module test {
            fn @main() -> u64 {
                let $i: u64 = 0
                let $even_sum: u64 = 0
                let $odd_sum: u64 = 0

                while ($i < 8) {
                    if ($i % 2 == 0) {
                        $even_sum = $even_sum + $i
                    } else {
                        $odd_sum = $odd_sum + $i
                    }

                    $i = $i + 1
                }

                return $even_sum * 100 + $odd_sum
            }
        }
        "#,
        1216,
    );
}

#[test]
fn hir_collatz_837799_returns_524_steps() {
    assert_vm_and_jit_r0(
        r#"
        module test {
            fn @main() -> u64 {
                let $n: u64 = 837799
                let $steps: u64 = 0

                while ($n > 1) {
                    if ($n % 2 == 0) {
                        $n = $n / 2
                    } else {
                        $n = $n * 3 + 1
                    }
                    $steps = $steps + 1
                }

                return $steps
            }
        }
        "#,
        524,
    );
}

#[test]
fn hir_select_expression() {
    assert_vm_and_jit_r0(
        r#"
        module test {
            fn @main() -> u64 {
                let $a: u64 = 10
                let $b: u64 = 20
                let $max: u64 = select($a > $b, $a, $b)
                return $max
            }
        }
        "#,
        20,
    );
}

#[test]
fn hir_function_call_returns_to_caller_in_vm() {
    assert_vm_r0(
        r#"
        module test {
            fn @main() -> u64 {
                let $x: u64 = 9
                let $y: u64 = @double_plus_one($x)
                return $y + 3
            }

            fn @double_plus_one($value: u64) -> u64 {
                return $value * 2 + 1
            }
        }
        "#,
        22,
    );
}

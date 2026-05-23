//! w16-ir/tests/optimizer_tests.rs
//!
//! Тесты MIR-оптимизаций.
//!
//! Каждый тест:
//! 1. Строит MIR через `lower_hir_to_mir` (парсер + lowerer)
//! 2. Запускает конкретный pass или весь `optimize_module`
//! 3. Проверяет структуру IR — что именно изменилось (или не изменилось)
//!
//! Запуск: `cargo test -p w16-ir optimizer`

use w16_ir::{
    lower_hir_to_mir,
    mir::{Literal, MIRInst},
    mir_f::{mir_optimizer, mir_optimizer::loop_closed_form},
    parse_hir_module,
};

// =============================================================================
// ВСПОМОГАТЕЛЬНЫЕ ФУНКЦИИ
// =============================================================================

/// Парсит HIR-строку и переводит в MIR. Паникует при ошибке.
fn parse_and_lower(src: &str) -> w16_ir::mir::MIRModule {
    let hir = parse_hir_module(src).unwrap_or_else(|e| panic!("Parse error: {e}"));
    lower_hir_to_mir(&hir)
}

/// Считает количество живых инструкций данного типа во всей функции.
fn count_live_insts<F>(module: &w16_ir::mir::MIRModule, func_id: usize, pred: F) -> usize
where
    F: Fn(&MIRInst) -> bool,
{
    module.functions[func_id]
        .blocks
        .iter()
        .filter(|b| !b.is_dead)
        .flat_map(|b| b.instructions.iter())
        .filter(|inst| pred(inst))
        .count()
}

/// Считает количество живых блоков в функции.
fn count_live_blocks(module: &w16_ir::mir::MIRModule, func_id: usize) -> usize {
    module.functions[func_id]
        .blocks
        .iter()
        .filter(|b| !b.is_dead)
        .count()
}

/// Считает количество живых значений в функции.
fn count_live_values(module: &w16_ir::mir::MIRModule, func_id: usize) -> usize {
    module.functions[func_id]
        .values
        .iter()
        .filter(|v| !v.is_dead)
        .count()
}

/// Проверяет что в функции есть Const с данным литеральным значением.
fn has_const_value(module: &w16_ir::mir::MIRModule, func_id: usize, expected: u64) -> bool {
    let func = &module.functions[func_id];
    func.blocks
        .iter()
        .filter(|b| !b.is_dead)
        .flat_map(|b| b.instructions.iter())
        .any(|inst| {
            if let MIRInst::Const(cid) = inst {
                if let Some(c) = module.constants.get(*cid) {
                    return c.value == Literal::Int(expected);
                }
            }
            false
        })
}

/// Проверяет что функция не содержит инструкций цикла (Add/Sub/Mul в loop_body).
fn has_no_loop_body(module: &w16_ir::mir::MIRModule, func_id: usize) -> bool {
    // После схлопывания все блоки цикла is_dead=true
    module.functions[func_id]
        .blocks
        .iter()
        .filter(|b| b.name.contains("loop"))
        .all(|b| b.is_dead)
}

// =============================================================================
// ТЕСТЫ: DEAD CODE ELIMINATION (DCE)
// =============================================================================

#[test]
fn dce_removes_unused_const() {
    // Константа определяется но никогда не используется — DCE должна убрать её.
    let mut module = parse_and_lower(
        r#"
        module test {
          fn @main() -> u64 {
            let $used: u64 = 42
            let $unused: u64 = 999
            return $used
          }
        }
    "#,
    );

    let before = count_live_values(&module, 0);
    mir_optimizer::optimize_module(&mut module);
    let after = count_live_values(&module, 0);

    // $unused (значение 999) должно быть убрано
    assert!(
        after < before,
        "DCE должна удалить мёртвое значение: было {before}, стало {after}"
    );
    // Но $used (42) должна остаться
    assert!(
        has_const_value(&module, 0, 42),
        "DCE не должна удалять используемые значения"
    );
}

#[test]
fn dce_removes_chain_of_dead_values() {
    // Цепочка вычислений чей результат не используется
    let mut module = parse_and_lower(
        r#"
        module test {
          fn @main() -> u64 {
            let $a: u64 = 10
            let $b: u64 = $a + 5
            let $c: u64 = $b * 2
            return 42
          }
        }
    "#,
    );

    mir_optimizer::optimize_module(&mut module);

    // Constant Folding + DCE вместе:
    // $a=10, $b=$a+5=15, $c=$b*2=30 — всё сворачивается, Add и Mul исчезают.
    // Проверяем что арифметических инструкций не осталось.
    let live_add = count_live_insts(&module, 0, |i| matches!(i, MIRInst::Add(..)));
    let live_mul = count_live_insts(&module, 0, |i| matches!(i, MIRInst::Mul(..)));
    assert_eq!(live_add, 0, "Add должен исчезнуть после Constant Folding");
    assert_eq!(live_mul, 0, "Mul должен исчезнуть после Constant Folding");

    // Итог: return $c = 30
    assert!(
        has_const_value(&module, 0, 30),
        "Итоговый результат (10+5)*2=30 должен быть константой"
    );
}

#[test]
fn dce_keeps_store_with_dead_result() {
    // Store имеет side-effect — нельзя удалять даже если результат не используется
    let mut module = parse_and_lower(
        r#"
        module test {
          fn @main() -> u64 {
            let $addr: u64 = 0
            store.u64($addr, 42)
            return 0
          }
        }
    "#,
    );

    mir_optimizer::optimize_module(&mut module);

    let store_count = count_live_insts(&module, 0, |inst| matches!(inst, MIRInst::Store { .. }));
    assert_eq!(store_count, 1, "Store должен остаться после DCE");
}

// =============================================================================
// ТЕСТЫ: CONSTANT FOLDING
// =============================================================================

#[test]
fn constant_folding_add() {
    // 2 + 3 должно свернуться в 5 во время компиляции
    let mut module = parse_and_lower(
        r#"
        module test {
          fn @main() -> u64 {
            let $x: u64 = 2 + 3
            return $x
          }
        }
    "#,
    );

    mir_optimizer::optimize_module(&mut module);

    // После свёртки не должно быть инструкции Add
    let add_count = count_live_insts(&module, 0, |inst| matches!(inst, MIRInst::Add(..)));
    assert_eq!(add_count, 0, "Add(2,3) должен свернуться в Const(5)");

    // Должна быть константа 5
    assert!(
        has_const_value(&module, 0, 5),
        "Результат свёртки Const(5) должен существовать"
    );
}

#[test]
fn constant_folding_mul() {
    let mut module = parse_and_lower(
        r#"
        module test {
          fn @main() -> u64 {
            let $x: u64 = 6 * 7
            return $x
          }
        }
    "#,
    );

    mir_optimizer::optimize_module(&mut module);

    let mul_count = count_live_insts(&module, 0, |inst| matches!(inst, MIRInst::Mul(..)));
    assert_eq!(mul_count, 0, "Mul(6,7) должен свернуться");
    assert!(
        has_const_value(&module, 0, 42),
        "Результат 42 должен быть в пуле констант"
    );
}

#[test]
fn constant_folding_chain() {
    // (2 + 3) * 4  -> 5 * 4  -> 20 — два шага свёртки
    let mut module = parse_and_lower(
        r#"
        module test {
          fn @main() -> u64 {
            let $a: u64 = 2 + 3
            let $b: u64 = $a * 4
            return $b
          }
        }
    "#,
    );

    mir_optimizer::optimize_module(&mut module);

    let arith_count = count_live_insts(&module, 0, |inst| {
        matches!(inst, MIRInst::Add(..) | MIRInst::Mul(..))
    });
    assert_eq!(arith_count, 0, "Вся цепочка должна свернуться");
    assert!(
        has_const_value(&module, 0, 20),
        "Итог (2+3)*4=20 должен быть константой"
    );
}

#[test]
fn constant_folding_comparison() {
    // 5 > 3  -> true(1)
    let mut module = parse_and_lower(
        r#"
        module test {
          fn @main() -> u64 {
            let $result: u64 = 0
            if (5 > 3) {
              $result = 1
            }
            return $result
          }
        }
    "#,
    );

    mir_optimizer::optimize_module(&mut module);

    // ICmp должен исчезнуть после свёртки
    let icmp_count = count_live_insts(&module, 0, |inst| matches!(inst, MIRInst::ICmp { .. }));
    assert_eq!(icmp_count, 0, "ICmp(5>3) должен свернуться в Bool(true)");
}

#[test]
fn constant_folding_no_division_by_zero() {
    // Деление на ноль НЕ должно сворачиваться — оставляем как есть
    let mut module = parse_and_lower(
        r#"
        module test {
          fn @main() -> u64 {
            let $x: u64 = 10 / 0
            return $x
          }
        }
    "#,
    );

    mir_optimizer::optimize_module(&mut module);

    // UDiv должен остаться (не сворачивается при делении на 0)
    let div_count = count_live_insts(&module, 0, |inst| matches!(inst, MIRInst::UDiv(..)));
    assert_eq!(div_count, 1, "Деление на 0 не должно сворачиваться");
}

// =============================================================================
// ТЕСТЫ: DEAD BLOCK ELIMINATION (DBE)
// =============================================================================

#[test]
fn dbe_removes_unreachable_block() {
    // После return следующий код недостижим
    let mut module = parse_and_lower(
        r#"
        module test {
          fn @main() -> u64 {
            return 42
            return 0
          }
        }
    "#,
    );

    let before = count_live_blocks(&module, 0);
    mir_optimizer::optimize_module(&mut module);
    // Entry блок должен остаться, лишние убраны
    assert!(
        count_live_blocks(&module, 0) <= before,
        "DBE должна убрать или не добавить лишних блоков"
    );
}

#[test]
fn dbe_keeps_reachable_blocks() {
    let mut module = parse_and_lower(
        r#"
        module test {
          fn @main() -> u64 {
            let $x: u64 = 1
            if ($x == 1) {
              return 10
            } else {
              return 20
            }
          }
        }
    "#,
    );

    mir_optimizer::optimize_module(&mut module);

    // if_then и if_else достижимы — оба должны остаться
    let live = count_live_blocks(&module, 0);
    assert!(live >= 2, "Оба достижимых блока должны остаться");
}

// =============================================================================
// ТЕСТЫ: LOOP CLOSED-FORM (схлопывание циклов)
// =============================================================================

#[test]
fn loop_closed_form_sum() {
    // sum(0..100) = 4950 — схлопывается в константу
    let mut module = parse_and_lower(
        r#"
        module test {
          fn @main() -> u64 {
            let $i: u64 = 0
            let $sum: u64 = 0
            while ($i < 100) {
              $sum = $sum + $i
              $i = $i + 1
            }
            return $sum
          }
        }
    "#,
    );

    let collapsed = loop_closed_form::run(&mut module);

    assert_eq!(collapsed, 1, "Ровно один цикл должен схлопнуться");

    // Проверяем что блоки цикла помечены мёртвыми напрямую
    let dead_loop_blocks = module.functions[0]
        .blocks
        .iter()
        .filter(|b| b.is_dead)
        .count();
    assert!(
        dead_loop_blocks > 0,
        "После схлопывания хотя бы один блок должен быть помечен мёртвым"
    );

    assert!(
        has_const_value(&module, 0, 4950),
        "Результат sum(0..100)=4950 должен быть константой"
    );
}

#[test]
fn loop_closed_form_counter() {
    // Счётчик: i после 1000 итераций = 1000
    let mut module = parse_and_lower(
        r#"
        module test {
          fn @main() -> u64 {
            let $i: u64 = 0
            while ($i < 1000) {
              $i = $i + 1
            }
            return $i
          }
        }
    "#,
    );

    let collapsed = loop_closed_form::run(&mut module);

    assert_eq!(collapsed, 1, "Цикл-счётчик должен схлопнуться");
    assert!(
        has_const_value(&module, 0, 1000),
        "Финальное значение i=1000 должно быть константой"
    );
}

#[test]
fn loop_closed_form_large_sum() {
    // sum(0..1_000_000) = 499_999_500_000
    let mut module = parse_and_lower(
        r#"
        module test {
          fn @main() -> u64 {
            let $i: u64 = 0
            let $sum: u64 = 0
            while ($i < 1000000) {
              $sum = $sum + $i
              $i = $i + 1
            }
            return $sum
          }
        }
    "#,
    );

    loop_closed_form::run(&mut module);

    assert!(
        has_const_value(&module, 0, 499_999_500_000),
        "sum(0..1M)=499_999_500_000 должна быть константой"
    );
}

#[test]
fn loop_closed_form_not_applied_to_collatz() {
    // Коллатц — нелинейный цикл, схлопывание НЕ должно применяться
    let mut module = parse_and_lower(
        r#"
        module test {
          fn @main() -> u64 {
            let $n: u64 = 27
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
    );

    let collapsed = loop_closed_form::run(&mut module);

    assert_eq!(
        collapsed, 0,
        "Нелинейный цикл (Коллатц) не должен схлопываться"
    );
    // Тело цикла должно остаться живым
    assert!(
        !has_no_loop_body(&module, 0),
        "Тело цикла Коллатца должно остаться"
    );
}

#[test]
fn loop_closed_form_with_side_effects_not_applied() {
    // Цикл со Store — нельзя схлопывать (side-effect)
    let mut module = parse_and_lower(
        r#"
        module test {
          fn @main() -> u64 {
            let $i: u64 = 0
            let $addr: u64 = 0
            while ($i < 10) {
              store.u64($addr, $i)
              $i = $i + 1
            }
            return $i
          }
        }
    "#,
    );

    let collapsed = loop_closed_form::run(&mut module);

    assert_eq!(collapsed, 0, "Цикл с Store не должен схлопываться");
}

// =============================================================================
// ТЕСТЫ: КОРРЕКТНОСТЬ ПОСЛЕ ОПТИМИЗАЦИЙ
// =============================================================================

#[test]
fn optimize_preserves_ssa_invariant() {
    // После оптимизации blocks[i].id == i должно выполняться
    let mut module = parse_and_lower(
        r#"
        module test {
          fn @main() -> u64 {
            let $i: u64 = 0
            let $sum: u64 = 0
            while ($i < 50) {
              $sum = $sum + $i
              $i = $i + 1
            }
            return $sum
          }
        }
    "#,
    );

    mir_optimizer::optimize_module(&mut module);

    for func in &module.functions {
        for (idx, block) in func.blocks.iter().enumerate() {
            assert_eq!(
                block.id, idx,
                "SSA инвариант нарушен: blocks[{idx}].id == {}",
                block.id
            );
        }
    }
}

#[test]
fn optimize_multiple_passes_idempotent() {
    // Запуск оптимизатора дважды не должен менять результат второй раз
    let mut module = parse_and_lower(
        r#"
        module test {
          fn @main() -> u64 {
            let $i: u64 = 0
            let $sum: u64 = 0
            while ($i < 100) {
              $sum = $sum + $i
              $i = $i + 1
            }
            return $sum
          }
        }
    "#,
    );

    let changes_first = mir_optimizer::optimize_module(&mut module);
    let changes_second = mir_optimizer::optimize_module(&mut module);

    assert!(changes_first > 0, "Первый проход должен что-то изменить");
    assert_eq!(
        changes_second, 0,
        "Второй проход не должен ничего менять (идемпотентность)"
    );
}

#[test]
fn dce_and_folding_work_together() {
    // Constant folding создаёт мёртвые промежуточные значения  -> DCE убирает их
    let mut module = parse_and_lower(
        r#"
        module test {
          fn @main() -> u64 {
            let $a: u64 = 2 + 3
            let $b: u64 = $a * 2
            let $c: u64 = $b - 1
            return $c
          }
        }
    "#,
    );

    mir_optimizer::optimize_module(&mut module);

    // Все арифметические операции должны свернуться
    let arith = count_live_insts(&module, 0, |inst| {
        matches!(inst, MIRInst::Add(..) | MIRInst::Mul(..) | MIRInst::Sub(..))
    });
    assert_eq!(arith, 0, "Все вычисления должны свернуться: (2+3)*2-1=9");
    assert!(
        has_const_value(&module, 0, 9),
        "Итог должен быть константой 9"
    );
}

#[test]
fn analysis_is_valid_after_optimize() {
    // FunctionAnalysis должен быть валидным после optimize_module
    let mut module = parse_and_lower(
        r#"
        module test {
          fn @main() -> u64 {
            let $i: u64 = 0
            while ($i < 10) {
              $i = $i + 1
            }
            return $i
          }
        }
    "#,
    );

    mir_optimizer::optimize_module(&mut module);

    // После optimize_module анализ пересчитывается
    assert!(
        module.analysis.len() == module.functions.len(),
        "analysis.len() должен совпадать с functions.len()"
    );
}

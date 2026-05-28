// w16-ir\src\mir_analyze\induction_vars.rs
//
//! # Анализ индуктивных переменных
//!
//! Находит индуктивные переменные (IV) в циклах и вычисляет их closed-form
//! выражение — формулу результата после N итераций без выполнения цикла.
//!
//! ## Что такое индуктивная переменная
//!
//! Переменная `x` является **базовой IV** в цикле если:
//! - Она определена как phi-параметр header-блока.
//! - В каждой итерации увеличивается/уменьшается на константу: `x_next = x + step`.
//!
//! Пример: `$i = 0; while { $i = $i + 1 }` -> IV с base=0, step=1.
//!
//! Переменная `y` является **производной IV** если:
//! - `y = f(x)` где x — базовая IV, f — линейная функция.
//! - Например: `$sum = $sum + $i` где $i — базовая IV.
//!   Тогда $sum — это "second-order IV": `sum(k) = Σ(i=0..k-1) step_i`.
//!
//! ## Closed-form формулы
//!
//! Для цикла с trip_count = N (число итераций):
//!
//! | Тип IV                        | Closed-form                      |
//! |-------------------------------|----------------------------------|
//! | Linear: base + step*k         | base + step * N                  |
//! | Sum of linear: Σ(base+step*k) | base*N + step * N*(N-1)/2   |
//!
//! Именно второй случай превращает O(N) цикл в O(1) вычисление.
//!
//! ## Trip count
//!
//! Trip count = количество итераций цикла.
//! Вычисляется из exit condition вида `i < N` или `i != N`:
//! `trip_count = (N - base) / step` (для шага 1: просто N - base).

use std::collections::HashMap;

use super::loop_info::Loop;
use crate::mir::ICmpOp;
use crate::mir::{BlockId, MIRFunction, MIRInst, MIRTerminator, ValueDef, ValueId};

// =============================================================================
// СТРУКТУРЫ
// =============================================================================

/// Шаг индуктивной переменной — всегда константа (для linear IV).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    pub value: i64,
}

/// Вид индуктивной переменной.
#[derive(Debug, Clone)]
pub enum IVKind {
    /// Базовая линейная IV: value(k) = base + step * k
    /// Пример: `$i = 0 + 1*k`
    Linear { base: i64, step: Step },

    /// Сумма линейной IV: value(k) = base_sum + sum(0..k-1)(base_iv + step_iv * j) = base_sum + base_iv*k + step_iv * k*(k-1)/2
    /// Пример: `$sum += $i` где $i — Linear(0, 1)
    /// -> $sum(N) = 0 + 0*N + 1 * N*(N-1)/2 = N*(N-1)/2
    SumOfLinear {
        /// Начальное значение суммы
        base_sum: i64,
        /// Базовое значение суммируемой IV
        base_iv: i64,
        /// Шаг суммируемой IV
        step_iv: Step,
    },
}

/// Описание одной индуктивной переменной.
#[derive(Debug, Clone)]
pub struct InductionVar {
    /// ValueId phi-параметра header-блока (это "имя" IV в SSA).
    pub phi_id: ValueId,
    /// Вид IV.
    pub kind: IVKind,
}

impl InductionVar {
    /// Вычисляет значение IV после `trip_count` итераций.
    /// Возвращает None если результат переполняет i64.
    pub fn eval(&self, trip_count: u64) -> Option<u64> {
        let n = trip_count as i64;
        match &self.kind {
            IVKind::Linear { base, step } => {
                // base + step * N
                let result = base.checked_add(step.value.checked_mul(n)?)?;
                Some(result as u64)
            }
            IVKind::SumOfLinear {
                base_sum,
                base_iv,
                step_iv,
            } => {
                // base_sum + base_iv * N + step_iv * N*(N-1)/2
                let triangular = n.checked_mul(n.checked_sub(1)?)?.checked_div(2)?;
                let part1 = base_iv.checked_mul(n)?;
                let part2 = step_iv.value.checked_mul(triangular)?;
                let result = base_sum.checked_add(part1)?.checked_add(part2)?;
                Some(result as u64)
            }
        }
    }
}

/// Условие выхода из цикла.
#[derive(Debug, Clone)]
pub struct ExitCondition {
    /// IV которая проверяется
    pub iv_phi: ValueId,
    /// Операция сравнения
    pub op: ICmpOp,
    /// Граница (константа)
    pub bound: i64,
}

impl ExitCondition {
    /// Вычисляет trip count — сколько раз выполняется тело цикла.
    /// Предполагает что IV начинает с base и идёт с шагом step.
    pub fn trip_count(&self, base: i64, step: i64) -> Option<u64> {
        if step == 0 {
            return None; // бесконечный цикл
        }
        let bound = self.bound;
        let trips = match self.op {
            // i < bound: итерации пока i < bound, начиная с base
            ICmpOp::Ult | ICmpOp::Slt => {
                if step > 0 && bound > base {
                    // ceil((bound - base) / step)
                    let range = bound - base;
                    ((range + step - 1) / step) as u64
                } else {
                    return None;
                }
            }
            // i != bound: итерации пока i != bound
            ICmpOp::Ne => {
                if step > 0 && bound > base && (bound - base) % step == 0 {
                    ((bound - base) / step) as u64
                } else {
                    return None;
                }
            }
            // i <= bound
            ICmpOp::Ule | ICmpOp::Sle => {
                if step > 0 && bound >= base {
                    ((bound - base) / step + 1) as u64
                } else {
                    return None;
                }
            }
            _ => return None,
        };
        Some(trips)
    }
}

/// Результат анализа IV для одного цикла.
#[derive(Debug, Clone)]
pub struct LoopIVAnalysis {
    /// Условие выхода из цикла (из terminator header-блока).
    pub exit_cond: Option<ExitCondition>,
    /// Trip count если удалось вычислить статически.
    pub trip_count: Option<u64>,
    /// Все найденные IV, индексированные по phi_id.
    pub ivs: HashMap<ValueId, InductionVar>,
}

impl LoopIVAnalysis {
    /// Возвращает IV по её phi_id.
    pub fn get_iv(&self, phi_id: ValueId) -> Option<&InductionVar> {
        self.ivs.get(&phi_id)
    }
}

// =============================================================================
// ПОСТРОЕНИЕ
// =============================================================================

/// Анализирует индуктивные переменные одного цикла.
pub fn analyze_loop_ivs(func: &MIRFunction, lp: &Loop) -> LoopIVAnalysis {
    // Шаг 1: найти базовые Linear IV через phi-параметры header
    let linear_ivs = find_linear_ivs(func, lp);

    // Шаг 2: найти exit condition
    let exit_cond = find_exit_condition(func, lp, &linear_ivs);

    // Шаг 3: вычислить trip count
    let trip_count = exit_cond.as_ref().and_then(|ec| {
        let iv = linear_ivs.get(&ec.iv_phi)?;
        if let IVKind::Linear { base, step } = &iv.kind {
            ec.trip_count(*base, step.value)
        } else {
            None
        }
    });

    // Шаг 4: найти производные SumOfLinear IV
    let mut all_ivs = linear_ivs;
    find_sum_ivs(func, lp, &mut all_ivs);

    LoopIVAnalysis {
        exit_cond,
        trip_count,
        ivs: all_ivs,
    }
}

// =============================================================================
// ШАГ 1: ПОИСК ЛИНЕЙНЫХ IV
// =============================================================================

/// Находит все базовые Linear IV в цикле.
///
/// IV = phi-параметр header-блока где:
/// - Один вход (из preheader) — константа (base)
/// - Другой вход (back edge) — `phi + const_step`
fn find_linear_ivs(func: &MIRFunction, lp: &Loop) -> HashMap<ValueId, InductionVar> {
    let mut ivs = HashMap::new();
    let header = &func.blocks[lp.header];

    for (phi_id, _ty) in &header.params {
        if let Some(iv) = try_extract_linear_iv(func, lp, *phi_id) {
            ivs.insert(*phi_id, iv);
        }
    }

    ivs
}

/// Пытается извлечь Linear IV из phi-параметра.
fn try_extract_linear_iv(func: &MIRFunction, lp: &Loop, phi_id: ValueId) -> Option<InductionVar> {
    // Phi получает значения через аргументы Jmp/Br:
    // - от preheader (извне цикла) -> base
    // - от loop latch (back edge tail) -> recurrence
    //
    // Находим Jmp в header: args[i] соответствует params[i].
    // Ищем аргументы от preheader и от latch.
    let header = &func.blocks[lp.header];

    // Индекс этого phi в params header-а
    let phi_idx = header.params.iter().position(|(id, _)| *id == phi_id)?;

    // Ищем base: аргумент от predecessor-а вне цикла
    // Ищем recurrence: аргумент от latch (back edge tail)
    let mut recurrence_val: Option<ValueId> = None;

    for &pred in &header.predecessors {
        let is_latch = pred == lp.back_edge.tail;
        let arg_id = get_jmp_arg(&func.blocks[pred].terminator, lp.header, phi_idx)?;

        if is_latch {
            recurrence_val = Some(arg_id);
        }
    }

    let rec = recurrence_val?;

    // Проверяем что recurrence = phi + step
    // try_extract_linear_iv is only used internally without module access;
    // the real entry point is analyze_loop_ivs_full which calls extract_add_step with module.
    let _ = (func, lp, rec, phi_id);
    None // disabled: use analyze_loop_ivs_full instead
}

/// Извлекает аргумент под индексом `arg_idx` из terminator-а `pred`
/// который прыгает в `target`.
fn get_jmp_arg(terminator: &MIRTerminator, target: BlockId, arg_idx: usize) -> Option<ValueId> {
    match terminator {
        MIRTerminator::Jmp { target: t, args } if *t == target => args.get(arg_idx).copied(),
        MIRTerminator::Br {
            then_blk,
            then_args,
            else_blk,
            else_args,
            ..
        } => {
            if *then_blk == target {
                then_args.get(arg_idx).copied()
            } else if *else_blk == target {
                else_args.get(arg_idx).copied()
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Проверяет что `val_id` определён как `phi_id + constant` или `constant + phi_id`.
/// Возвращает константу-шаг если да.
fn extract_add_step(
    func: &MIRFunction,
    lp: &Loop,
    val_id: ValueId,
    phi_id: ValueId,
    module: &crate::mir::MIRModule,
) -> Option<i64> {
    let val_data = &func.values[val_id];

    // val должен быть определён инструкцией в теле цикла
    let (block_id, inst_idx) = match val_data.def {
        ValueDef::Inst(b, i) => (b, i),
        _ => return None,
    };
    if !lp.contains(block_id) {
        return None;
    }

    let inst = &func.blocks[block_id].instructions[inst_idx];
    match inst {
        MIRInst::Add(a, b) => {
            if *a == phi_id {
                extract_const_from_module(func, module, *b)
            } else if *b == phi_id {
                extract_const_from_module(func, module, *a)
            } else {
                None
            }
        }
        MIRInst::Sub(a, b) => {
            if *a == phi_id {
                extract_const_from_module(func, module, *b).map(|c| -c)
            } else {
                None
            }
        }
        _ => None,
    }
}

// =============================================================================
// ШАГ 2: EXIT CONDITION
// =============================================================================

fn find_exit_condition(
    func: &MIRFunction,
    lp: &Loop,
    linear_ivs: &HashMap<ValueId, InductionVar>,
) -> Option<ExitCondition> {
    // Header должен заканчиваться Br с ICmp как условием
    let header = &func.blocks[lp.header];
    let cond_id = match &header.terminator {
        MIRTerminator::Br {
            cond,
            then_blk,
            else_blk,
            ..
        } => {
            // then должен быть внутри цикла, else — выход (или наоборот)
            let then_in = lp.contains(*then_blk);
            let else_in = lp.contains(*else_blk);
            if then_in && !else_in {
                *cond // true -> continue, false -> exit
            } else {
                return None;
            }
        }
        _ => return None,
    };

    // Находим инструкцию ICmp которая определяет cond
    let (block_id, inst_idx) = match func.values[cond_id].def {
        ValueDef::Inst(b, i) => (b, i),
        _ => return None,
    };

    let inst = &func.blocks[block_id].instructions[inst_idx];
    let (op, lhs, rhs) = match inst {
        MIRInst::ICmp { op, lhs, rhs } => (*op, *lhs, *rhs),
        _ => return None,
    };

    // lhs должен быть IV, rhs — константа (или наоборот)
    if let Some(iv) = linear_ivs.get(&lhs)
        && let IVKind::Linear { .. } = &iv.kind
    {
        // rhs должна быть константой — пока заглушка
        // (реальное значение нужно из module.constants)
        let _ = rhs;
        return Some(ExitCondition {
            iv_phi: lhs,
            op,
            bound: 0,
        }); // bound заполнится ниже
    }
    None
}

// =============================================================================
// ШАГ 4: ПРОИЗВОДНЫЕ SumOfLinear IV
// =============================================================================

/// Находит переменные вида `sum = sum + iv` где iv — Linear IV.
fn find_sum_ivs(func: &MIRFunction, lp: &Loop, ivs: &mut HashMap<ValueId, InductionVar>) {
    let header = &func.blocks[lp.header];
    let phi_idx_to_id: Vec<ValueId> = header.params.iter().map(|(id, _)| *id).collect();

    for (phi_idx, &phi_id) in phi_idx_to_id.iter().enumerate() {
        // Уже обработали как Linear?
        if ivs.contains_key(&phi_id) {
            continue;
        }

        // Ищем recurrence: sum_next = sum_phi + linear_iv
        let rec_id = {
            let latch_term = &func.blocks[lp.back_edge.tail].terminator;
            match get_jmp_arg(latch_term, lp.header, phi_idx) {
                Some(id) => id,
                None => continue,
            }
        };

        // Получаем base из preheader
        let base_sum = {
            let mut found = None;
            for &pred in &header.predecessors {
                if pred != lp.back_edge.tail
                    && let Some(arg) =
                        get_jmp_arg(&func.blocks[pred].terminator, lp.header, phi_idx)
                {
                    // Пока заглушка — реальное значение берётся из module
                    let _ = arg;
                    found = Some(0i64);
                }
            }
            match found {
                Some(v) => v,
                None => continue,
            }
        };

        // rec должен быть phi + linear_iv
        let (block_id, inst_idx) = match func.values[rec_id].def {
            ValueDef::Inst(b, i) => (b, i),
            _ => continue,
        };
        let inst = &func.blocks[block_id].instructions[inst_idx];
        let (a, b) = match inst {
            MIRInst::Add(a, b) => (*a, *b),
            _ => continue,
        };

        // Один из операндов = phi_id (сам sum), другой = Linear IV
        let (sum_op, iv_op) = if a == phi_id {
            (a, b)
        } else if b == phi_id {
            (b, a)
        } else {
            continue;
        };
        let _ = sum_op;

        if let Some(linear_iv) = ivs.get(&iv_op)
            && let IVKind::Linear {
                base: base_iv,
                step: step_iv,
            } = linear_iv.kind
        {
            ivs.insert(
                phi_id,
                InductionVar {
                    phi_id,
                    kind: IVKind::SumOfLinear {
                        base_sum,
                        base_iv,
                        step_iv,
                    },
                },
            );
        }
    }
}

// =============================================================================
// ПУБЛИЧНЫЙ API С ДОСТУПОМ К МОДУЛЮ
// =============================================================================

use crate::mir::{Literal, MIRModule};

/// Полный анализ IV с доступом к константам модуля.
/// Это основная функция которую вызывает оптимизатор.
pub fn analyze_loop_ivs_full(func: &MIRFunction, lp: &Loop, module: &MIRModule) -> LoopIVAnalysis {
    let header = &func.blocks[lp.header];
    let phi_count = header.params.len();

    // --- Находим Linear IV ---
    let mut ivs: HashMap<ValueId, InductionVar> = HashMap::new();
    for phi_idx in 0..phi_count {
        let phi_id = header.params[phi_idx].0;

        let mut base_val: Option<i64> = None;
        let mut rec_val: Option<ValueId> = None;

        for &pred in &header.predecessors {
            let is_latch = pred == lp.back_edge.tail;
            let arg = get_jmp_arg(&func.blocks[pred].terminator, lp.header, phi_idx);
            if let Some(arg_id) = arg {
                if is_latch {
                    rec_val = Some(arg_id);
                } else {
                    let cv = extract_const_from_module(func, module, arg_id);
                    base_val = Some(cv.unwrap_or(i64::MIN));
                }
            }
        }

        let base = match base_val {
            Some(v) if v != i64::MIN => v,
            _other => {
                continue;
            }
        };
        let rec = match rec_val {
            Some(v) => v,
            None => {
                continue;
            }
        };

        let step = extract_add_step(func, lp, rec, phi_id, module);
        if let Some(step) = step {
            ivs.insert(
                phi_id,
                InductionVar {
                    phi_id,
                    kind: IVKind::Linear {
                        base,
                        step: Step { value: step },
                    },
                },
            );
        }
    }

    // --- Exit condition ---
    for _ in ivs.values() {}
    let exit_cond = find_exit_condition_with_module(func, lp, &ivs, module);

    // --- Trip count ---
    let trip_count = exit_cond.as_ref().and_then(|ec| {
        let iv = ivs.get(&ec.iv_phi)?;
        if let IVKind::Linear { base, step } = &iv.kind {
            ec.trip_count(*base, step.value)
        } else {
            None
        }
    });

    // --- SumOfLinear IV ---
    find_sum_ivs_with_module(func, lp, &mut ivs, module);
    LoopIVAnalysis {
        exit_cond,
        trip_count,
        ivs,
    }
}

/// Извлекает i64 из ValueId с доступом к module.constants.
pub fn extract_const_from_module(
    func: &MIRFunction,
    module: &MIRModule,
    val_id: ValueId,
) -> Option<i64> {
    let val_data = &func.values[val_id];
    let const_id = match val_data.def {
        ValueDef::Constant(id) => id,
        ValueDef::Inst(b, i) => match func.blocks[b].instructions.get(i)? {
            MIRInst::Const(id) => *id,
            _ => return None,
        },
        _ => return None,
    };
    let mir_const = module.constants.get(const_id)?;
    match &mir_const.value {
        Literal::Int(v) => Some(*v as i64),
        Literal::Bool(v) => Some(if *v { 1 } else { 0 }),
        _ => None,
    }
}

fn find_exit_condition_with_module(
    func: &MIRFunction,
    lp: &Loop,
    ivs: &HashMap<ValueId, InductionVar>,
    module: &MIRModule,
) -> Option<ExitCondition> {
    let header = &func.blocks[lp.header];
    let (cond_id, exit_is_else) = match &header.terminator {
        MIRTerminator::Br {
            cond,
            then_blk,
            else_blk,
            ..
        } => {
            let then_in = lp.contains(*then_blk);
            let else_in = lp.contains(*else_blk);
            if then_in && !else_in {
                (*cond, false) // false -> exit через else
            } else if !then_in && else_in {
                (*cond, true) // true -> exit через then
            } else {
                return None;
            }
        }
        _ => return None,
    };

    let (block_id, inst_idx) = match func.values[cond_id].def {
        ValueDef::Inst(b, i) => (b, i),
        _ => return None,
    };

    let inst = &func.blocks[block_id].instructions[inst_idx];
    let (mut op, lhs, rhs) = match inst {
        MIRInst::ICmp { op, lhs, rhs } => (*op, *lhs, *rhs),
        _ => return None,
    };

    // Если exit через then — инвертируем операцию
    if exit_is_else {
        op = invert_icmp(op);
    }

    // lhs = IV, rhs = bound
    if ivs.contains_key(&lhs) {
        let bound = extract_const_from_module(func, module, rhs)?;
        return Some(ExitCondition {
            iv_phi: lhs,
            op,
            bound,
        });
    }
    // rhs = IV, lhs = bound (зеркальный случай)
    if ivs.contains_key(&rhs) {
        let bound = extract_const_from_module(func, module, lhs)?;
        let op = mirror_icmp(op);
        return Some(ExitCondition {
            iv_phi: rhs,
            op,
            bound,
        });
    }

    None
}

fn find_sum_ivs_with_module(
    func: &MIRFunction,
    lp: &Loop,
    ivs: &mut HashMap<ValueId, InductionVar>,
    module: &MIRModule,
) {
    let header = &func.blocks[lp.header];
    let phi_ids: Vec<ValueId> = header.params.iter().map(|(id, _)| *id).collect();

    for (phi_idx, &phi_id) in phi_ids.iter().enumerate() {
        if ivs.contains_key(&phi_id) {
            continue;
        }

        let rec_id = match get_jmp_arg(
            &func.blocks[lp.back_edge.tail].terminator,
            lp.header,
            phi_idx,
        ) {
            Some(id) => id,
            None => continue,
        };

        // Base sum из preheader
        let mut base_sum: Option<i64> = None;
        for &pred in &header.predecessors {
            if pred != lp.back_edge.tail
                && let Some(arg) = get_jmp_arg(&func.blocks[pred].terminator, lp.header, phi_idx)
            {
                base_sum = extract_const_from_module(func, module, arg);
            }
        }
        let base_sum = match base_sum {
            Some(v) => v,
            None => continue,
        };

        // rec = phi + linear_iv
        let (block_id, inst_idx) = match func.values[rec_id].def {
            ValueDef::Inst(b, i) => (b, i),
            _ => continue,
        };
        let inst = &func.blocks[block_id].instructions[inst_idx];
        let (a, b) = match inst {
            MIRInst::Add(a, b) => (*a, *b),
            _ => continue,
        };

        let iv_op = if a == phi_id {
            b
        } else if b == phi_id {
            a
        } else {
            continue;
        };

        if let Some(linear_iv) = ivs.get(&iv_op).cloned()
            && let IVKind::Linear {
                base: base_iv,
                step: step_iv,
            } = linear_iv.kind
        {
            ivs.insert(
                phi_id,
                InductionVar {
                    phi_id,
                    kind: IVKind::SumOfLinear {
                        base_sum,
                        base_iv,
                        step_iv,
                    },
                },
            );
        }
    }
}

// =============================================================================
// УТИЛИТЫ
// =============================================================================

fn invert_icmp(op: ICmpOp) -> ICmpOp {
    match op {
        ICmpOp::Eq => ICmpOp::Ne,
        ICmpOp::Ne => ICmpOp::Eq,
        ICmpOp::Slt => ICmpOp::Sge,
        ICmpOp::Sle => ICmpOp::Sgt,
        ICmpOp::Sgt => ICmpOp::Sle,
        ICmpOp::Sge => ICmpOp::Slt,
        ICmpOp::Ult => ICmpOp::Uge,
        ICmpOp::Ule => ICmpOp::Ugt,
        ICmpOp::Ugt => ICmpOp::Ule,
        ICmpOp::Uge => ICmpOp::Ult,
    }
}

fn mirror_icmp(op: ICmpOp) -> ICmpOp {
    match op {
        ICmpOp::Slt => ICmpOp::Sgt,
        ICmpOp::Sle => ICmpOp::Sge,
        ICmpOp::Sgt => ICmpOp::Slt,
        ICmpOp::Sge => ICmpOp::Sle,
        ICmpOp::Ult => ICmpOp::Ugt,
        ICmpOp::Ule => ICmpOp::Uge,
        ICmpOp::Ugt => ICmpOp::Ult,
        ICmpOp::Uge => ICmpOp::Ule,
        op => op,
    }
}

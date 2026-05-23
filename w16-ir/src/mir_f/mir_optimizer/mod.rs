//! # MIR Оптимизатор
//!
//! Запускает pass-ы оптимизации над MIR-модулем.
//! Каждый pass инвалидирует `FunctionAnalysis` для изменённых функций.
//! В debug-режиме верификатор запускается после каждого pass-а.
//!
//! ## Pass-ы (в порядке применения)
//!
//! 1. **Dead Block Elimination (DBE)** — помечает недостижимые блоки как мёртвые.
//!    Быстро и позволяет остальным pass-ам не обходить мёртвый код.
//!
//! 2. **Constant Folding** — вычисляет результаты инструкций с константными
//!    операндами во время компиляции. `add(Const(2), Const(3))` -> `Const(5)`.
//!    Использует `intern_constant` для дедупликации в пуле констант.
//!
//! 3. **Dead Code Elimination (DCE)** — помечает значения мёртвыми если
//!    их use-list пуст и они не имеют side-effects (Store, Call).
//!    Работает итеративно: удаление одного мёртвого значения может открыть новые.
//!
//! ## Что пока не реализовано (будущие pass-ы)
//! - Inlining (требует FunctionAnalysis.is_inlinable и глубокого клонирования IR)
//! - Copy Propagation (упрощение Mov-цепочек)
//! - LICM (Loop-Invariant Code Motion)
//! - Strength Reduction (замена mul на shift где возможно)

pub mod loop_closed_form;

use std::collections::VecDeque;

use crate::mir::{BlockId, ConstantId, FunctionId, Literal, MIRInst, MIRModule, Type, ValueId};
use crate::mir_f::mir_analyze::compute_all_analysis;

// =============================================================================
// ТОЧКА ВХОДА
// =============================================================================

/// Запускает все оптимизационные pass-ы над модулем.
/// Возвращает количество изменений (полезно для тестов и диагностики).
pub fn optimize_module(module: &mut MIRModule) -> usize {
    let mut total_changes = 0;

    // Пересчитываем анализ перед оптимизацией
    compute_all_analysis(module);

    // Loop Closed-Form: запускаем ДО остальных pass-ов.
    // Если цикл схлопнулся — DCE и DBE уберут мёртвый код автоматически.
    let closed = loop_closed_form::run(module);
    if closed > 0 {
        total_changes += closed;
        compute_all_analysis(module);
    }

    let func_count = module.functions.len();
    for func_id in 0..func_count {
        total_changes += optimize_function(module, func_id);
    }

    total_changes
}

/// Запускает pass-ы для одной функции. Итерирует до фиксированной точки.
fn optimize_function(module: &mut MIRModule, func_id: FunctionId) -> usize {
    let mut total = 0;
    loop {
        let mut round = 0;
        round += dead_block_elimination(module, func_id);
        round += constant_folding(module, func_id);
        round += dead_code_elimination(module, func_id);

        if round == 0 {
            break;
        }
        total += round;
        module.invalidate_analysis(func_id);
    }
    total
}

// =============================================================================
// PASS 1: DEAD BLOCK ELIMINATION
// =============================================================================

/// Помечает блоки недостижимые из entry как `is_dead = true`.
/// Не удаляет физически — сохраняем инвариант индексов.
fn dead_block_elimination(module: &mut MIRModule, func_id: FunctionId) -> usize {
    let func = &mut module.functions[func_id];
    let n = func.blocks.len();

    // BFS от entry
    let mut reachable = vec![false; n];
    let mut queue = VecDeque::new();
    queue.push_back(0usize); // entry = block 0
    reachable[0] = true;

    while let Some(b) = queue.pop_front() {
        for succ in func.blocks[b].terminator.successors() {
            if succ < n && !reachable[succ] {
                reachable[succ] = true;
                queue.push_back(succ);
            }
        }
    }

    let mut changes = 0;
    for (b, block) in func.blocks.iter_mut().enumerate() {
        if !reachable[b] && !block.is_dead {
            block.is_dead = true;
            changes += 1;
        }
    }
    changes
}

// =============================================================================
// PASS 2: CONSTANT FOLDING
// =============================================================================

/// Вычисляет результаты инструкций с константными операндами.
///
/// Алгоритм:
/// 1. Строим карту `ValueId -> константное значение` (если значение — константа).
/// 2. Для каждой инструкции: если все операнды константны -> вычисляем результат,
///    добавляем его в пул констант, заменяем инструкцию на `MIRInst::Const(id)`,
///    вызываем `replace_all_uses_with`.
fn constant_folding(module: &mut MIRModule, func_id: FunctionId) -> usize {
    let mut changes = 0;

    // Строим карту: ValueId -> (ConstantId, Literal)
    // Сначала собираем все значения которые уже являются Const
    let const_vals: Vec<(ValueId, ConstantId)> = {
        let func = &module.functions[func_id];
        let mut cv = Vec::new();
        for block in func.live_blocks() {
            for (inst_idx, inst) in block.instructions.iter().enumerate() {
                if let MIRInst::Const(cid) = inst {
                    if let Some(val_id) = func
                        .values
                        .iter()
                        .position(|v| v.def == crate::mir::ValueDef::Inst(block.id, inst_idx))
                    {
                        cv.push((val_id, *cid));
                    }
                }
            }
        }
        cv
    };

    // Карта ValueId -> ConstantId для быстрого lookup
    let mut val_to_const: std::collections::HashMap<ValueId, ConstantId> =
        const_vals.into_iter().collect();

    // Обходим все инструкции в живых блоках
    let block_ids: Vec<BlockId> = module.functions[func_id]
        .blocks
        .iter()
        .filter(|b| !b.is_dead)
        .map(|b| b.id)
        .collect();

    for block_id in block_ids {
        let inst_count = module.functions[func_id].blocks[block_id]
            .instructions
            .len();
        for inst_idx in 0..inst_count {
            let result = try_fold_inst(module, func_id, block_id, inst_idx, &val_to_const);
            if let Some((new_const_id, result_val_id)) = result {
                // Заменяем инструкцию на Const
                module.functions[func_id].blocks[block_id].instructions[inst_idx] =
                    MIRInst::Const(new_const_id);
                // Обновляем карту
                val_to_const.insert(result_val_id, new_const_id);
                changes += 1;
            }
        }
    }

    changes
}

/// Пытается свернуть одну инструкцию.
/// Возвращает `Some((new_const_id, result_val_id))` если свёртка удалась.
fn try_fold_inst(
    module: &mut MIRModule,
    func_id: FunctionId,
    block_id: BlockId,
    inst_idx: usize,
    val_to_const: &std::collections::HashMap<ValueId, ConstantId>,
) -> Option<(ConstantId, ValueId)> {
    let inst = module.functions[func_id].blocks[block_id].instructions[inst_idx].clone();

    // Вспомогательная: получить u64 значение константы по ValueId
    let get_u64 = |val: ValueId| -> Option<u64> {
        let cid = val_to_const.get(&val)?;
        match &module.constants[*cid].value {
            Literal::Int(v) => Some(*v),
            Literal::Bool(b) => Some(*b as u64),
            _ => None,
        }
    };
    let get_i64 = |val: ValueId| -> Option<i64> { get_u64(val).map(|v| v as i64) };
    let get_f64 = |val: ValueId| -> Option<f64> {
        let cid = val_to_const.get(&val)?;
        match &module.constants[*cid].value {
            Literal::Float(v) => Some(*v),
            _ => None,
        }
    };

    // Находим ValueId результата этой инструкции
    let result_val_id = module.functions[func_id]
        .values
        .iter()
        .position(|v| v.def == crate::mir::ValueDef::Inst(block_id, inst_idx))?;

    let folded: Option<(Type, Literal)> = match &inst {
        // --- Целочисленная арифметика ---
        MIRInst::Add(a, b) => Some((
            Type::U64,
            Literal::Int(get_u64(*a)?.wrapping_add(get_u64(*b)?)),
        )),
        MIRInst::Sub(a, b) => Some((
            Type::U64,
            Literal::Int(get_u64(*a)?.wrapping_sub(get_u64(*b)?)),
        )),
        MIRInst::Mul(a, b) => Some((
            Type::U64,
            Literal::Int(get_u64(*a)?.wrapping_mul(get_u64(*b)?)),
        )),
        MIRInst::UDiv(a, b) => {
            let divisor = get_u64(*b)?;
            if divisor == 0 {
                return None;
            } // деление на ноль не сворачиваем
            Some((Type::U64, Literal::Int(get_u64(*a)? / divisor)))
        }
        MIRInst::IDiv(a, b) => {
            let divisor = get_i64(*b)?;
            if divisor == 0 {
                return None;
            }
            let dividend = get_i64(*a)?;
            if dividend == i64::MIN && divisor == -1 {
                return None;
            } // overflow
            Some((Type::I64, Literal::Int((dividend / divisor) as u64)))
        }
        MIRInst::URem(a, b) => {
            let d = get_u64(*b)?;
            if d == 0 {
                return None;
            }
            Some((Type::U64, Literal::Int(get_u64(*a)? % d)))
        }
        MIRInst::IRem(a, b) => {
            let d = get_i64(*b)?;
            if d == 0 {
                return None;
            }
            Some((Type::I64, Literal::Int((get_i64(*a)? % d) as u64)))
        }
        MIRInst::Neg(a) => Some((Type::I64, Literal::Int(get_i64(*a)?.wrapping_neg() as u64))),

        // --- Float арифметика ---
        MIRInst::FAdd(a, b) => Some((Type::F64, Literal::Float(get_f64(*a)? + get_f64(*b)?))),
        MIRInst::FSub(a, b) => Some((Type::F64, Literal::Float(get_f64(*a)? - get_f64(*b)?))),
        MIRInst::FMul(a, b) => Some((Type::F64, Literal::Float(get_f64(*a)? * get_f64(*b)?))),
        MIRInst::FDiv(a, b) => Some((Type::F64, Literal::Float(get_f64(*a)? / get_f64(*b)?))),
        MIRInst::FRem(a, b) => Some((Type::F64, Literal::Float(get_f64(*a)? % get_f64(*b)?))),
        MIRInst::FNeg(a) => Some((Type::F64, Literal::Float(-get_f64(*a)?))),
        MIRInst::FAbs(a) => Some((Type::F64, Literal::Float(get_f64(*a)?.abs()))),

        // --- Битовые ---
        MIRInst::And(a, b) => Some((Type::U64, Literal::Int(get_u64(*a)? & get_u64(*b)?))),
        MIRInst::Or(a, b) => Some((Type::U64, Literal::Int(get_u64(*a)? | get_u64(*b)?))),
        MIRInst::Xor(a, b) => Some((Type::U64, Literal::Int(get_u64(*a)? ^ get_u64(*b)?))),
        MIRInst::Not(a) => Some((Type::U64, Literal::Int(!get_u64(*a)?))),
        MIRInst::Shl(a, b) => {
            let shift = get_u64(*b)? & 63;
            Some((
                Type::U64,
                Literal::Int(get_u64(*a)?.wrapping_shl(shift as u32)),
            ))
        }
        MIRInst::Shr(a, b) => {
            let shift = get_u64(*b)? & 63;
            Some((
                Type::U64,
                Literal::Int(get_u64(*a)?.wrapping_shr(shift as u32)),
            ))
        }
        MIRInst::Sar(a, b) => {
            let shift = get_u64(*b)? & 63;
            Some((
                Type::I64,
                Literal::Int(get_i64(*a)?.wrapping_shr(shift as u32) as u64),
            ))
        }

        // --- Сравнения ---
        MIRInst::ICmp { op, lhs, rhs } => {
            use crate::mir::ICmpOp;
            let l = get_u64(*lhs)?;
            let r = get_u64(*rhs)?;
            let li = l as i64;
            let ri = r as i64;
            let result = match op {
                ICmpOp::Eq => l == r,
                ICmpOp::Ne => l != r,
                ICmpOp::Ult => l < r,
                ICmpOp::Ule => l <= r,
                ICmpOp::Ugt => l > r,
                ICmpOp::Uge => l >= r,
                ICmpOp::Slt => li < ri,
                ICmpOp::Sle => li <= ri,
                ICmpOp::Sgt => li > ri,
                ICmpOp::Sge => li >= ri,
            };
            Some((Type::Bool, Literal::Bool(result)))
        }
        MIRInst::FCmp { op, lhs, rhs } => {
            use crate::mir::FCmpOp;
            let l = get_f64(*lhs)?;
            let r = get_f64(*rhs)?;
            let result = match op {
                FCmpOp::Eq => l == r,
                FCmpOp::Ne => l != r,
                FCmpOp::Lt => l < r,
                FCmpOp::Le => l <= r,
                FCmpOp::Gt => l > r,
                FCmpOp::Ge => l >= r,
            };
            Some((Type::Bool, Literal::Bool(result)))
        }

        // --- Mov: копирование константы ---
        MIRInst::Mov(src) => {
            let cid = val_to_const.get(src)?;
            let lit = module.constants[*cid].value.clone();
            let ty = module.constants[*cid].typ;
            Some((ty, lit))
        }

        // Нельзя свернуть: Const (уже константа), Load, Store, Call, Cast, Select
        _ => None,
    };

    if let Some((typ, lit)) = folded {
        let name = format!("__fold_{}", module.constants.len());
        let const_id = module.intern_constant(name, typ, lit);
        Some((const_id, result_val_id))
    } else {
        None
    }
}

// =============================================================================
// PASS 3: DEAD CODE ELIMINATION
// =============================================================================

/// Помечает SSA-значения мёртвыми если их use-list пуст и нет side-effects.
///
/// Side-effect инструкции которые нельзя удалять даже без uses:
/// - `Store` — запись в память
/// - `Call`  — вызов функции (может иметь side-effects)
///
/// Алгоритм worklist: начинаем с явно мёртвых, итеративно распространяем.
fn dead_code_elimination(module: &mut MIRModule, func_id: FunctionId) -> usize {
    let func = &mut module.functions[func_id];
    let mut changes = 0;

    // Строим множество значений используемых в terminators живых блоков.
    // Lowerer не трекал uses terminators, поэтому делаем это здесь.
    // Без этого Ret([%x]) не добавляет use для %x -> DCE убивает %x.
    let terminator_used: std::collections::HashSet<ValueId> = func
        .blocks
        .iter()
        .filter(|b| !b.is_dead)
        .flat_map(|b| b.terminator.operands())
        .collect();

    // Worklist: значения кандидаты на удаление.
    // Значение — кандидат если: uses пуст И не используется в terminators.
    let mut worklist: VecDeque<ValueId> = func
        .values
        .iter()
        .enumerate()
        .filter(|(id, v)| !v.is_dead && v.uses.is_empty() && !terminator_used.contains(id))
        .map(|(id, _)| id)
        .collect();

    while let Some(val_id) = worklist.pop_front() {
        let val = &func.values[val_id];
        if val.is_dead {
            continue;
        }
        // Не удаляем если есть живые uses или значение используется в terminator
        if val.uses.iter().any(|&u| !func.values[u].is_dead) {
            continue;
        }
        if terminator_used.contains(&val_id) {
            continue;
        }

        // Проверяем что инструкция не имеет side-effects
        let has_side_effect = match &val.def {
            crate::mir::ValueDef::Inst(block_id, inst_idx) => {
                match func.blocks[*block_id].instructions.get(*inst_idx) {
                    Some(MIRInst::Store { .. }) => true,
                    Some(MIRInst::Call { .. }) => true,
                    // Print-инструкции — side-effects (вывод в stdout)
                    Some(MIRInst::PrintInt(..))
                    | Some(MIRInst::PrintUInt(..))
                    | Some(MIRInst::PrintFloat(..))
                    | Some(MIRInst::PrintStr(..)) => true,
                    _ => false,
                }
            }
            _ => false,
        };

        if has_side_effect {
            continue;
        }

        // Помечаем мёртвым
        func.values[val_id].is_dead = true;
        changes += 1;

        // Освобождаем uses операндов: они могут стать мёртвыми тоже
        let def = func.values[val_id].def.clone();
        if let crate::mir::ValueDef::Inst(block_id, inst_idx) = def {
            if let Some(inst) = func.blocks[block_id].instructions.get(inst_idx).cloned() {
                let operands = inst_operands_for_dce(&inst);
                for operand in operands {
                    if operand < func.values.len() {
                        // Убираем val_id из use-list операнда
                        func.values[operand].uses.retain(|&u| u != val_id);
                        // Если операнд теперь без uses — добавляем в worklist
                        if func.values[operand].uses.is_empty()
                            && !func.values[operand].is_dead
                            && !terminator_used.contains(&operand)
                        {
                            worklist.push_back(operand);
                        }
                    }
                }
            }
        }
    }

    changes
}

fn inst_operands_for_dce(inst: &MIRInst) -> Vec<ValueId> {
    match inst {
        MIRInst::Add(a, b)
        | MIRInst::Sub(a, b)
        | MIRInst::Mul(a, b)
        | MIRInst::UDiv(a, b)
        | MIRInst::IDiv(a, b)
        | MIRInst::URem(a, b)
        | MIRInst::IRem(a, b)
        | MIRInst::FAdd(a, b)
        | MIRInst::FSub(a, b)
        | MIRInst::FMul(a, b)
        | MIRInst::FDiv(a, b)
        | MIRInst::FRem(a, b)
        | MIRInst::And(a, b)
        | MIRInst::Or(a, b)
        | MIRInst::Xor(a, b)
        | MIRInst::Shl(a, b)
        | MIRInst::Shr(a, b)
        | MIRInst::Sar(a, b) => vec![*a, *b],
        MIRInst::Neg(a)
        | MIRInst::FNeg(a)
        | MIRInst::FAbs(a)
        | MIRInst::Not(a)
        | MIRInst::Mov(a) => vec![*a],
        MIRInst::ICmp { lhs, rhs, .. } | MIRInst::FCmp { lhs, rhs, .. } => vec![*lhs, *rhs],
        MIRInst::Cast { src, .. } => vec![*src],
        MIRInst::Load { addr, .. } => vec![*addr],
        MIRInst::Store { addr, value } => vec![*addr, *value],
        MIRInst::Select {
            cond,
            then_v,
            else_v,
        } => vec![*cond, *then_v, *else_v],
        MIRInst::Call { args, .. } => args.clone(),
        MIRInst::Const(_) => vec![],
        MIRInst::PrintInt(a)
        | MIRInst::PrintUInt(a)
        | MIRInst::PrintFloat(a)
        | MIRInst::PrintStr(a) => vec![*a],
    }
}

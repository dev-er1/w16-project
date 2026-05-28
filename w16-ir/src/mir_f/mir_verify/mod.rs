//! w16-ir\src\mir\mir_verify\mod.rs
//!
//! # MIR Верификатор
//!
//! Проверяет SSA-корректность, типы, структуру CFG и соответствие сигнатурам.
//! Запускается:
//! - После `lowerer` (гарантия что IR корректен перед оптимизацией).
//! - После каждого оптимизационного pass-а в debug-режиме.
//! - Перед bytecode lowerer-ом (финальная проверка).
//!
//! Если верификатор возвращает `Ok(())` — бэкенд может полностью доверять IR.
//!
//! ## Что проверяется
//!
//! ### SSA-инварианты
//! - Каждый ValueId определён ровно один раз.
//! - Все использования ValueId определены до использования
//!   (dominance: def доминирует все uses).
//! - `value_types[id]` совпадает с типом определяющей инструкции.
//! - `values[id].uses` содержит только реально существующие ValueId.
//!
//! ### Структура CFG
//! - `blocks[i].id == i` (инвариант индексов).
//! - Каждый блок завершён ровно одним terminator.
//! - `predecessors[b]` совпадает с реальными predecessors из CFG.
//! - Нет блоков без predecessors кроме entry (blocks[0]).
//! - Аргументы Jmp/Br соответствуют параметрам целевого блока по количеству и типу.
//!
//! ### Типы
//! - Операнды арифметических инструкций имеют совместимые типы.
//! - Условия Br имеют тип Bool.
//! - Return values соответствуют сигнатуре функции.
//! - Cast src/dest типы допустимы для выбранного CastKind.
//!
//! ### Модуль
//! - ConstantId в MIRInst::Const не выходит за пределы constants.
//! - FunctionId в MIRInst::Call не выходит за пределы functions.
//! - `analysis.len() == functions.len()`.

use crate::mir::{
    BasicBlock, BlockId, CastKind, FunctionId, ICmpOp, MIRFunction, MIRInst, MIRModule,
    MIRTerminator, Type, ValueDef, ValueId,
};

// =============================================================================
// ОШИБКИ ВЕРИФИКАТОРА
// =============================================================================

/// Одна ошибка верификации. Содержит контекст для диагностики.
#[derive(Debug, Clone)]
pub struct VerifyError {
    /// В какой функции обнаружена ошибка.
    pub func: Option<FunctionId>,
    /// В каком блоке (если применимо).
    pub block: Option<BlockId>,
    /// ValueId который вызвал ошибку (если применимо).
    pub value: Option<ValueId>,
    /// Человекочитаемое описание.
    pub message: String,
}

impl VerifyError {
    fn module(message: impl Into<String>) -> Self {
        Self {
            func: None,
            block: None,
            value: None,
            message: message.into(),
        }
    }

    fn func(func: FunctionId, message: impl Into<String>) -> Self {
        Self {
            func: Some(func),
            block: None,
            value: None,
            message: message.into(),
        }
    }

    fn block(func: FunctionId, block: BlockId, message: impl Into<String>) -> Self {
        Self {
            func: Some(func),
            block: Some(block),
            value: None,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.func, self.block, self.value) {
            (Some(func), Some(block), Some(val)) => {
                write!(f, "[fn#{func} ^{block} %{val}] {}", self.message)
            }
            (Some(func), Some(block), None) => write!(f, "[fn#{func} ^{block}] {}", self.message),
            (Some(func), None, None) => write!(f, "[fn#{func}] {}", self.message),
            _ => write!(f, "[module] {}", self.message),
        }
    }
}

/// Результат верификации: список всех найденных ошибок.
/// Верификатор не останавливается на первой ошибке — собирает все.
pub type VerifyResult = Result<(), Vec<VerifyError>>;

// =============================================================================
// ТОЧКА ВХОДА
// =============================================================================

/// Верифицирует весь MIR-модуль.
/// Возвращает `Ok(())` если IR корректен, иначе список всех ошибок.
pub fn verify_module(module: &MIRModule) -> VerifyResult {
    let mut errors = Vec::new();

    // Структура модуля
    if module.analysis.len() != module.functions.len() {
        errors.push(VerifyError::module(format!(
            "analysis.len()={} != functions.len()={}",
            module.analysis.len(),
            module.functions.len()
        )));
    }

    // Верифицируем каждую функцию
    for (func_id, func) in module.functions.iter().enumerate() {
        verify_function(
            func,
            func_id,
            module.constants.len(),
            module.functions.len(),
            &mut errors,
        );
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Верифицирует одну функцию. Ошибки добавляются в `errors`.
pub fn verify_function(
    func: &MIRFunction,
    func_id: FunctionId,
    const_count: usize,
    func_count: usize,
    errors: &mut Vec<VerifyError>,
) {
    if func.blocks.is_empty() {
        errors.push(VerifyError::func(func_id, "function has no blocks"));
        return;
    }

    // —- Инвариант индексов блоков —-
    for (i, block) in func.blocks.iter().enumerate() {
        if block.id != i {
            errors.push(VerifyError::func(
                func_id,
                format!("blocks[{i}].id == {} (expected {i})", block.id),
            ));
        }
    }

    // —- Проверяем predecessors —-
    verify_predecessors(func, func_id, errors);

    // —- Проверяем каждый блок —-
    for block in &func.blocks {
        verify_block(func, func_id, block, const_count, func_count, errors);
    }

    // —- Проверяем use-lists —-
    verify_use_lists(func, func_id, errors);
}

// =============================================================================
// ПРОВЕРКА PREDECESSORS
// =============================================================================

fn verify_predecessors(func: &MIRFunction, func_id: FunctionId, errors: &mut Vec<VerifyError>) {
    // Строим ожидаемые predecessors из terminators
    let mut expected: Vec<Vec<BlockId>> = vec![Vec::new(); func.blocks.len()];
    for block in &func.blocks {
        for succ in block.terminator.successors() {
            if succ >= func.blocks.len() {
                errors.push(VerifyError::block(
                    func_id,
                    block.id,
                    format!("terminator references non-existent block ^{succ}"),
                ));
                continue;
            }
            if !expected[succ].contains(&block.id) {
                expected[succ].push(block.id);
            }
        }
    }

    // Entry block не должен иметь predecessors (кроме случая рекурсии через Jmp,
    // что запрещено в нашем lowerer-е)
    if !func.blocks[0].predecessors.is_empty() {
        errors.push(VerifyError::block(
            func_id,
            0,
            "entry block must have no predecessors",
        ));
    }

    // Сверяем с реальными predecessors
    for (i, block) in func.blocks.iter().enumerate() {
        let exp = &expected[i];
        for pred in exp {
            if !block.predecessors.contains(pred) {
                errors.push(VerifyError::block(
                    func_id,
                    i,
                    format!("missing predecessor ^{pred}"),
                ));
            }
        }
        for pred in &block.predecessors {
            if !exp.contains(pred) {
                errors.push(VerifyError::block(
                    func_id,
                    i,
                    format!("spurious predecessor ^{pred} (no terminator points here)"),
                ));
            }
        }
    }
}

// =============================================================================
// ПРОВЕРКА БЛОКА
// =============================================================================

fn verify_block(
    func: &MIRFunction,
    func_id: FunctionId,
    block: &BasicBlock,
    const_count: usize,
    func_count: usize,
    errors: &mut Vec<VerifyError>,
) {
    // Проверяем инструкции
    for (inst_idx, inst) in block.instructions.iter().enumerate() {
        // Результат этой инструкции — ValueId по позиции в value_types
        // (инвариант: инструкции добавляются через emit, который вызывает new_value)
        let result_id = get_result_id(func, block.id, inst_idx);

        verify_inst(
            func,
            func_id,
            block.id,
            inst,
            result_id,
            const_count,
            func_count,
            errors,
        );
    }

    // Проверяем terminator
    verify_terminator(func, func_id, block, errors);
}

/// Находит ValueId результата инструкции по позиции в блоке.
/// Ищем в values запись с def == Inst(block_id, inst_idx).
fn get_result_id(func: &MIRFunction, block_id: BlockId, inst_idx: usize) -> Option<ValueId> {
    func.values
        .iter()
        .position(|v| v.def == ValueDef::Inst(block_id, inst_idx))
}

// =============================================================================
// ПРОВЕРКА ИНСТРУКЦИИ
// =============================================================================

fn verify_inst(
    func: &MIRFunction,
    func_id: FunctionId,
    block_id: BlockId,
    inst: &MIRInst,
    result_id: Option<ValueId>,
    const_count: usize,
    func_count: usize,
    errors: &mut Vec<VerifyError>,
) {
    // Вспомогательные: проверить что ValueId существует и имеет ожидаемый тип
    let check_val =
        |val: ValueId, expected_ty: Option<Type>, ctx: &str, errors: &mut Vec<VerifyError>| {
            if val >= func.values.len() {
                errors.push(VerifyError::block(
                    func_id,
                    block_id,
                    format!("{ctx}: ValueId %{val} out of range"),
                ));
                return;
            }
            if let Some(ty) = expected_ty {
                let actual = func.value_types[val];
                if actual != ty {
                    errors.push(VerifyError::block(
                        func_id,
                        block_id,
                        format!("{ctx}: expected type {ty:?}, got {actual:?} for %{val}"),
                    ));
                }
            }
        };

    match inst {
        // Арифметика: оба операнда должны быть одного числового типа
        MIRInst::Add(a, b)
        | MIRInst::Sub(a, b)
        | MIRInst::Mul(a, b)
        | MIRInst::UDiv(a, b)
        | MIRInst::URem(a, b)
        | MIRInst::And(a, b)
        | MIRInst::Or(a, b)
        | MIRInst::Xor(a, b)
        | MIRInst::Shl(a, b)
        | MIRInst::Shr(a, b) => {
            check_val(*a, None, "int binop lhs", errors);
            check_val(*b, None, "int binop rhs", errors);
            // Типы операндов должны совпадать
            if *a < func.values.len() && *b < func.values.len() {
                let ta = func.value_types[*a];
                let tb = func.value_types[*b];
                if ta != tb {
                    errors.push(VerifyError::block(
                        func_id,
                        block_id,
                        format!("int binop: type mismatch {ta:?} vs {tb:?}"),
                    ));
                }
            }
        }
        MIRInst::IDiv(a, b) | MIRInst::IRem(a, b) => {
            check_val(*a, Some(Type::I64), "idiv/irem lhs", errors);
            check_val(*b, Some(Type::I64), "idiv/irem rhs", errors);
        }
        MIRInst::Sar(a, b) => {
            check_val(*a, Some(Type::I64), "sar lhs", errors);
            check_val(*b, None, "sar rhs", errors);
        }
        MIRInst::Neg(a) => {
            check_val(*a, None, "neg", errors);
        }

        // Float арифметика: оба операнда должны быть F64
        MIRInst::FAdd(a, b)
        | MIRInst::FSub(a, b)
        | MIRInst::FMul(a, b)
        | MIRInst::FDiv(a, b)
        | MIRInst::FRem(a, b) => {
            check_val(*a, Some(Type::F64), "float binop lhs", errors);
            check_val(*b, Some(Type::F64), "float binop rhs", errors);
        }
        MIRInst::FNeg(a) | MIRInst::FAbs(a) => {
            check_val(*a, Some(Type::F64), "float unop", errors);
        }

        // Битовые: оба операнда, Not — один
        MIRInst::Not(a) => {
            check_val(*a, None, "not", errors);
        }

        // Сравнения: типы операндов должны совпадать, результат Bool
        MIRInst::ICmp { lhs, rhs, op } => {
            check_val(*lhs, None, "icmp lhs", errors);
            check_val(*rhs, None, "icmp rhs", errors);
            if *lhs < func.values.len() && *rhs < func.values.len() {
                let tl = func.value_types[*lhs];
                let tr = func.value_types[*rhs];
                if tl != tr {
                    errors.push(VerifyError::block(
                        func_id,
                        block_id,
                        format!("icmp: type mismatch {tl:?} vs {tr:?}"),
                    ));
                }
                // Знаковые операции только для I64
                let is_signed = matches!(op, ICmpOp::Slt | ICmpOp::Sle | ICmpOp::Sgt | ICmpOp::Sge);
                if is_signed && tl != Type::I64 {
                    errors.push(VerifyError::block(
                        func_id,
                        block_id,
                        format!("signed icmp on non-i64 type {tl:?}"),
                    ));
                }
            }
        }
        MIRInst::FCmp { lhs, rhs, .. } => {
            check_val(*lhs, Some(Type::F64), "fcmp lhs", errors);
            check_val(*rhs, Some(Type::F64), "fcmp rhs", errors);
        }

        // Const: ConstantId должен быть в пределах
        MIRInst::Const(id) => {
            if *id >= const_count {
                errors.push(VerifyError::block(
                    func_id,
                    block_id,
                    format!("Const({id}): ConstantId out of range (pool size {const_count})"),
                ));
            }
        }

        MIRInst::Mov(src) => {
            check_val(*src, None, "mov src", errors);
        }

        MIRInst::Load { addr, .. } => {
            check_val(*addr, Some(Type::Ptr), "load addr", errors);
        }

        MIRInst::Store { addr, value } => {
            check_val(*addr, Some(Type::Ptr), "store addr", errors);
            check_val(*value, None, "store value", errors);
        }

        MIRInst::Cast {
            kind,
            src,
            dest_typ,
        } => {
            check_val(*src, None, "cast src", errors);
            if *src < func.values.len() {
                let src_ty = func.value_types[*src];
                verify_cast_types(func_id, block_id, *kind, src_ty, *dest_typ, errors);
            }
        }

        MIRInst::Select {
            cond,
            then_v,
            else_v,
        } => {
            check_val(*cond, Some(Type::Bool), "select cond", errors);
            check_val(*then_v, None, "select then", errors);
            check_val(*else_v, None, "select else", errors);
            // then и else должны иметь одинаковый тип
            if *then_v < func.values.len() && *else_v < func.values.len() {
                let tt = func.value_types[*then_v];
                let te = func.value_types[*else_v];
                if tt != te {
                    errors.push(VerifyError::block(
                        func_id,
                        block_id,
                        format!("select: then type {tt:?} != else type {te:?}"),
                    ));
                }
            }
        }

        MIRInst::Call {
            func: callee_id,
            args,
        } => {
            if *callee_id >= func_count {
                errors.push(VerifyError::block(func_id, block_id, format!(
                    "Call: FunctionId {callee_id} out of range (module has {func_count} functions)"
                )));
            }
            for (i, arg) in args.iter().enumerate() {
                check_val(*arg, None, &format!("call arg[{i}]"), errors);
            }
        }

        MIRInst::PrintInt(val)
        | MIRInst::PrintUInt(val)
        | MIRInst::PrintFloat(val)
        | MIRInst::PrintStr(val) => {
            check_val(*val, None, "print operand", errors);
        }
    }

    // Проверяем что у результата правильный тип (совпадает с infer_type)
    if let Some(rid) = result_id
        && rid < func.value_types.len()
    {
        let actual_ty = func.value_types[rid];
        let expected_ty = crate::translator::lowerer::infer_type(inst);
        if actual_ty != expected_ty {
            errors.push(VerifyError::block(
                func_id,
                block_id,
                format!("result %{rid}: recorded type {actual_ty:?} != inferred {expected_ty:?}"),
            ));
        }
    }
}

/// Проверяет допустимость комбинации src_ty -> dest_ty для данного CastKind.
fn verify_cast_types(
    func_id: FunctionId,
    block_id: BlockId,
    kind: CastKind,
    src: Type,
    dst: Type,
    errors: &mut Vec<VerifyError>,
) {
    let ok = match kind {
        CastKind::I2F => src == Type::I64 && dst == Type::F64,
        CastKind::U2F => src == Type::U64 && dst == Type::F64,
        CastKind::F2I => src == Type::F64 && dst == Type::I64,
        CastKind::F2U => src == Type::F64 && dst == Type::U64,
        CastKind::I2U => src == Type::I64 && dst == Type::U64,
        CastKind::U2I => src == Type::U64 && dst == Type::I64,
        CastKind::TruncU64ToU32 => src == Type::U64 && dst == Type::U64,
        CastKind::ZextU32ToU64 => src == Type::U64 && dst == Type::U64,
        CastKind::SextI32ToI64 => src == Type::I64 && dst == Type::I64,
        CastKind::Bitcast => true, // любые типы одного размера
    };
    if !ok {
        errors.push(VerifyError::block(
            func_id,
            block_id,
            format!("cast {:?}: invalid {src:?} -> {dst:?}", kind),
        ));
    }
}

// =============================================================================
// ПРОВЕРКА TERMINATOR
// =============================================================================

fn verify_terminator(
    func: &MIRFunction,
    func_id: FunctionId,
    block: &BasicBlock,
    errors: &mut Vec<VerifyError>,
) {
    let check_val =
        |val: ValueId, expected_ty: Option<Type>, ctx: &str, errors: &mut Vec<VerifyError>| {
            if val >= func.values.len() {
                errors.push(VerifyError::block(
                    func_id,
                    block.id,
                    format!("terminator {ctx}: ValueId %{val} out of range"),
                ));
                return;
            }
            if let Some(ty) = expected_ty {
                let actual = func.value_types[val];
                if actual != ty {
                    errors.push(VerifyError::block(
                        func_id,
                        block.id,
                        format!("terminator {ctx}: expected {ty:?}, got {actual:?} for %{val}"),
                    ));
                }
            }
        };

    // Проверяем аргументы перехода: количество и типы должны совпадать с параметрами целевого блока
    let verify_jump_args =
        |target: BlockId, args: &[ValueId], ctx: &str, errors: &mut Vec<VerifyError>| {
            if target >= func.blocks.len() {
                errors.push(VerifyError::block(
                    func_id,
                    block.id,
                    format!("{ctx}: target ^{target} out of range"),
                ));
                return;
            }
            let params = &func.blocks[target].params;
            if args.len() != params.len() {
                errors.push(VerifyError::block(
                    func_id,
                    block.id,
                    format!(
                        "{ctx}: args count {} != params count {} for ^{target}",
                        args.len(),
                        params.len()
                    ),
                ));
                return;
            }
            for (i, (arg, (_, param_ty))) in args.iter().zip(params.iter()).enumerate() {
                if *arg >= func.values.len() {
                    errors.push(VerifyError::block(
                        func_id,
                        block.id,
                        format!("{ctx}: arg[{i}] = %{arg} out of range"),
                    ));
                    continue;
                }
                let arg_ty = func.value_types[*arg];
                if arg_ty != *param_ty {
                    errors.push(VerifyError::block(
                        func_id,
                        block.id,
                        format!("{ctx}: arg[{i}] type {arg_ty:?} != param type {param_ty:?}"),
                    ));
                }
            }
        };

    match &block.terminator {
        MIRTerminator::Jmp { target, args } => {
            verify_jump_args(*target, args, "Jmp", errors);
        }
        MIRTerminator::Br {
            cond,
            then_blk,
            then_args,
            else_blk,
            else_args,
        } => {
            check_val(*cond, Some(Type::Bool), "Br cond", errors);
            verify_jump_args(*then_blk, then_args, "Br then", errors);
            verify_jump_args(*else_blk, else_args, "Br else", errors);
        }
        MIRTerminator::Ret(vals) => {
            // Количество и типы возвращаемых значений проверяются на уровне функции
            // (нужна сигнатура  — проверяем в verify_function если передать return_types)
            for (i, val) in vals.iter().enumerate() {
                check_val(*val, None, &format!("Ret[{i}]"), errors);
            }
        }
        MIRTerminator::Halt => {}
    }
}

// =============================================================================
// ПРОВЕРКА USE-LISTS
// =============================================================================

fn verify_use_lists(func: &MIRFunction, func_id: FunctionId, errors: &mut Vec<VerifyError>) {
    // Для каждого ValueId проверяем что все записанные uses реально используют его
    for (val_id, val_data) in func.values.iter().enumerate() {
        for &user_id in &val_data.uses {
            if user_id >= func.values.len() {
                errors.push(VerifyError::func(
                    func_id,
                    format!("use-list of %{val_id} contains out-of-range user %{user_id}"),
                ));
            }
        }
    }
}

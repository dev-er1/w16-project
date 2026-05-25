// w16-ir\src\mir_optimizer\loop_closed_form.rs
//
//! # Оптимизация: замена цикла closed-form выражением
//!
//! ## Что делает этот pass
//!
//! Находит циклы которые полностью описываются линейными индуктивными
//! переменными с известным trip count, и заменяет весь цикл на прямые
//! присваивания результатов.
//!
//! ## Пример
//!
//! До оптимизации (MIR):
//! ```text
//! ^entry:
//!   %zero = const 0
//!   %limit = const 100000000
//!   jmp ^loop_header(%zero, %zero)
//!
//! ^loop_header(%i: u64, %sum: u64):
//!   %cond = icmp.ult %i, %limit
//!   br %cond, ^loop_body(%i, %sum), ^exit(%sum)
//!
//! ^loop_body(%i2: u64, %sum2: u64):
//!   %sum_next = add %sum2, %i2
//!   %i_next = add %i2, %one
//!   jmp ^loop_header(%i_next, %sum_next)
//!
//! ^exit(%result: u64):
//!   ret %result
//! ```
//!
//! После оптимизации:
//! ```text
//! ^entry:
//!   %i_final = const 100000000        ; i = base + step * N = 0 + 1 * N
//!   %sum_final = const 4999999950000000  ; sum = N*(N-1)/2
//!   jmp ^exit(%sum_final)               ; header и body помечены is_dead
//!
//! ^exit(%result: u64):
//!   ret %result
//! ```
//!
//! ## Условия применимости
//!
//! Pass применяется если:
//! 1. Trip count известен статически (константная граница).
//! 2. Все phi-параметры header-блока — это IV (Linear или SumOfLinear).
//! 3. Цикл не содержит Store или Call (нет side-effects).
//! 4. Нет вложенных break (единственный выход из цикла — через exit condition).

use crate::mir::{
    FunctionId, Literal, MIRInst, MIRModule, MIRTerminator, Type, ValueData, ValueDef, ValueId,
};
use crate::mir_f::mir_analyze::{
    DominatorTree,
    induction_vars::analyze_loop_ivs_full,
    loop_info::{Loop, LoopInfo},
};

/// Запускает pass для всех функций модуля.
/// Возвращает количество схлопнутых циклов.
pub fn run(module: &mut MIRModule) -> usize {
    let mut total = 0;
    let func_count = module.functions.len();
    for func_id in 0..func_count {
        total += run_on_function(module, func_id);
    }
    total
}

/// Запускает pass для одной функции.
fn run_on_function(module: &mut MIRModule, func_id: FunctionId) -> usize {
    let domtree = DominatorTree::build(&module.functions[func_id]);
    let loop_info = LoopInfo::build(&module.functions[func_id], &domtree);

    if loop_info.loops.is_empty() {
        return 0;
    }

    let mut collapsed = 0;

    // Обрабатываем циклы от внутренних к внешним (по размеру тела)
    let mut loop_indices: Vec<usize> = (0..loop_info.loops.len()).collect();
    loop_indices.sort_by_key(|&i| loop_info.loops[i].size());

    for loop_idx in loop_indices {
        let lp = &loop_info.loops[loop_idx];

        // Проверяем применимость
        if !is_eligible(module, func_id, lp) {
            continue;
        }

        // Анализируем IV
        let iv_analysis = analyze_loop_ivs_full(&module.functions[func_id], lp, module);

        let trip_count = match iv_analysis.trip_count {
            Some(tc) => tc,
            None => continue,
        };

        // Проверяем что все phi-параметры header-а покрыты IV
        let header_params: Vec<(ValueId, Type)> =
            module.functions[func_id].blocks[lp.header].params.clone();

        let all_covered = header_params
            .iter()
            .all(|(phi_id, _)| iv_analysis.ivs.contains_key(phi_id));
        for (phi_id, _) in &header_params {
            if iv_analysis.ivs.contains_key(phi_id) {}
        }
        if !all_covered {
            continue;
        }

        // Вычисляем финальные значения всех IV
        let mut final_values: Vec<(ValueId, u64, Type)> = Vec::new();
        let mut ok = true;
        for (phi_id, ty) in &header_params {
            let iv = &iv_analysis.ivs[phi_id];
            match iv.eval(trip_count) {
                Some(val) => final_values.push((*phi_id, val, *ty)),
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            continue;
        }

        // Применяем трансформацию
        apply_closed_form(module, func_id, lp, &final_values, trip_count);
        collapsed += 1;
    }

    collapsed
}

// =============================================================================
// ПРОВЕРКА ПРИМЕНИМОСТИ
// =============================================================================

fn is_eligible(module: &MIRModule, func_id: FunctionId, lp: &Loop) -> bool {
    let func = &module.functions[func_id];

    // Ровно один выход из цикла
    if lp.exits.len() != 1 {
        return false;
    }

    // Нет side-effects в теле цикла
    for &block_id in &lp.body {
        let block = &func.blocks[block_id];
        if block.is_dead {
            continue;
        }
        for inst in &block.instructions {
            match inst {
                MIRInst::Store { .. } => return false,
                MIRInst::Call { .. } => return false,
                _ => {}
            }
        }
    }

    // Header должен заканчиваться Br с одним выходом наружу
    let header = &func.blocks[lp.header];
    matches!(&header.terminator,
        MIRTerminator::Br { then_blk, else_blk, .. }
        if (lp.contains(*then_blk) && !lp.contains(*else_blk))
        || (!lp.contains(*then_blk) && lp.contains(*else_blk))
    )
}

// =============================================================================
// ТРАНСФОРМАЦИЯ
// =============================================================================

/// Применяет closed-form замену:
/// 1. В preheader (блок перед header): добавляем Const инструкции для финальных значений.
/// 2. Terminator preheader меняем с `Jmp header(args)` на `Jmp exit(final_values)`.
/// 3. Все блоки тела цикла (включая header) помечаем is_dead.
fn apply_closed_form(
    module: &mut MIRModule,
    func_id: FunctionId,
    lp: &Loop,
    final_values: &[(ValueId, u64, Type)], // (phi_id, final_u64, type)
    _trip_count: u64,
) {
    let exit_block = lp.exits[0];

    // Находим preheader: predecessor header-а вне цикла
    let func = &module.functions[func_id];
    let preheader = func.blocks[lp.header]
        .predecessors
        .iter()
        .copied()
        .find(|&p| !lp.contains(p));
    let preheader = match preheader {
        Some(p) => p,
        None => return, // нет preheader — пропускаем
    };

    // Параметры exit_block: нужно передать финальные значения в нужном порядке.
    // exit_block получает аргументы из Br в header-е.
    // Нам нужно знать какой phi_id header соответствует какому аргументу exit.
    let exit_args_from_header: Vec<ValueId> = {
        let func = &module.functions[func_id];
        let header = &func.blocks[lp.header];
        match &header.terminator {
            MIRTerminator::Br {
                then_blk,
                then_args,
                else_blk,
                else_args,
                ..
            } => {
                if *then_blk == exit_block {
                    then_args.clone()
                } else if *else_blk == exit_block {
                    else_args.clone()
                } else {
                    return;
                }
            }
            _ => return,
        }
    };

    // Строим new_const_ids из ВСЕХ final_values (не только из exit_args).
    // phi могут использоваться напрямую в Ret/Print после цикла —
    // не только через block args exit_block-а.
    let new_const_ids: Vec<(ValueId, u64, Type)> = final_values.to_vec();

    // Добавляем MIRInst::Const для каждого финального значения
    let mut phi_to_new_val: std::collections::HashMap<ValueId, ValueId> =
        std::collections::HashMap::new();

    for (phi_id, const_val, ty) in &new_const_ids {
        // Добавляем константу в пул модуля
        let const_name = format!("__closed_form_{}", module.constants.len());
        let lit = Literal::Int(*const_val);
        let const_id = module.intern_constant(const_name, *ty, lit);

        // Добавляем инструкцию в preheader
        let inst_idx = module.functions[func_id].blocks[preheader]
            .instructions
            .len();
        module.functions[func_id].blocks[preheader]
            .instructions
            .push(MIRInst::Const(const_id));

        // Создаём новый ValueId для результата этой инструкции
        let new_val_id = module.functions[func_id].values.len();
        module.functions[func_id].values.push(ValueData {
            typ: *ty,
            def: ValueDef::Inst(preheader, inst_idx),
            uses: Vec::new(),
            is_dead: false,
        });
        module.functions[func_id].value_types.push(*ty);

        phi_to_new_val.insert(*phi_id, new_val_id);
    }

    // --- Строим новые аргументы для exit_block ---
    let new_exit_arg_ids: Vec<ValueId> = exit_args_from_header
        .iter()
        .map(|arg| *phi_to_new_val.get(arg).unwrap_or(arg))
        .collect();

    // --- Меняем terminator preheader-а: вместо Jmp header -> Jmp exit ---
    let preheader_term = &mut module.functions[func_id].blocks[preheader].terminator;
    *preheader_term = MIRTerminator::Jmp {
        target: exit_block,
        args: new_exit_arg_ids,
    };

    // Обновляем predecessors: exit_block теперь предшествует preheader, а не header
    let func = &mut module.functions[func_id];
    // Убираем header из predecessors exit_block
    func.blocks[exit_block]
        .predecessors
        .retain(|&p| !lp.contains(p));
    // Добавляем preheader
    if !func.blocks[exit_block].predecessors.contains(&preheader) {
        func.blocks[exit_block].predecessors.push(preheader);
    }

    // --- Заменяем все uses phi-параметров на новые Const значения ---
    // ВАЖНО: phi используются не только через exit_args, но и напрямую
    // в инструкциях после цикла (Ret, PrintUInt и т.д.).
    // Поэтому replace_all_uses_with нужен для ВСЕХ phi из final_values,
    // а не только тех что передаются через exit block args.
    for (phi_id, &new_val_id) in &phi_to_new_val {
        func.replace_all_uses_with(*phi_id, new_val_id);
    }

    // --- Помечаем все блоки цикла как мёртвые —-
    for &block_id in &lp.body {
        func.blocks[block_id].is_dead = true;
    }
    // Помечаем значения в теле цикла как мёртвые.
    // Phi-параметры теперь безопасно пометить — их uses уже перенаправлены.
    for val in &mut func.values {
        let block = match val.def {
            ValueDef::Inst(b, _) => b,
            ValueDef::Param(b) => b,
            _ => continue,
        };
        if lp.body.contains(&block) {
            val.is_dead = true;
        }
    }

    // Инвалидируем анализ
    module.invalidate_analysis(func_id);
}

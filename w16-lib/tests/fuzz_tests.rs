//! # Интеграционное фаззинг-тестирование W16
//!
//! Этот тест использует библиотеку `proptest` для генерации случайного,
//! в том числе некорректного, байт-кода и пула констант.

use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};
use w16_lib::{Bytecode, ConstantPool, ExecutionMode, Instruction, OpCode, W16};

/// Список всех валидных опкодов для базовой генерации.
fn valid_opcodes() -> impl Strategy<Value = OpCode> {
    prop_oneof![
        Just(OpCode::Halt),
        Just(OpCode::NoOp),
        Just(OpCode::Mov),
        Just(OpCode::Load8),
        Just(OpCode::Load16),
        Just(OpCode::LoadConst),
        Just(OpCode::Add),
        Just(OpCode::Sub),
        Just(OpCode::UDiv),
        Just(OpCode::IDiv),
        Just(OpCode::Ld64),
        Just(OpCode::St64),
        Just(OpCode::Jmp),
        Just(OpCode::Jnz),
        Just(OpCode::Ret),
    ]
}

/// Стратегия генерации агрессивных инструкций.
fn aggressive_instruction() -> impl Strategy<Value = Instruction> {
    prop_oneof![
        8 => (valid_opcodes(), any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(opcode, a, b, c)| {
            Instruction { opcode, a, b, c }
        }),
        2 => (Just(OpCode::NoOp), any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(_, a, b, c)| {
            Instruction { opcode: OpCode::NoOp, a, b, c }
        }),
    ]
}

#[test]
fn test_fuzz_bytecode_resilience() {
    let mut runner = TestRunner::new(Config {
        cases: 5000,
        ..Config::default()
    });

    let strategy = (
        prop::collection::vec(aggressive_instruction(), 5..50),
        prop::collection::vec(any::<u8>(), 0..16), // Маленький пул для провокации Out of Bounds
    );

    runner
        .run(&strategy, |(instructions, pool_bytes)| {
            let mut constant_pool = ConstantPool::new();
            constant_pool.data.extend_from_slice(&pool_bytes);

            let mut insts = instructions.clone();
            let inst_len = insts.len();

            for i in 0..inst_len {
                // Искусственно создаем критическую ошибку деления на ноль в 20% NoOp
                if insts[i].opcode == OpCode::NoOp {
                    insts[i].opcode = OpCode::IDiv;
                    insts[i].c = 0;
                }

                if insts[i].opcode == OpCode::Jmp || insts[i].opcode == OpCode::Jnz {
                    if i + 1 < inst_len {
                        // Прыгаем на случайную инструкцию СТРОГО впереди текущей.
                        // Это исключает циклы, но тестирует валидность указателя команд (PC) в JIT.
                        let range_forward = inst_len - (i + 1);
                        let offset = (insts[i].c as usize) % range_forward;
                        insts[i].c = (i + 1 + offset) as u8;
                    } else {
                        // Если это последняя инструкция — превращаем её в Halt
                        insts[i].opcode = OpCode::Halt;
                    }
                }
            }

            // Гарантируем легальный Halt в самом конце, чтобы у процессора была точка выхода
            if let Some(last) = insts.last_mut() {
                last.opcode = OpCode::Halt;
            }

            let bytecode = Bytecode::new(insts, constant_pool);

            // Тест ВМ
            let _vm_res = W16::new()
                .with_mode(ExecutionMode::Interpreter)
                .with_memory_size(512)
                .run_bytecode(&bytecode);

            // Тест JIT-компилятора
            let _jit_res = W16::new()
                .with_mode(ExecutionMode::Jit)
                .run_bytecode(&bytecode);

            Ok(())
        })
        .unwrap();
}

#![allow(dead_code)]

use w16_core::{Bytecode, ConstantPool, Instruction, OpCode};

pub const BENCH_ITERS: u64 = 1_000_000;
pub const BENCH_MEMORY_SIZE: usize = 1024 * 1024;
pub const REGISTER_COUNT: usize = 256;

/// Одна и та же программа для VM, JIT и Orca:
///
/// ```text
/// i = 0
/// step = 1
/// limit = BENCH_ITERS
/// sum = 0
/// while i < limit {
///     sum += i
///     i += step
/// }
/// halt
/// ```
///
/// Результат лежит в `r3`.
pub fn build_sum_loop_program(iterations: u64) -> Bytecode {
    let mut cp = ConstantPool::new();
    cp.data.extend_from_slice(&iterations.to_le_bytes());

    let instructions = vec![
        inst(OpCode::Load16, 0, 0, 0),    // r0 = i = 0
        inst(OpCode::Load16, 1, 1, 0),    // r1 = step = 1
        inst(OpCode::LoadConst, 2, 0, 0), // r2 = limit
        inst(OpCode::Load16, 3, 0, 0),    // r3 = sum = 0
        inst(OpCode::IULt, 4, 0, 2),      // [4] r4 = i < limit
        inst(OpCode::Load16, 5, 11, 0),   // r5 = halt ip
        inst(OpCode::Jz, 4, 5, 0),        // if !r4 jump to halt
        inst(OpCode::Add, 3, 3, 0),       // sum += i
        inst(OpCode::Add, 0, 0, 1),       // i += 1
        inst(OpCode::Load16, 6, 4, 0),    // r6 = loop start
        inst(OpCode::Jmp, 6, 0, 0),       // jump to loop start
        inst(OpCode::Halt, 0, 0, 0),      // [11]
    ];

    Bytecode::new(instructions, cp)
}

/// Быстрая проверка контракта Orca для benchmark workload.
///
/// Это не общий verifier W16 bytecode. Это маленький guard для бенчмарков:
/// он проверяет, что конкретная программа безопасна для unchecked Orca run.
pub fn verify_orca_bench_program(bytecode: &Bytecode) {
    assert!(
        !bytecode.instructions.is_empty(),
        "Orca requires non-empty bytecode"
    );
    assert!(
        matches!(
            bytecode.instructions.last().map(|i| i.opcode),
            Some(OpCode::Halt)
        ),
        "bench program must end with Halt"
    );
    assert!(
        bytecode.constant_pool.data.len() >= 8,
        "bench program must contain u64 iteration limit"
    );

    for inst in &bytecode.instructions {
        match inst.opcode {
            OpCode::LoadConst => {
                let offset = imm16(inst) as usize;
                assert!(
                    offset + 8 <= bytecode.constant_pool.data.len(),
                    "LoadConst reads outside constant pool"
                );
            }
            OpCode::UDiv | OpCode::IDiv | OpCode::URem | OpCode::IRem => {
                panic!("division/rem is intentionally absent from Orca benchmark");
            }
            OpCode::PrintStr | OpCode::PrintInt | OpCode::PrintUInt | OpCode::PrintFloat => {
                panic!("I/O is intentionally absent from runtime benchmark");
            }
            _ => {}
        }
    }

    assert_eq!(
        bytecode.instructions.len(),
        12,
        "unexpected bench program shape"
    );
}

pub fn expected_sum(iterations: u64) -> u64 {
    iterations.saturating_sub(1).wrapping_mul(iterations) / 2
}

fn inst(opcode: OpCode, a: u8, b: u8, c: u8) -> Instruction {
    Instruction { opcode, a, b, c }
}

fn imm16(inst: &Instruction) -> u16 {
    ((inst.c as u16) << 8) | inst.b as u16
}

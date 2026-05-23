use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use w16_core::bytecode::{Bytecode, ConstantPool, Instruction, OpCode};
use w16_core::interpreter::vm::VM;

/// Создает программу-счетчик: декремент r0 в цикле 1 000 000 раз.
/// Это лучший способ измерить "чистый" MIPS.
fn create_loop_program(iterations: u64) -> Bytecode {
    let mut instructions = Vec::new();

    // r0 = iterations (загрузка из ConstantPool по индексу 0)
    instructions.push(Instruction {
        opcode: OpCode::LoadConst,
        a: 0,
        b: 0,
        c: 0,
    });

    // r1 = 1 (шаг декремента)
    instructions.push(Instruction {
        opcode: OpCode::Load8,
        a: 1,
        b: 0,
        c: 1,
    });

    let loop_start = 2; // IP, где начинается вычитание

    // r0 = r0 - r1
    instructions.push(Instruction {
        opcode: OpCode::Sub,
        a: 0,
        b: 0,
        c: 1,
    });

    // Подготовка адреса для прыжка: r2 = loop_start (2)
    instructions.push(Instruction {
        opcode: OpCode::Load8,
        a: 2,
        b: 0,
        c: loop_start as u8,
    });

    // Прыгаем на адрес из r2, если r0 != 0
    instructions.push(Instruction {
        opcode: OpCode::Jnz,
        a: 0,
        b: 2,
        c: 0,
    });

    // Стоп
    instructions.push(Instruction {
        opcode: OpCode::Halt,
        a: 0,
        b: 0,
        c: 0,
    });

    let mut cp = ConstantPool::new();
    // Добавляем 8-байтную константу в пул
    let bytes = iterations.to_le_bytes();
    cp.data.extend_from_slice(&bytes);

    Bytecode::new(instructions, cp)
}

fn bench_vm_speed(c: &mut Criterion) {
    let mut group = c.benchmark_group("W16_Interpreter");
    let iters = 1_000_000;
    let bytecode = create_loop_program(iters);

    // Важно: инициализируем VM вне итерации бенчмарка,
    // чтобы не мерить скорость аллокации Vec<u8>
    let mut vm = VM::new(1024 * 1024);

    group.bench_function("tight_loop_1m_iters", |b| {
        b.iter(|| {
            let _ = vm.run(black_box(&bytecode));
        });
    });

    group.finish();
}

criterion_group!(benches, bench_vm_speed);
criterion_main!(benches);

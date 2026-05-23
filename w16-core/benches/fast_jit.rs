use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use w16_core::jit::jit_compiler::JIT;
use w16_core::{Bytecode, ConstantPool, Instruction, OpCode}; // Используем нашу новую структуру

/// Программа-счетчик для теста
fn create_bench_program(iters: u64) -> Bytecode {
    let mut cp = ConstantPool::new();
    cp.data.extend_from_slice(&iters.to_le_bytes());

    let prog = vec![
        Instruction {
            opcode: OpCode::Load16,
            a: 0,
            b: 0,
            c: 0,
        }, // R0 = 0 (i)
        Instruction {
            opcode: OpCode::Load16,
            a: 1,
            b: 1,
            c: 0,
        }, // R1 = 1 (step)
        Instruction {
            opcode: OpCode::LoadConst,
            a: 2,
            b: 0,
            c: 0,
        }, // R2 = iters
        Instruction {
            opcode: OpCode::Load16,
            a: 3,
            b: 0,
            c: 0,
        }, // R3 = 0 (sum)
        // [4] Начало цикла
        Instruction {
            opcode: OpCode::IULt,
            a: 4,
            b: 0,
            c: 2,
        }, // R4 = i < limit
        Instruction {
            opcode: OpCode::Load16,
            a: 5,
            b: 11,
            c: 0,
        }, // R5 = halt ip
        Instruction {
            opcode: OpCode::Jz,
            a: 4,
            b: 5,
            c: 0,
        }, // If !R4 jump to Halt (11)
        Instruction {
            opcode: OpCode::Add,
            a: 3,
            b: 3,
            c: 0,
        }, // sum += i
        Instruction {
            opcode: OpCode::Add,
            a: 0,
            b: 0,
            c: 1,
        }, // i++
        Instruction {
            opcode: OpCode::Load16,
            a: 6,
            b: 4,
            c: 0,
        }, // R6 = 4 (loop start)
        Instruction {
            opcode: OpCode::Jmp,
            a: 6,
            b: 0,
            c: 0,
        }, // Jump to [4]
        Instruction {
            opcode: OpCode::Halt,
            a: 0,
            b: 0,
            c: 0,
        }, // [11] Halt
    ];

    Bytecode::new(prog, cp)
}

fn bench_jit_only(c: &mut Criterion) {
    let mut group = c.benchmark_group("W16_JIT");
    let iters = 10_000_000;
    let bytecode = create_bench_program(iters);

    // Тип функции, которую генерирует наш JIT
    type JitFn = unsafe extern "C" fn(*mut u64, *mut u8, usize, *const u8, usize);

    // 1. JIT Cold: Время на инициализацию Cranelift, компиляцию и выполнение
    // Здесь мы увидим цену "умной" оптимизации
    group.bench_function(BenchmarkId::new("JIT_Cold_Total", iters), |b| {
        b.iter(|| {
            let mut jit = JIT::new();
            let code_ptr = jit.compile(black_box(&bytecode));
            let f: JitFn = unsafe { std::mem::transmute(code_ptr) };

            let mut regs = [0u64; 256];
            let mut memory = [0u8; 0];
            unsafe {
                f(
                    regs.as_mut_ptr(),
                    memory.as_mut_ptr(),
                    memory.len(),
                    bytecode.constant_pool.data.as_ptr(),
                    bytecode.constant_pool.data.len(),
                );
            }
            black_box(regs);
        });
    });

    // 2. JIT Hot: Только выполнение уже оптимизированного маш-кода
    let mut jit = JIT::new();
    let code_ptr = jit.compile(&bytecode);
    let jit_fn: JitFn = unsafe { std::mem::transmute(code_ptr) };
    let const_ptr = bytecode.constant_pool.data.as_ptr();
    let const_len = bytecode.constant_pool.data.len();

    group.bench_function(BenchmarkId::new("JIT_Hot_Execution", iters), |b| {
        let mut regs = [0u64; 256];
        let mut memory = [0u8; 0];
        b.iter(|| {
            unsafe {
                jit_fn(
                    black_box(regs.as_mut_ptr()),
                    black_box(memory.as_mut_ptr()),
                    black_box(memory.len()),
                    black_box(const_ptr),
                    black_box(const_len),
                );
            }
            black_box(regs);
        });
    });

    group.finish();
}

criterion_group!(benches, bench_jit_only);
criterion_main!(benches);

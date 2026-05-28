mod common;

use common::{BENCH_ITERS, REGISTER_COUNT, build_sum_loop_program, expected_sum};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use w16_core::jit::jit_compiler::JIT;

type JitFn = unsafe extern "C" fn(*mut u64, *mut u8, usize, *const u8, usize);

fn bench_jit(c: &mut Criterion) {
    let mut group = c.benchmark_group("W16_JIT");
    let bytecode = build_sum_loop_program(BENCH_ITERS);

    group.bench_function(BenchmarkId::new("cold_compile_and_run", BENCH_ITERS), |b| {
        b.iter(|| {
            let mut jit = JIT::new();
            let code_ptr = jit.compile(black_box(&bytecode));
            let jit_fn: JitFn = unsafe { std::mem::transmute(code_ptr) };

            let mut regs = [0u64; REGISTER_COUNT];
            let mut memory = [];

            unsafe {
                jit_fn(
                    regs.as_mut_ptr(),
                    memory.as_mut_ptr(),
                    memory.len(),
                    bytecode.constant_pool.data.as_ptr(),
                    bytecode.constant_pool.data.len(),
                );
            }

            assert_eq!(black_box(regs[3]), expected_sum(BENCH_ITERS));
        });
    });

    let mut jit = JIT::new();
    let code_ptr = jit.compile(&bytecode);
    let jit_fn: JitFn = unsafe { std::mem::transmute(code_ptr) };
    let const_ptr = bytecode.constant_pool.data.as_ptr();
    let const_len = bytecode.constant_pool.data.len();

    group.bench_function(BenchmarkId::new("hot_execution", BENCH_ITERS), |b| {
        let mut regs = [0u64; REGISTER_COUNT];
        let mut memory = [];

        b.iter(|| {
            regs = [0; REGISTER_COUNT];

            unsafe {
                jit_fn(
                    black_box(regs.as_mut_ptr()),
                    black_box(memory.as_mut_ptr()),
                    black_box(memory.len()),
                    black_box(const_ptr),
                    black_box(const_len),
                );
            }

            assert_eq!(black_box(regs[3]), expected_sum(BENCH_ITERS));
        });
    });

    group.finish();
}

criterion_group!(benches, bench_jit);
criterion_main!(benches);

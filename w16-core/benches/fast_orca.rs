mod common;

use common::{
    BENCH_ITERS, BENCH_MEMORY_SIZE, build_sum_loop_program, expected_sum, verify_orca_bench_program,
};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use w16_core::interpreter::evms::OrcaEvm;

fn bench_orca(c: &mut Criterion) {
    let mut group = c.benchmark_group("W16_Orca_EVM");
    let bytecode = build_sum_loop_program(BENCH_ITERS);
    verify_orca_bench_program(&bytecode);

    let mut orca = OrcaEvm::with_call_capacity(BENCH_MEMORY_SIZE, 8);

    group.bench_function(BenchmarkId::new("sum_loop_unchecked", BENCH_ITERS), |b| {
        b.iter(|| {
            orca.reset_registers();
            unsafe {
                orca.run_unchecked(black_box(&bytecode));
            }
            assert_eq!(black_box(orca.registers[3]), expected_sum(BENCH_ITERS));
        });
    });

    group.finish();
}

criterion_group!(benches, bench_orca);
criterion_main!(benches);

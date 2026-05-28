mod common;

use common::{BENCH_ITERS, BENCH_MEMORY_SIZE, build_sum_loop_program, expected_sum};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use w16_core::interpreter::vm::VM;

fn bench_vm(c: &mut Criterion) {
    let mut group = c.benchmark_group("W16_VM");
    let bytecode = build_sum_loop_program(BENCH_ITERS);
    let mut vm = VM::new(BENCH_MEMORY_SIZE);

    group.bench_function(BenchmarkId::new("sum_loop", BENCH_ITERS), |b| {
        b.iter(|| {
            vm.registers = [0; 256];
            vm.run(black_box(&bytecode)).expect("VM benchmark must run");
            assert_eq!(black_box(vm.registers[3]), expected_sum(BENCH_ITERS));
        });
    });

    group.finish();
}

criterion_group!(benches, bench_vm);
criterion_main!(benches);

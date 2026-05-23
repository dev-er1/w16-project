// w16-core/tests/vm_registers.rs
//
//! Тесты регистрового файла: все 256 регистров, изоляция, Mov, Load8/16/Const.
use crate::{
    vm_tests::helpers::{Program, run},
    *,
};
use w16_core::bytecode::OpCode;

// =============================================================================
// БАЗОВЫЕ ОПЕРАЦИИ С РЕГИСТРАМИ
// =============================================================================

#[test]
fn r0_default_zero() {
    let bc = Program::new().halt();
    assert_reg!(run(&bc), 0, 0);
}

#[test]
fn all_registers_default_zero() {
    let bc = Program::new().halt();
    let regs = run(&bc);
    for i in 0..=255usize {
        assert_eq!(regs[i], 0, "r{i} should be 0 initially");
    }
}

#[test]
fn write_r0() {
    let bc = Program::new().load8(0, 42).halt();
    assert_reg!(run(&bc), 0, 42);
}

#[test]
fn write_r1() {
    let bc = Program::new().load8(1, 42).halt();
    assert_reg!(run(&bc), 1, 42);
}

#[test]
fn write_r127() {
    let bc = Program::new().load8(127, 99).halt();
    assert_reg!(run(&bc), 127, 99);
}

#[test]
fn write_r128() {
    let bc = Program::new().load8(128, 77).halt();
    assert_reg!(run(&bc), 128, 77);
}

#[test]
fn write_r255() {
    let bc = Program::new().load8(255, 42).halt();
    let regs = run(&bc);
    assert_eq!(regs[255], 42);
}

#[test]
fn write_r254_and_r255() {
    let bc = Program::new().load8(254, 10).load8(255, 20).halt();
    let regs = run(&bc);
    assert_eq!(regs[254], 10);
    assert_eq!(regs[255], 20);
}

// =============================================================================
// ИЗОЛЯЦИЯ РЕГИСТРОВ
// =============================================================================

#[test]
fn write_r1_does_not_affect_r0() {
    let bc = Program::new().load8(1, 99).halt();
    assert_reg!(run(&bc), 0, 0);
}

#[test]
fn write_r0_does_not_affect_r1() {
    let bc = Program::new().load8(0, 99).halt();
    assert_reg!(run(&bc), 1, 0);
}

#[test]
fn adjacent_registers_independent() {
    let bc = Program::new().load8(5, 11).load8(6, 22).load8(7, 33).halt();
    let regs = run(&bc);
    assert_reg!(regs, 5, 11);
    assert_reg!(regs, 6, 22);
    assert_reg!(regs, 7, 33);
    assert_reg!(regs, 4, 0);
    assert_reg!(regs, 8, 0);
}

#[test]
fn arithmetic_only_writes_dest() {
    // Add пишет только в r0, r1 и r2 не меняются
    let bc = Program::new()
        .load8(1, 10)
        .load8(2, 20)
        .inst(OpCode::Add, 0, 1, 2)
        .halt();
    let regs = run(&bc);
    assert_reg!(regs, 0, 30);
    assert_reg!(regs, 1, 10); // не изменился
    assert_reg!(regs, 2, 20); // не изменился
}

#[test]
fn ten_independent_results() {
    let bc = Program::new()
        .load8(10, 1)
        .load8(11, 2)
        .load8(12, 3)
        .load8(13, 4)
        .load8(14, 5)
        .inst(OpCode::Add, 20, 10, 11)
        .inst(OpCode::Add, 21, 11, 12)
        .inst(OpCode::Add, 22, 12, 13)
        .inst(OpCode::Add, 23, 13, 14)
        .inst(OpCode::Mul, 24, 10, 14)
        .halt();
    let regs = run(&bc);
    assert_reg!(regs, 20, 3);
    assert_reg!(regs, 21, 5);
    assert_reg!(regs, 22, 7);
    assert_reg!(regs, 23, 9);
    assert_reg!(regs, 24, 5);
}

// =============================================================================
// MOV
// =============================================================================

#[test]
fn mov_basic() {
    let bc = Program::new()
        .load8(1, 42)
        .inst(OpCode::Mov, 0, 1, 0)
        .halt();
    assert_reg!(run(&bc), 0, 42);
}

#[test]
fn mov_r0_to_r255() {
    let bc = Program::new()
        .load8(0, 77)
        .inst(OpCode::Mov, 255, 0, 0)
        .halt();
    let regs = run(&bc);
    assert_eq!(regs[255], 77);
}

#[test]
fn mov_r255_to_r0() {
    let bc = Program::new()
        .load8(255, 55)
        .inst(OpCode::Mov, 0, 255, 0)
        .halt();
    assert_reg!(run(&bc), 0, 55);
}

#[test]
fn mov_does_not_clear_source() {
    let bc = Program::new()
        .load8(1, 42)
        .inst(OpCode::Mov, 0, 1, 0)
        .halt();
    let regs = run(&bc);
    assert_reg!(regs, 0, 42);
    assert_reg!(regs, 1, 42); // источник не изменился
}

#[test]
fn mov_self() {
    // Mov r1, r1 — копирование в себя, результат не меняется
    let bc = Program::new()
        .load8(1, 42)
        .inst(OpCode::Mov, 1, 1, 0)
        .halt();
    assert_reg!(run(&bc), 1, 42);
}

#[test]
fn mov_chain() {
    // r1 -> r2 -> r3 -> r4 -> r5
    let bc = Program::new()
        .load8(1, 99)
        .inst(OpCode::Mov, 2, 1, 0)
        .inst(OpCode::Mov, 3, 2, 0)
        .inst(OpCode::Mov, 4, 3, 0)
        .inst(OpCode::Mov, 5, 4, 0)
        .halt();
    let regs = run(&bc);
    for i in 1..=5usize {
        assert_eq!(regs[i], 99, "r{i} should be 99");
    }
}

#[test]
fn mov_overwrites_destination() {
    let bc = Program::new()
        .load8(0, 42)
        .load8(1, 99)
        .inst(OpCode::Mov, 0, 1, 0)
        .halt();
    assert_reg!(run(&bc), 0, 99);
}

#[test]
fn swap_via_temp() {
    // swap(r1, r2) используя r3 как temp
    let bc = Program::new()
        .load8(1, 10)
        .load8(2, 20)
        .inst(OpCode::Mov, 3, 1, 0) // temp = r1
        .inst(OpCode::Mov, 1, 2, 0) // r1 = r2
        .inst(OpCode::Mov, 2, 3, 0) // r2 = temp
        .halt();
    let regs = run(&bc);
    assert_reg!(regs, 1, 20);
    assert_reg!(regs, 2, 10);
}

// =============================================================================
// LOAD8
// =============================================================================

#[test]
fn load8_min() {
    let bc = Program::new().load8(0, 0).halt();
    assert_reg!(run(&bc), 0, 0);
}

#[test]
fn load8_max() {
    let bc = Program::new().load8(0, 255).halt();
    assert_reg!(run(&bc), 0, 255);
}

#[test]
fn load8_mid() {
    let bc = Program::new().load8(0, 128).halt();
    assert_reg!(run(&bc), 0, 128);
}

#[test]
fn load8_zero_extends_to_u64() {
    // Load8 zero-extends: r0 = 0x00000000000000FF
    let bc = Program::new().load8(0, 255).halt();
    let regs = run(&bc);
    assert_eq!(regs[0], 255u64); // верхние биты = 0
}

#[test]
fn load8_multiple_registers() {
    let bc = Program::new()
        .load8(0, 1)
        .load8(1, 2)
        .load8(2, 3)
        .load8(3, 4)
        .load8(4, 5)
        .halt();
    let regs = run(&bc);
    for i in 0..5usize {
        assert_eq!(regs[i], (i + 1) as u64);
    }
}

// =============================================================================
// LOAD16
// =============================================================================

#[test]
fn load16_basic() {
    let bc = Program::new().load16(0, 1000).halt();
    assert_reg!(run(&bc), 0, 1000);
}

#[test]
fn load16_max() {
    let bc = Program::new().load16(0, 0xFFFF).halt();
    assert_reg!(run(&bc), 0, 0xFFFF);
}

#[test]
fn load16_crosses_byte_boundary() {
    // 0x0100 = 256, lo=0x00, hi=0x01
    let bc = Program::new().load16(0, 256).halt();
    assert_reg!(run(&bc), 0, 256);
}

#[test]
fn load16_zero_extends() {
    let bc = Program::new().load16(0, 0xFFFF).halt();
    let regs = run(&bc);
    assert_eq!(regs[0], 0xFFFFu64); // верхние 48 бит = 0
}

// =============================================================================
// LOADCONST
// =============================================================================

#[test]
fn load_const_u64_max() {
    let (p, off) = Program::new().add_u64(u64::MAX);
    let bc = p.load_const(0, off).halt();
    assert_reg!(run(&bc), 0, u64::MAX);
}

#[test]
fn load_const_into_r255() {
    let (p, off) = Program::new().add_u64(0xDEADBEEF);
    let bc = p.load_const(255, off).halt();
    let regs = run(&bc);
    assert_eq!(regs[255], 0xDEADBEEF);
}

#[test]
fn load_const_f64() {
    let (p, off) = Program::new().add_f64(std::f64::consts::PI);
    let bc = p.load_const(0, off).halt();
    let regs = run(&bc);
    let got = f64::from_bits(regs[0]);
    assert!((got - std::f64::consts::PI).abs() < 1e-15);
}

#[test]
fn load_const_zero() {
    let (p, off) = Program::new().add_u64(0);
    let bc = p.load_const(0, off).halt();
    assert_reg!(run(&bc), 0, 0);
}

#[test]
fn load_const_sequential_offsets() {
    // Три константы подряд в пуле
    let (p, o1) = Program::new().add_u64(111);
    let (p, o2) = p.add_u64(222);
    let (p, o3) = p.add_u64(333);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .load_const(3, o3)
        .halt();
    let regs = run(&bc);
    assert_reg!(regs, 1, 111);
    assert_reg!(regs, 2, 222);
    assert_reg!(regs, 3, 333);
}

// =============================================================================
// РЕГИСТРЫ КАК ОПЕРАНДЫ ДЛЯ АДРЕСОВ И УСЛОВИЙ
// =============================================================================

#[test]
fn register_as_jump_target() {
    // Адрес перехода хранится в регистре
    let bc = Program::new()
        .load8(1, 3) // ip=3 -> Halt
        .inst(OpCode::Jmp, 1, 0, 0) // прыгаем на ip=3
        .load8(0, 99) // не выполняется (ip=2)
        .inst(OpCode::Halt, 0, 0, 0) // ip=3
        .build();
    assert_reg!(run(&bc), 0, 0);
}

#[test]
fn register_as_memory_address() {
    // Адрес памяти хранится в регистре
    let bc = Program::new()
        .load8(1, 50) // addr = 50
        .load8(2, 123) // val = 123
        .inst(OpCode::St8, 1, 2, 0)
        .inst(OpCode::Ld8, 0, 1, 0)
        .halt();
    assert_reg!(run(&bc), 0, 123);
}

#[test]
fn register_as_condition() {
    // Условие перехода хранится в регистре
    let bc = Program::new()
        .load8(1, 5)
        .load8(2, 5)
        .inst(OpCode::IEq, 3, 1, 2) // r3 = (5==5) = 1
        .load8(4, 6) // halt_ip = 6
        .inst(OpCode::Jnz, 3, 4, 0) // если r3 != 0, прыгаем
        .load8(0, 99) // Не выполняется
        .inst(OpCode::Halt, 0, 0, 0)
        .build();
    assert_reg!(run(&bc), 0, 0);
}

// =============================================================================
// МНОГО РЕГИСТРОВ ОДНОВРЕМЕННО
// =============================================================================

#[test]
fn fill_all_low_registers() {
    let mut p = Program::new();
    for i in 0u8..=50 {
        p = p.load8(i, i);
    }
    let bc = p.halt();
    let regs = run(&bc);
    for i in 0..=50usize {
        assert_eq!(regs[i], i as u64, "r{i}");
    }
}

#[test]
fn high_registers_untouched_by_low_ops() {
    // Операции с r0..r10 не должны влиять на r200..r255
    let bc = Program::new()
        .load8(200, 42)
        .load8(255, 99)
        .load8(0, 1)
        .load8(1, 2)
        .load8(2, 3)
        .inst(OpCode::Add, 3, 0, 1)
        .inst(OpCode::Mul, 4, 2, 3)
        .halt();
    let regs = run(&bc);
    assert_eq!(regs[200], 42);
    assert_eq!(regs[255], 99);
}

#[test]
fn accumulate_in_r0_from_all_others() {
    // Суммируем значения из r1..r5 в r0
    let bc = Program::new()
        .load8(1, 10)
        .load8(2, 20)
        .load8(3, 30)
        .load8(4, 40)
        .load8(5, 50)
        .inst(OpCode::Add, 0, 1, 2)
        .inst(OpCode::Add, 0, 0, 3)
        .inst(OpCode::Add, 0, 0, 4)
        .inst(OpCode::Add, 0, 0, 5)
        .halt();
    assert_reg!(run(&bc), 0, 150);
}

// =============================================================================
// ГРАНИЧНЫЕ СЛУЧАИ
// =============================================================================

#[test]
fn overwrite_same_register_multiple_times() {
    let bc = Program::new()
        .load8(0, 1)
        .load8(0, 2)
        .load8(0, 3)
        .load8(0, 42)
        .halt();
    assert_reg!(run(&bc), 0, 42);
}

#[test]
fn register_value_persists_after_other_ops() {
    let bc = Program::new()
        .load8(5, 77)
        .load8(1, 10)
        .load8(2, 20)
        .inst(OpCode::Add, 3, 1, 2)
        .inst(OpCode::Mul, 4, 3, 1)
        .halt();
    assert_reg!(run(&bc), 5, 77); // r5 не трогался
}

#[test]
fn u64_max_in_register() {
    let (p, off) = Program::new().add_u64(u64::MAX);
    let bc = p.load_const(1, off).halt();
    assert_reg!(run(&bc), 1, u64::MAX);
}

#[test]
fn arithmetic_with_r255() {
    let bc = Program::new()
        .load8(254, 20)
        .load8(255, 22)
        .inst(OpCode::Add, 253, 254, 255)
        .halt();
    let regs = run(&bc);
    assert_eq!(regs[253], 42);
}

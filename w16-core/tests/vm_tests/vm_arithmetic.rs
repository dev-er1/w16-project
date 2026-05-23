// w16-core\tests\vm_tests\vm_arithmetic.rs
//
//! Тесты арифметических инструкций VM.
//! Покрывает: Add, Sub, Mul, UDiv, IDiv, URem, IRem, Neg, FNeg, FAbs,
//! FAdd, FSub, FMul, FDiv, FRem, I2F, F2I, U2F, F2U, а также граничные случаи.

use crate::*;
use vm_tests::helpers::*;
use w16_core::{VMError, bytecode::OpCode};

// =============================================================================
// ADD — целочисленное сложение (wrapping)
// =============================================================================

#[test]
fn add_basic() {
    let bc = Program::new()
        .load8(1, 10)
        .load8(2, 32)
        .inst(OpCode::Add, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 42);
}

#[test]
fn add_zero_left() {
    let bc = Program::new()
        .load8(1, 0)
        .load8(2, 99)
        .inst(OpCode::Add, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 99);
}

#[test]
fn add_zero_right() {
    let bc = Program::new()
        .load8(1, 77)
        .load8(2, 0)
        .inst(OpCode::Add, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 77);
}

#[test]
fn add_both_zero() {
    let bc = Program::new()
        .load8(1, 0)
        .load8(2, 0)
        .inst(OpCode::Add, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 0);
}

#[test]
fn add_wrapping_overflow() {
    // u64::MAX + 1 = 0 (wrapping)
    let (p, off) = Program::new().add_u64(u64::MAX);
    let bc = p
        .load_const(1, off)
        .load8(2, 1)
        .inst(OpCode::Add, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 0);
}

#[test]
fn add_wrapping_large() {
    let (p, off) = Program::new().add_u64(u64::MAX - 5);
    let bc = p
        .load_const(1, off)
        .load8(2, 10)
        .inst(OpCode::Add, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 4); // wrapping
}

#[test]
fn add_same_register_src() {
    // r1 + r1 = 2 * r1
    let bc = Program::new()
        .load8(1, 21)
        .inst(OpCode::Add, 0, 1, 1)
        .halt();
    assert_reg!(run(&bc), 0, 42);
}

#[test]
fn add_result_to_self() {
    // r0 = r0 + r1, где r0 начинается как 0
    let bc = Program::new()
        .load8(0, 5)
        .load8(1, 7)
        .inst(OpCode::Add, 0, 0, 1)
        .halt();
    assert_reg!(run(&bc), 0, 12);
}

#[test]
fn add_chain() {
    // 1 + 2 + 3 + 4 = 10
    let bc = Program::new()
        .load8(1, 1)
        .load8(2, 2)
        .load8(3, 3)
        .load8(4, 4)
        .inst(OpCode::Add, 0, 1, 2)
        .inst(OpCode::Add, 0, 0, 3)
        .inst(OpCode::Add, 0, 0, 4)
        .halt();
    assert_reg!(run(&bc), 0, 10);
}

#[test]
fn add_max_plus_max() {
    let (p, o1) = Program::new().add_u64(u64::MAX);
    let (p, o2) = p.add_u64(u64::MAX);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::Add, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, u64::MAX.wrapping_add(u64::MAX));
}

// =============================================================================
// SUB — вычитание (wrapping)
// =============================================================================

#[test]
fn sub_basic() {
    let bc = Program::new()
        .load8(1, 50)
        .load8(2, 8)
        .inst(OpCode::Sub, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 42);
}

#[test]
fn sub_to_zero() {
    let bc = Program::new()
        .load8(1, 42)
        .load8(2, 42)
        .inst(OpCode::Sub, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 0);
}

#[test]
fn sub_wrapping_underflow() {
    // 0 - 1 = u64::MAX (wrapping)
    let bc = Program::new()
        .load8(1, 0)
        .load8(2, 1)
        .inst(OpCode::Sub, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, u64::MAX);
}

#[test]
fn sub_from_max() {
    let (p, off) = Program::new().add_u64(u64::MAX);
    let bc = p
        .load_const(1, off)
        .load8(2, 1)
        .inst(OpCode::Sub, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, u64::MAX - 1);
}

#[test]
fn sub_zero_right() {
    let bc = Program::new()
        .load8(1, 100)
        .load8(2, 0)
        .inst(OpCode::Sub, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 100);
}

#[test]
fn sub_same_registers() {
    let bc = Program::new()
        .load8(1, 77)
        .inst(OpCode::Sub, 0, 1, 1)
        .halt();
    assert_reg!(run(&bc), 0, 0);
}

// =============================================================================
// MUL — умножение (wrapping)
// =============================================================================

#[test]
fn mul_basic() {
    let bc = Program::new()
        .load8(1, 6)
        .load8(2, 7)
        .inst(OpCode::Mul, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 42);
}

#[test]
fn mul_by_zero() {
    let (p, off) = Program::new().add_u64(u64::MAX);
    let bc = p
        .load_const(1, off)
        .load8(2, 0)
        .inst(OpCode::Mul, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 0);
}

#[test]
fn mul_by_one() {
    let bc = Program::new()
        .load8(1, 42)
        .load8(2, 1)
        .inst(OpCode::Mul, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 42);
}

#[test]
fn mul_wrapping() {
    let (p, o1) = Program::new().add_u64(u64::MAX);
    let bc = p
        .load_const(1, o1)
        .load8(2, 2)
        .inst(OpCode::Mul, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, u64::MAX.wrapping_mul(2));
}

#[test]
fn mul_large() {
    let (p, o1) = Program::new().add_u64(1_000_000);
    let (p, o2) = p.add_u64(1_000_000);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::Mul, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 1_000_000_000_000u64);
}

#[test]
fn mul_power_of_two() {
    // 2^10 = 1024
    let bc = Program::new().load8(1, 1).load8(2, 2);
    let mut p = bc;
    for _ in 0..10 {
        p = p.inst(OpCode::Mul, 1, 1, 2);
    }
    let bc = p.inst(OpCode::Mov, 0, 1, 0).halt();
    assert_reg!(run(&bc), 0, 1024);
}

#[test]
fn mul_commutative() {
    let bc = Program::new()
        .load8(1, 7)
        .load8(2, 13)
        .inst(OpCode::Mul, 0, 1, 2)
        .inst(OpCode::Mul, 3, 2, 1)
        .halt();
    let regs = run(&bc);
    assert_eq!(regs[0], regs[3]);
}

// =============================================================================
// UDIV — беззнаковое деление
// =============================================================================

#[test]
fn udiv_basic() {
    let bc = Program::new()
        .load8(1, 84)
        .load8(2, 2)
        .inst(OpCode::UDiv, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 42);
}

#[test]
fn udiv_by_one() {
    let (p, off) = Program::new().add_u64(123456789);
    let bc = p
        .load_const(1, off)
        .load8(2, 1)
        .inst(OpCode::UDiv, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 123456789);
}

#[test]
fn udiv_larger_by_smaller() {
    let bc = Program::new()
        .load8(1, 100)
        .load8(2, 7)
        .inst(OpCode::UDiv, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 14); // floor(100/7)
}

#[test]
fn udiv_smaller_by_larger() {
    let bc = Program::new()
        .load8(1, 5)
        .load8(2, 10)
        .inst(OpCode::UDiv, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 0);
}

#[test]
fn udiv_zero_numerator() {
    let bc = Program::new()
        .load8(1, 0)
        .load8(2, 42)
        .inst(OpCode::UDiv, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 0);
}

#[test]
fn udiv_by_zero_returns_error() {
    let bc = Program::new()
        .load8(1, 10)
        .load8(2, 0)
        .inst(OpCode::UDiv, 0, 1, 2)
        .halt();
    let err = run_err(&bc);
    assert_vm_error!(err, VMError::DivisionByZero);
}

#[test]
fn udiv_large_values() {
    let (p, o1) = Program::new().add_u64(u64::MAX);
    let bc = p
        .load_const(1, o1)
        .load8(2, 2)
        .inst(OpCode::UDiv, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, u64::MAX / 2);
}

// =============================================================================
// IDIV — знаковое деление
// =============================================================================

#[test]
fn idiv_positive() {
    let bc = Program::new()
        .load8(1, 42)
        .load8(2, 6)
        .inst(OpCode::IDiv, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 7u64);
}

#[test]
fn idiv_negative_dividend() {
    // -42 / 6 = -7
    let (p, o1) = Program::new().add_u64((-42i64) as u64);
    let bc = p
        .load_const(1, o1)
        .load8(2, 6)
        .inst(OpCode::IDiv, 0, 1, 2)
        .halt();
    let regs = run(&bc);
    assert_eq!(regs[0] as i64, -7);
}

#[test]
fn idiv_negative_divisor() {
    // 42 / -6 = -7
    let (p, o2) = Program::new().add_u64((-6i64) as u64);
    let bc = p
        .load8(1, 42)
        .load_const(2, o2)
        .inst(OpCode::IDiv, 0, 1, 2)
        .halt();
    let regs = run(&bc);
    assert_eq!(regs[0] as i64, -7);
}

#[test]
fn idiv_both_negative() {
    // -42 / -6 = 7
    let (p, o1) = Program::new().add_u64((-42i64) as u64);
    let (p, o2) = p.add_u64((-6i64) as u64);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::IDiv, 0, 1, 2)
        .halt();
    let regs = run(&bc);
    assert_eq!(regs[0] as i64, 7);
}

#[test]
fn idiv_by_zero_returns_error() {
    let bc = Program::new()
        .load8(1, 42)
        .load8(2, 0)
        .inst(OpCode::IDiv, 0, 1, 2)
        .halt();
    let err = run_err(&bc);
    assert_vm_error!(err, VMError::DivisionByZero);
}

#[test]
fn idiv_min_by_minus_one_safe() {
    // i64::MIN / -1 — overflow, VM должна вернуть 0 или не паниковать
    let (p, o1) = Program::new().add_u64(i64::MIN as u64);
    let (p, o2) = p.add_u64((-1i64) as u64);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::IDiv, 0, 1, 2)
        .halt();
    // Не должно паниковать
    let _ = run(&bc);
}

// =============================================================================
// UREM / IREM — остаток от деления
// =============================================================================

#[test]
fn urem_basic() {
    let bc = Program::new()
        .load8(1, 17)
        .load8(2, 5)
        .inst(OpCode::URem, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 2);
}

#[test]
fn urem_zero_remainder() {
    let bc = Program::new()
        .load8(1, 42)
        .load8(2, 7)
        .inst(OpCode::URem, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 0);
}

#[test]
fn urem_by_zero_returns_error() {
    let bc = Program::new()
        .load8(1, 17)
        .load8(2, 0)
        .inst(OpCode::URem, 0, 1, 2)
        .halt();
    assert_vm_error!(run_err(&bc), VMError::DivisionByZero);
}

#[test]
fn urem_smaller_than_divisor() {
    let bc = Program::new()
        .load8(1, 3)
        .load8(2, 10)
        .inst(OpCode::URem, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 3);
}

#[test]
fn irem_negative() {
    // -17 % 5 = -2 (truncated toward zero)
    let (p, o1) = Program::new().add_u64((-17i64) as u64);
    let bc = p
        .load_const(1, o1)
        .load8(2, 5)
        .inst(OpCode::IRem, 0, 1, 2)
        .halt();
    let regs = run(&bc);
    assert_eq!(regs[0] as i64, -2);
}

#[test]
fn irem_by_zero_returns_error() {
    let bc = Program::new()
        .load8(1, 17)
        .load8(2, 0)
        .inst(OpCode::IRem, 0, 1, 2)
        .halt();
    assert_vm_error!(run_err(&bc), VMError::DivisionByZero);
}

// =============================================================================
// NEG — унарный минус (целое)
// =============================================================================

#[test]
fn neg_positive() {
    let bc = Program::new()
        .load8(1, 42)
        .inst(OpCode::Neg, 0, 1, 0)
        .halt();
    let regs = run(&bc);
    assert_eq!(regs[0] as i64, -42);
}

#[test]
fn neg_negative() {
    let (p, off) = Program::new().add_u64((-42i64) as u64);
    let bc = p.load_const(1, off).inst(OpCode::Neg, 0, 1, 0).halt();
    let regs = run(&bc);
    assert_eq!(regs[0] as i64, 42);
}

#[test]
fn neg_zero() {
    let bc = Program::new().load8(1, 0).inst(OpCode::Neg, 0, 1, 0).halt();
    assert_reg!(run(&bc), 0, 0);
}

#[test]
fn neg_min_i64_wrapping() {
    // -i64::MIN = i64::MIN (wrapping)
    let (p, off) = Program::new().add_u64(i64::MIN as u64);
    let bc = p.load_const(1, off).inst(OpCode::Neg, 0, 1, 0).halt();
    let regs = run(&bc);
    assert_eq!(regs[0] as i64, i64::MIN.wrapping_neg());
}

#[test]
fn neg_double_negation() {
    let bc = Program::new()
        .load8(1, 77)
        .inst(OpCode::Neg, 2, 1, 0)
        .inst(OpCode::Neg, 0, 2, 0)
        .halt();
    assert_reg!(run(&bc), 0, 77);
}

// =============================================================================
// FLOAT ARITHMETIC
// =============================================================================

#[test]
fn fadd_basic() {
    let (p, o1) = Program::new().add_f64(1.5);
    let (p, o2) = p.add_f64(2.5);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::FAdd, 0, 1, 2)
        .halt();
    assert_reg_f64!(run(&bc), 0, 4.0);
}

#[test]
fn fsub_basic() {
    let (p, o1) = Program::new().add_f64(10.0);
    let (p, o2) = p.add_f64(3.5);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::FSub, 0, 1, 2)
        .halt();
    assert_reg_f64!(run(&bc), 0, 6.5);
}

#[test]
fn fmul_basic() {
    let (p, o1) = Program::new().add_f64(3.0);
    let (p, o2) = p.add_f64(14.0);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::FMul, 0, 1, 2)
        .halt();
    assert_reg_f64!(run(&bc), 0, 42.0);
}

#[test]
fn fdiv_basic() {
    let (p, o1) = Program::new().add_f64(84.0);
    let (p, o2) = p.add_f64(2.0);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::FDiv, 0, 1, 2)
        .halt();
    assert_reg_f64!(run(&bc), 0, 42.0);
}

#[test]
fn fdiv_by_zero_gives_inf() {
    let (p, o1) = Program::new().add_f64(1.0);
    let (p, o2) = p.add_f64(0.0);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::FDiv, 0, 1, 2)
        .halt();
    let regs = run(&bc);
    assert!(f64::from_bits(regs[0]).is_infinite());
}

#[test]
fn frem_basic() {
    let (p, o1) = Program::new().add_f64(10.5);
    let (p, o2) = p.add_f64(3.0);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::FRem, 0, 1, 2)
        .halt();
    let regs = run(&bc);
    let got = f64::from_bits(regs[0]);
    assert!((got - 1.5).abs() < 1e-10);
}

#[test]
fn fadd_nan_propagates() {
    let (p, o1) = Program::new().add_f64(f64::NAN);
    let (p, o2) = p.add_f64(1.0);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::FAdd, 0, 1, 2)
        .halt();
    let regs = run(&bc);
    assert!(f64::from_bits(regs[0]).is_nan());
}

#[test]
fn fadd_inf() {
    let (p, o1) = Program::new().add_f64(f64::INFINITY);
    let (p, o2) = p.add_f64(1.0);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::FAdd, 0, 1, 2)
        .halt();
    let regs = run(&bc);
    assert_eq!(f64::from_bits(regs[0]), f64::INFINITY);
}

#[test]
fn fadd_inf_minus_inf_is_nan() {
    let (p, o1) = Program::new().add_f64(f64::INFINITY);
    let (p, o2) = p.add_f64(f64::NEG_INFINITY);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::FAdd, 0, 1, 2)
        .halt();
    let regs = run(&bc);
    assert!(f64::from_bits(regs[0]).is_nan());
}

#[test]
fn fneg_positive() {
    let (p, off) = Program::new().add_f64(42.0);
    let bc = p.load_const(1, off).inst(OpCode::FNeg, 0, 1, 0).halt();
    assert_reg_f64!(run(&bc), 0, -42.0);
}

#[test]
fn fneg_negative() {
    let (p, off) = Program::new().add_f64(-42.0);
    let bc = p.load_const(1, off).inst(OpCode::FNeg, 0, 1, 0).halt();
    assert_reg_f64!(run(&bc), 0, 42.0);
}

#[test]
fn fneg_nan_stays_nan() {
    let (p, off) = Program::new().add_f64(f64::NAN);
    let bc = p.load_const(1, off).inst(OpCode::FNeg, 0, 1, 0).halt();
    let regs = run(&bc);
    assert!(f64::from_bits(regs[0]).is_nan());
}

#[test]
fn fabs_negative() {
    let (p, off) = Program::new().add_f64(-42.0);
    let bc = p.load_const(1, off).inst(OpCode::FAbs, 0, 1, 0).halt();
    assert_reg_f64!(run(&bc), 0, 42.0);
}

#[test]
fn fabs_positive_unchanged() {
    let (p, off) = Program::new().add_f64(42.0);
    let bc = p.load_const(1, off).inst(OpCode::FAbs, 0, 1, 0).halt();
    assert_reg_f64!(run(&bc), 0, 42.0);
}

#[test]
fn fabs_neg_inf() {
    let (p, off) = Program::new().add_f64(f64::NEG_INFINITY);
    let bc = p.load_const(1, off).inst(OpCode::FAbs, 0, 1, 0).halt();
    let regs = run(&bc);
    assert_eq!(f64::from_bits(regs[0]), f64::INFINITY);
}

// =============================================================================
// CAST — приведение типов
// =============================================================================

#[test]
fn i2f_basic() {
    let bc = Program::new()
        .load8(1, 42)
        .inst(OpCode::I2F, 0, 1, 0)
        .halt();
    assert_reg_f64!(run(&bc), 0, 42.0);
}

#[test]
fn i2f_negative() {
    let (p, off) = Program::new().add_u64((-10i64) as u64);
    let bc = p.load_const(1, off).inst(OpCode::I2F, 0, 1, 0).halt();
    assert_reg_f64!(run(&bc), 0, -10.0);
}

#[test]
fn f2i_basic() {
    let (p, off) = Program::new().add_f64(42.7);
    let bc = p.load_const(1, off).inst(OpCode::F2I, 0, 1, 0).halt();
    let regs = run(&bc);
    assert_eq!(regs[0] as i64, 42); // truncate toward zero
}

#[test]
fn f2i_negative() {
    let (p, off) = Program::new().add_f64(-7.9);
    let bc = p.load_const(1, off).inst(OpCode::F2I, 0, 1, 0).halt();
    let regs = run(&bc);
    assert_eq!(regs[0] as i64, -7); // truncate toward zero
}

#[test]
fn u2f_large() {
    let (p, off) = Program::new().add_u64(u64::MAX);
    let bc = p.load_const(1, off).inst(OpCode::U2F, 0, 1, 0).halt();
    let regs = run(&bc);
    let f = f64::from_bits(regs[0]);
    // u64::MAX как f64 — приближённо
    assert!(f > 1e19);
}

#[test]
fn f2u_basic() {
    let (p, off) = Program::new().add_f64(42.9);
    let bc = p.load_const(1, off).inst(OpCode::F2U, 0, 1, 0).halt();
    assert_reg!(run(&bc), 0, 42u64);
}

#[test]
fn roundtrip_i2f_f2i() {
    let bc = Program::new()
        .load8(1, 99)
        .inst(OpCode::I2F, 2, 1, 0)
        .inst(OpCode::F2I, 0, 2, 0)
        .halt();
    assert_reg!(run(&bc), 0, 99);
}

// =============================================================================
// ГРАНИЧНЫЕ СЛУЧАИ И КОМБИНАЦИИ
// =============================================================================

#[test]
fn arithmetic_independence_of_registers() {
    // Убеждаемся что операции в разных регистрах не влияют друг на друга
    let bc = Program::new()
        .load8(1, 10)
        .load8(2, 20)
        .load8(3, 30)
        .inst(OpCode::Add, 4, 1, 2) // r4 = 30
        .inst(OpCode::Sub, 5, 3, 1) // r5 = 20
        .inst(OpCode::Mul, 6, 2, 3) // r6 = 600
        .halt();
    let regs = run(&bc);
    assert_reg!(regs, 4, 30);
    assert_reg!(regs, 5, 20);
    assert_reg!(regs, 6, 600);
}

#[test]
fn mul_add_expression() {
    // 3 * 4 + 2 * 5 = 12 + 10 = 22
    let bc = Program::new()
        .load8(1, 3)
        .load8(2, 4)
        .load8(3, 2)
        .load8(4, 5)
        .inst(OpCode::Mul, 5, 1, 2)
        .inst(OpCode::Mul, 6, 3, 4)
        .inst(OpCode::Add, 0, 5, 6)
        .halt();
    assert_reg!(run(&bc), 0, 22);
}

#[test]
fn quadratic_formula_like() {
    // a*x^2 + b*x + c где a=1, b=2, c=3, x=4
    // = 16 + 8 + 3 = 27
    let bc = Program::new()
        .load8(1, 1) // a
        .load8(2, 2) // b
        .load8(3, 3) // c
        .load8(4, 4) // x
        .inst(OpCode::Mul, 5, 4, 4) // x^2 = 16
        .inst(OpCode::Mul, 6, 1, 5) // a*x^2 = 16
        .inst(OpCode::Mul, 7, 2, 4) // b*x = 8
        .inst(OpCode::Add, 8, 6, 7) // a*x^2 + b*x = 24
        .inst(OpCode::Add, 0, 8, 3) // + c = 27
        .halt();
    assert_reg!(run(&bc), 0, 27);
}

#[test]
fn factorial_5_iterative() {
    // 5! = 120 через умножение
    let bc = Program::new()
        .load8(0, 1) // result = 1
        .load8(1, 2) // i = 2
        .load8(2, 5) // limit = 5
        .load8(3, 1) // step = 1
        // loop: result *= i, i += 1, if i <= 5 goto loop
        // Реализуем разворачиванием: 1*2*3*4*5
        .load8(10, 1)
        .load8(11, 2)
        .load8(12, 3)
        .load8(13, 4)
        .load8(14, 5)
        .inst(OpCode::Mul, 0, 10, 11)
        .inst(OpCode::Mul, 0, 0, 12)
        .inst(OpCode::Mul, 0, 0, 13)
        .inst(OpCode::Mul, 0, 0, 14)
        .halt();
    assert_reg!(run(&bc), 0, 120);
}

#[test]
fn sum_of_squares() {
    // 1^2 + 2^2 + 3^2 + 4^2 = 1 + 4 + 9 + 16 = 30
    let bc = Program::new()
        .load8(1, 1)
        .load8(2, 2)
        .load8(3, 3)
        .load8(4, 4)
        .inst(OpCode::Mul, 5, 1, 1)
        .inst(OpCode::Mul, 6, 2, 2)
        .inst(OpCode::Mul, 7, 3, 3)
        .inst(OpCode::Mul, 8, 4, 4)
        .inst(OpCode::Add, 0, 5, 6)
        .inst(OpCode::Add, 0, 0, 7)
        .inst(OpCode::Add, 0, 0, 8)
        .halt();
    assert_reg!(run(&bc), 0, 30);
}

#[test]
fn float_arithmetic_chain() {
    // (1.0 + 2.0) * (3.0 - 0.5) / 2.0 = 3.0 * 2.5 / 2.0 = 3.75
    let (p, o1) = Program::new().add_f64(1.0);
    let (p, o2) = p.add_f64(2.0);
    let (p, o3) = p.add_f64(3.0);
    let (p, o4) = p.add_f64(0.5);
    let (p, o5) = p.add_f64(2.0);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .load_const(3, o3)
        .load_const(4, o4)
        .load_const(5, o5)
        .inst(OpCode::FAdd, 6, 1, 2) // 3.0
        .inst(OpCode::FSub, 7, 3, 4) // 2.5
        .inst(OpCode::FMul, 8, 6, 7) // 7.5
        .inst(OpCode::FDiv, 0, 8, 5) // 3.75
        .halt();
    assert_reg_f64!(run(&bc), 0, 3.75);
}

#[test]
fn add_many_small_numbers() {
    // 255 * 100 = 25500 через Load8 (max imm8 = 255)
    let p = Program::new().load8(0, 0);
    let mut tmp = p;
    tmp = tmp.load8(1, 255);
    for _ in 0..100 {
        tmp = tmp.inst(OpCode::Add, 0, 0, 1);
    }
    let bc = tmp.halt();
    assert_reg!(run(&bc), 0, 25500);
}

#[test]
fn div_and_mul_roundtrip() {
    // (a / b) * b should be close to a for exact division
    let bc = Program::new()
        .load8(1, 100)
        .load8(2, 4)
        .inst(OpCode::UDiv, 3, 1, 2) // 25
        .inst(OpCode::Mul, 0, 3, 2) // 100
        .halt();
    assert_reg!(run(&bc), 0, 100);
}

#[test]
fn remainder_property() {
    // a == (a / b) * b + (a % b)
    let bc = Program::new()
        .load8(1, 17)
        .load8(2, 5)
        .inst(OpCode::UDiv, 3, 1, 2) // 3
        .inst(OpCode::URem, 4, 1, 2) // 2
        .inst(OpCode::Mul, 5, 3, 2) // 15
        .inst(OpCode::Add, 0, 5, 4) // 17
        .halt();
    assert_reg!(run(&bc), 0, 17);
}

#[test]
fn load_const_multiple_values() {
    let (p, o1) = Program::new().add_u64(1000);
    let (p, o2) = p.add_u64(2000);
    let (p, o3) = p.add_u64(3000);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .load_const(3, o3)
        .inst(OpCode::Add, 4, 1, 2)
        .inst(OpCode::Add, 0, 4, 3)
        .halt();
    assert_reg!(run(&bc), 0, 6000);
}

#[test]
fn result_register_r255() {
    // Проверяем что арифметика работает в регистре r255
    let bc = Program::new()
        .load8(253, 20)
        .load8(254, 22)
        .inst(OpCode::Add, 255, 253, 254)
        .halt();
    let regs = run(&bc);
    assert_eq!(regs[255], 42);
}

#[test]
fn float_epsilon_addition() {
    // Проверяем что маленькие числа не теряются
    let (p, o1) = Program::new().add_f64(1.0);
    let (p, o2) = p.add_f64(f64::EPSILON);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::FAdd, 0, 1, 2)
        .halt();
    let regs = run(&bc);
    let got = f64::from_bits(regs[0]);
    assert!(got > 1.0, "1.0 + epsilon должно быть > 1.0");
}

#[test]
fn unsigned_vs_signed_division_differ() {
    // -1 как u64 = u64::MAX, деление даёт разные результаты
    let (p, o1) = Program::new().add_u64((-1i64) as u64); // u64::MAX
    let bc = p
        .load_const(1, o1)
        .load8(2, 2)
        .inst(OpCode::UDiv, 3, 1, 2) // u64::MAX / 2 = большое число
        .inst(OpCode::IDiv, 4, 1, 2) // -1 / 2 = 0 (truncated)
        .halt();
    let regs = run(&bc);
    assert!(regs[3] > 1_000_000, "UDiv должен дать большое число");
    assert_eq!(regs[4] as i64, 0, "IDiv(-1, 2) должен дать 0");
}

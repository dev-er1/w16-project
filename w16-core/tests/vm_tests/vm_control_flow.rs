// w16-core\tests\vm_tests\vm_control_flow.rs
//
//! Тесты инструкций управления потоком: Jmp, Jnz, Jz, Halt, Select,
//! циклы, вложенные условия, граничные случаи.

use crate::{
    vm_tests::helpers::{Program, run},
    *,
};
use w16_core::bytecode::OpCode;

// =============================================================================
// HALT
// =============================================================================

#[test]
fn halt_stops_execution() {
    // Инструкции после Halt не должны выполняться
    let bc = Program::new()
        .load8(0, 42)
        .inst(OpCode::Halt, 0, 0, 0)
        .load8(0, 99) // не должна выполниться
        .build();
    assert_reg!(run(&bc), 0, 42);
}

#[test]
fn halt_at_start() {
    let bc = Program::new().inst(OpCode::Halt, 0, 0, 0).build();
    let regs = run(&bc);
    assert_reg!(regs, 0, 0); // все регистры = 0
}

// =============================================================================
// JMP — безусловный переход
// =============================================================================

#[test]
fn jmp_forward_skip() {
    // Порядок: load8(r1,4)[0], Jmp[1], load8(r0,99)[2], Halt[3]
    // Jmp прыгает на ip=4 = Halt[3]? Нет, ip=4 это следующий после halt
    // Перепишем с явными адресами
    // ip0: Load8 r1, 3    <- r1 = 3 (адрес Halt)
    // ip1: Jmp r1         <- прыгаем на ip=3
    // ip2: Load8 r0, 99   <- пропускаем
    // ip3: Halt
    let bc = Program::new()
        .load8(1, 3)
        .inst(OpCode::Jmp, 1, 0, 0)
        .load8(0, 99)
        .inst(OpCode::Halt, 0, 0, 0)
        .build();
    assert_reg!(run(&bc), 0, 0); // r0 не трогался
}

#[test]
fn jmp_backward_loop() {
    // Счётчик: уменьшаем r0 от 5 до 0
    // ip0: Load8 r0, 5
    // ip1: Load8 r1, 1   <- шаг
    // ip2: Load8 r2, 5   <- адрес Halt (ip=5)
    // ip3: Sub r0, r0, r1
    // ip4: Jnz r0, r3    <- r3 = 3 (ip loop start)
    // ip5: Halt
    let bc = Program::new()
        .load8(0, 5)
        .load8(1, 1)
        .load8(3, 3) // адрес loop = ip3
        .inst(OpCode::Sub, 0, 0, 1) // ip3
        .inst(OpCode::Jnz, 0, 3, 0) // ip4
        .inst(OpCode::Halt, 0, 0, 0) // ip5
        .build();
    assert_reg!(run(&bc), 0, 0);
}

// =============================================================================
// JNZ — прыжок если не ноль
// =============================================================================

#[test]
fn jnz_taken_when_nonzero() {
    // r0=1 -> прыгаем, r10 остаётся 0
    // ip0: Load8 r0, 1
    // ip1: Load8 r1, 3    <- target ip=3
    // ip2: Jnz r0, r1
    // ip3: нет — нет инструкции которая меняет r10, только Halt
    // Исправим: прыгаем через инструкцию которая ставит r10=99
    // ip0: Load8 r0, 1
    // ip1: Load8 r1, 4    <- ip4=Halt
    // ip2: Jnz r0, r1
    // ip3: Load8 r10, 99  <- пропускается
    // ip4: Halt
    let bc = Program::new()
        .load8(0, 1)
        .load8(1, 4)
        .inst(OpCode::Jnz, 0, 1, 0)
        .load8(10, 99)
        .inst(OpCode::Halt, 0, 0, 0)
        .build();
    assert_reg!(run(&bc), 10, 0);
}

#[test]
fn jnz_not_taken_when_zero() {
    // r0=0 -> не прыгаем, r10 устанавливается в 99
    // ip0: Load8 r0, 0
    // ip1: Load8 r1, 4    <- ip4=Halt
    // ip2: Jnz r0, r1
    // ip3: Load8 r10, 99
    // ip4: Halt
    let bc = Program::new()
        .load8(0, 0)
        .load8(1, 4)
        .inst(OpCode::Jnz, 0, 1, 0)
        .load8(10, 99)
        .inst(OpCode::Halt, 0, 0, 0)
        .build();
    assert_reg!(run(&bc), 10, 99);
}

#[test]
fn jnz_with_large_nonzero() {
    let (p, off) = Program::new().add_u64(u64::MAX);
    let bc = p
        .load_const(0, off)
        .load8(1, 4)
        .inst(OpCode::Jnz, 0, 1, 0)
        .load8(10, 99)
        .inst(OpCode::Halt, 0, 0, 0)
        .build();
    assert_reg!(run(&bc), 10, 0); // прыгнули
}

// =============================================================================
// JZ — прыжок если ноль
// =============================================================================

#[test]
fn jz_taken_when_zero() {
    // ip0: Load8 r0, 0
    // ip1: Load8 r1, 4
    // ip2: Jz r0, r1
    // ip3: Load8 r10, 99  <- пропускается
    // ip4: Halt
    let bc = Program::new()
        .load8(0, 0)
        .load8(1, 4)
        .inst(OpCode::Jz, 0, 1, 0)
        .load8(10, 99)
        .inst(OpCode::Halt, 0, 0, 0)
        .build();
    assert_reg!(run(&bc), 10, 0);
}

#[test]
fn jz_not_taken_when_nonzero() {
    let bc = Program::new()
        .load8(0, 1)
        .load8(1, 4)
        .inst(OpCode::Jz, 0, 1, 0)
        .load8(10, 99)
        .inst(OpCode::Halt, 0, 0, 0)
        .build();
    assert_reg!(run(&bc), 10, 99);
}

#[test]
fn jz_after_comparison() {
    // if (r1 == r2) r0 = 1
    // ip0: Load8 r1, 5
    // ip1: Load8 r2, 5
    // ip2: IEq r3, r1, r2    <- r3 = 1
    // ip3: Load8 r4, 6       <- ip6=Halt
    // ip4: Jz r3, r4         <- если r3==0 прыгаем на 6, иначе продолжаем
    // ip5: Load8 r0, 1
    // ip6: Halt
    let bc = Program::new()
        .load8(1, 5)
        .load8(2, 5)
        .inst(OpCode::IEq, 3, 1, 2)
        .load8(4, 6)
        .inst(OpCode::Jz, 3, 4, 0)
        .load8(0, 1)
        .inst(OpCode::Halt, 0, 0, 0)
        .build();
    assert_reg!(run(&bc), 0, 1);
}

// =============================================================================
// LOOPS — циклы
// =============================================================================

#[test]
fn loop_sum_1_to_10() {
    let bc = Program::new()
        .load8(0, 0)
        .load8(1, 1)
        .load8(2, 10)
        .load8(3, 1)
        .load8(4, 4)
        .inst(OpCode::IULe, 5, 1, 2)
        .load8(6, 12)
        .inst(OpCode::Jz, 5, 6, 0)
        .inst(OpCode::Add, 0, 0, 1)
        .inst(OpCode::Add, 1, 1, 3)
        .inst(OpCode::Jmp, 4, 0, 0)
        .inst(OpCode::Halt, 0, 0, 0)
        .build();
    assert_reg!(run(&bc), 0, 55);
}

#[test]
fn loop_1000_iterations() {
    // Считаем 1000 итераций
    // ip0: Load16 r0, 1000  <- counter
    // ip1: Load8  r1, 1     <- step
    // ip2: Load8  r2, 2     <- loop_ip = ip2
    // ip3: Load8  r3, 6     <- halt_ip = ip6
    // ip2: Sub r0, r0, r1
    // ip3: Jnz r0, r2
    // ip4: Halt
    // Пересчитаем ip:
    let bc = Program::new()
        .load16(0, 1000) // ip0
        .load8(1, 1) // ip1
        .load8(2, 3) // ip2: loop_start = ip3
        .inst(OpCode::Sub, 0, 0, 1) // ip3
        .inst(OpCode::Jnz, 0, 2, 0) // ip4
        .inst(OpCode::Halt, 0, 0, 0) // ip5
        .build();
    assert_reg!(run(&bc), 0, 0);
}

#[test]
fn loop_not_entered_when_condition_false() {
    // i = 10, while i < 5: r0++
    // Цикл не должен выполниться ни разу
    let bc = Program::new()
        .load8(0, 0) // result
        .load8(1, 10) // i = 10
        .load8(2, 5) // limit = 5
        .load8(3, 3) // loop_ip = ip3
        // ip3: check
        .inst(OpCode::IULt, 4, 1, 2) // ip3: i < 5?
        .load8(5, 9) // ip4: halt = ip7
        .inst(OpCode::Jz, 4, 5, 0) // ip5: if false goto halt
        .inst(OpCode::Add, 0, 0, 4) // ip6: r0++ (never)
        .inst(OpCode::Jmp, 3, 0, 0) // ip7: goto loop (never)
        .inst(OpCode::Halt, 0, 0, 0) // ip8
        .build();
    assert_reg!(run(&bc), 0, 0);
}

#[test]
fn loop_single_iteration() {
    let bc = Program::new()
        .load8(0, 1) // counter = 1
        .load8(1, 1) // step
        .load8(2, 2) // loop_ip=ip2
        .inst(OpCode::Sub, 0, 0, 1) // ip2: counter--
        .inst(OpCode::Jnz, 0, 2, 0) // ip3: if counter != 0 loop
        .load8(10, 42) // ip4: executed once
        .inst(OpCode::Halt, 0, 0, 0) // ip5
        .build();
    assert_reg!(run(&bc), 10, 42);
}

// =============================================================================
// SELECT
// =============================================================================

#[test]
fn select_takes_then_when_nonzero() {
    // r0 = (r0 != 0) ? r1 : r2
    // r0=1, r1=10, r2=20 -> result=10
    let bc = Program::new()
        .load8(0, 1)
        .load8(1, 10)
        .load8(2, 20)
        .inst(OpCode::Select, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 10);
}

#[test]
fn select_takes_else_when_zero() {
    // r0=0, r1=10, r2=20 -> result=20
    let bc = Program::new()
        .load8(0, 0)
        .load8(1, 10)
        .load8(2, 20)
        .inst(OpCode::Select, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 20);
}

#[test]
fn select_with_comparison() {
    // r3 = (5 > 3) ? 100 : 200 -> 100
    let bc = Program::new()
        .load8(1, 5)
        .load8(2, 3)
        .inst(OpCode::ISGt, 0, 1, 2) // r0 = 5>3 = 1
        .load8(3, 100)
        .load8(4, 200)
        .inst(OpCode::Select, 0, 3, 4)
        .halt();
    assert_reg!(run(&bc), 0, 100);
}

#[test]
fn select_with_false_comparison() {
    // r0 = (3 > 5) ? 100 : 200 -> 200
    let bc = Program::new()
        .load8(1, 3)
        .load8(2, 5)
        .inst(OpCode::ISGt, 0, 1, 2) // r0 = 3>5 = 0
        .load8(3, 100)
        .load8(4, 200)
        .inst(OpCode::Select, 0, 3, 4)
        .halt();
    assert_reg!(run(&bc), 0, 200);
}

// =============================================================================
// COMPARISONS — сравнения
// =============================================================================

#[test]
fn ieq_equal() {
    let bc = Program::new()
        .load8(1, 42)
        .load8(2, 42)
        .inst(OpCode::IEq, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 1);
}

#[test]
fn ieq_not_equal() {
    let bc = Program::new()
        .load8(1, 42)
        .load8(2, 43)
        .inst(OpCode::IEq, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 0);
}

#[test]
fn ine_equal() {
    let bc = Program::new()
        .load8(1, 5)
        .load8(2, 5)
        .inst(OpCode::INe, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 0);
}

#[test]
fn ine_not_equal() {
    let bc = Program::new()
        .load8(1, 5)
        .load8(2, 6)
        .inst(OpCode::INe, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 1);
}

#[test]
fn islt_true() {
    let bc = Program::new()
        .load8(1, 3)
        .load8(2, 5)
        .inst(OpCode::ISLt, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 1);
}

#[test]
fn islt_false() {
    let bc = Program::new()
        .load8(1, 5)
        .load8(2, 3)
        .inst(OpCode::ISLt, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 0);
}

#[test]
fn islt_negative() {
    let (p, off) = Program::new().add_u64((-5i64) as u64);
    let bc = p
        .load_const(1, off)
        .load8(2, 0)
        .inst(OpCode::ISLt, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 1); // -5 < 0
}

#[test]
fn isgt_true() {
    let bc = Program::new()
        .load8(1, 10)
        .load8(2, 5)
        .inst(OpCode::ISGt, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 1);
}

#[test]
fn isle_equal() {
    let bc = Program::new()
        .load8(1, 5)
        .load8(2, 5)
        .inst(OpCode::ISLe, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 1);
}

#[test]
fn isge_equal() {
    let bc = Program::new()
        .load8(1, 5)
        .load8(2, 5)
        .inst(OpCode::ISGe, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 1);
}

#[test]
fn iult_unsigned_large() {
    // u64::MAX > 5 в unsigned
    let (p, off) = Program::new().add_u64(u64::MAX);
    let bc = p
        .load_const(1, off)
        .load8(2, 5)
        .inst(OpCode::IULt, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 0); // MAX не < 5
}

#[test]
fn iugt_unsigned() {
    let (p, off) = Program::new().add_u64(u64::MAX);
    let bc = p
        .load_const(1, off)
        .load8(2, 5)
        .inst(OpCode::IUGt, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 1); // MAX > 5
}

// =============================================================================
// BITWISE
// =============================================================================

#[test]
fn and_basic() {
    let bc = Program::new()
        .load8(1, 0b1100)
        .load8(2, 0b1010)
        .inst(OpCode::And, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 0b1000);
}

#[test]
fn or_basic() {
    let bc = Program::new()
        .load8(1, 0b1100)
        .load8(2, 0b1010)
        .inst(OpCode::Or, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 0b1110);
}

#[test]
fn xor_basic() {
    let bc = Program::new()
        .load8(1, 0b1100)
        .load8(2, 0b1010)
        .inst(OpCode::Xor, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 0b0110);
}

#[test]
fn not_basic() {
    let bc = Program::new().load8(1, 0).inst(OpCode::Not, 0, 1, 0).halt();
    assert_reg!(run(&bc), 0, u64::MAX);
}

#[test]
fn shl_basic() {
    let bc = Program::new()
        .load8(1, 1)
        .load8(2, 10)
        .inst(OpCode::Shl, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 1024);
}

#[test]
fn shr_basic() {
    let bc = Program::new()
        .load8(1, 128)
        .load8(2, 3)
        .inst(OpCode::Shr, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 16);
}

#[test]
fn sar_sign_extends() {
    let (p, off) = Program::new().add_u64((-8i64) as u64);
    let bc = p
        .load_const(1, off)
        .load8(2, 1)
        .inst(OpCode::Sar, 0, 1, 2)
        .halt();
    let regs = run(&bc);
    assert_eq!(regs[0] as i64, -4);
}

#[test]
fn shl_by_zero() {
    let bc = Program::new()
        .load8(1, 42)
        .load8(2, 0)
        .inst(OpCode::Shl, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 42);
}

#[test]
fn xor_self_is_zero() {
    let bc = Program::new()
        .load8(1, 0xFF)
        .inst(OpCode::Xor, 0, 1, 1)
        .halt();
    assert_reg!(run(&bc), 0, 0);
}

#[test]
fn and_with_mask() {
    let (p, o1) = Program::new().add_u64(0xDEADBEEF);
    let (p, o2) = p.add_u64(0x0000FFFF);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::And, 0, 1, 2)
        .halt();
    assert_reg!(run(&bc), 0, 0x0000BEEF);
}

// =============================================================================
// NESTED CONTROL FLOW
// =============================================================================

#[test]
fn if_else_chain() {
    // if x==1: r0=10 elif x==2: r0=20 else: r0=30
    // Используем Select
    let bc = Program::new()
        .load8(5, 2) // x = 2
        .load8(6, 1) // cmp1 = 1
        .load8(7, 2) // cmp2 = 2
        .inst(OpCode::IEq, 1, 5, 6) // r1 = (x==1)
        .inst(OpCode::IEq, 2, 5, 7) // r2 = (x==2)
        .load8(10, 10)
        .load8(11, 20)
        .load8(12, 30)
        .inst(OpCode::Select, 2, 11, 12) // r2 = x==2 ? 20 : 30
        .inst(OpCode::Select, 1, 10, 2) // r1 = x==1 ? 10 : r2
        .inst(OpCode::Mov, 0, 1, 0)
        .halt();
    assert_reg!(run(&bc), 0, 20);
}

#[test]
fn fibonacci_7_iterative() {
    // fib(7) = 13
    // fib: 0,1,1,2,3,5,8,13
    // a=0, b=1, loop 7 times: tmp=a+b; a=b; b=tmp
    // ip0: r0=0 (a), ip1: r1=1 (b), ip2: r2=7 (count), ip3: r3=1 (step)
    // ip4: r4=3 (loop_ip)
    // loop: ip5: Jz r2, exit ... let's compute
    // Проще: развернём 7 итераций
    let bc = Program::new()
        .load8(0, 0) // a
        .load8(1, 1) // b
        // tmp = a+b; a=b; b=tmp  × 7
        .inst(OpCode::Add, 2, 0, 1)
        .inst(OpCode::Mov, 0, 1, 0)
        .inst(OpCode::Mov, 1, 2, 0)
        .inst(OpCode::Add, 2, 0, 1)
        .inst(OpCode::Mov, 0, 1, 0)
        .inst(OpCode::Mov, 1, 2, 0)
        .inst(OpCode::Add, 2, 0, 1)
        .inst(OpCode::Mov, 0, 1, 0)
        .inst(OpCode::Mov, 1, 2, 0)
        .inst(OpCode::Add, 2, 0, 1)
        .inst(OpCode::Mov, 0, 1, 0)
        .inst(OpCode::Mov, 1, 2, 0)
        .inst(OpCode::Add, 2, 0, 1)
        .inst(OpCode::Mov, 0, 1, 0)
        .inst(OpCode::Mov, 1, 2, 0)
        .inst(OpCode::Add, 2, 0, 1)
        .inst(OpCode::Mov, 0, 1, 0)
        .inst(OpCode::Mov, 1, 2, 0)
        .inst(OpCode::Add, 2, 0, 1)
        .inst(OpCode::Mov, 0, 1, 0)
        .inst(OpCode::Mov, 1, 2, 0)
        .halt();
    // После 7 итераций a = fib(7) = 13
    assert_reg!(run(&bc), 0, 13);
}

#[test]
fn gcd_euclid() {
    // gcd(48, 18) = 6
    // while b != 0: tmp = b; b = a%b; a = tmp
    // ip0: r0=48, r1=18
    // ip1: r2=1 (step dummy), r3=loop_ip
    // Реализуем развёрткой: 48%18=12, 18%12=6, 12%6=0 -> 6
    let bc = Program::new()
        .load8(0, 48)
        .load8(1, 18)
        // iter1: tmp=18, b=48%18=12, a=18
        .inst(OpCode::URem, 2, 0, 1)
        .inst(OpCode::Mov, 0, 1, 0)
        .inst(OpCode::Mov, 1, 2, 0)
        // iter2: tmp=12, b=18%12=6, a=12
        .inst(OpCode::URem, 2, 0, 1)
        .inst(OpCode::Mov, 0, 1, 0)
        .inst(OpCode::Mov, 1, 2, 0)
        // iter3: tmp=6, b=12%6=0, a=6
        .inst(OpCode::URem, 2, 0, 1)
        .inst(OpCode::Mov, 0, 1, 0)
        .inst(OpCode::Mov, 1, 2, 0)
        .halt();
    assert_reg!(run(&bc), 0, 6);
}

#[test]
fn max_of_three() {
    // max(3, 7, 5) = 7
    let bc = Program::new()
        .load8(1, 3)
        .load8(2, 7)
        .load8(3, 5)
        .inst(OpCode::ISGt, 4, 1, 2) // 3>7? = 0
        .inst(OpCode::Select, 4, 1, 2) // max(3,7) = 7 -> r4
        .inst(OpCode::ISGt, 5, 4, 3) // 7>5? = 1
        .inst(OpCode::Select, 5, 4, 3) // max(7,5) = 7 -> r5
        .inst(OpCode::Mov, 0, 5, 0)
        .halt();
    assert_reg!(run(&bc), 0, 7);
}

#[test]
fn absolute_value_via_select() {
    // abs(-42) = 42
    let (p, off) = Program::new().add_u64((-42i64) as u64);
    let bc = p
        .load_const(1, off)
        .inst(OpCode::Neg, 2, 1, 0) // r2 = 42
        .load8(3, 0)
        .inst(OpCode::ISGt, 4, 1, 3) // r4 = (-42 > 0)? = 0
        .inst(OpCode::Select, 4, 1, 2) // r4 = (negative) ? orig : neg
        .inst(OpCode::Mov, 0, 4, 0)
        .halt();
    let regs = run(&bc);
    assert_eq!(regs[0] as i64, 42);
}

#[test]
fn collatz_6_steps() {
    // Коллатц для n=6: 6->3->10->5->16->8->4->2->1 = 8 шагов
    // Развернём вручную
    let bc = Program::new()
        .load8(0, 0) // steps
        .load8(1, 6) // n
        .load8(2, 2) // divisor for mod
        .load8(3, 1) // step counter
        .load8(4, 3) // multiplier
        // iter1: n=6, even -> n=3, steps=1
        .inst(OpCode::URem, 5, 1, 2) // n%2=0
        // n/2
        .inst(OpCode::UDiv, 1, 1, 2) // n=3
        .inst(OpCode::Add, 0, 0, 3) // steps=1
        // iter2: n=3, odd -> n=10, steps=2
        .inst(OpCode::Mul, 1, 1, 4) // n=9
        .inst(OpCode::Add, 1, 1, 3) // n=10
        .inst(OpCode::Add, 0, 0, 3) // steps=2
        // iter3: n=10, even -> n=5, steps=3
        .inst(OpCode::UDiv, 1, 1, 2) // n=5
        .inst(OpCode::Add, 0, 0, 3)
        // iter4: n=5, odd -> n=16
        .inst(OpCode::Mul, 1, 1, 4)
        .inst(OpCode::Add, 1, 1, 3) // n=16
        .inst(OpCode::Add, 0, 0, 3)
        // iter5: n=16 -> 8
        .inst(OpCode::UDiv, 1, 1, 2)
        .inst(OpCode::Add, 0, 0, 3)
        // iter6: n=8 -> 4
        .inst(OpCode::UDiv, 1, 1, 2)
        .inst(OpCode::Add, 0, 0, 3)
        // iter7: n=4 -> 2
        .inst(OpCode::UDiv, 1, 1, 2)
        .inst(OpCode::Add, 0, 0, 3)
        // iter8: n=2 -> 1
        .inst(OpCode::UDiv, 1, 1, 2)
        .inst(OpCode::Add, 0, 0, 3)
        .halt();
    assert_reg!(run(&bc), 0, 8);
}

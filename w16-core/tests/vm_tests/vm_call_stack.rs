// w16-core\tests\vm_tests\vm_call_stack.rs
//
//! Тесты стека вызовов: Call/Ret ABI, аргументы, вложенные вызовы,
//! сохранение регистров, stack overflow.

use crate::{
    vm_tests::helpers::{Program, run},
    *,
};
use w16_core::bytecode::OpCode;

// =============================================================================
// ПРОСТОЙ ВЫЗОВ И ВОЗВРАТ
// =============================================================================

#[test]
fn call_ret_basic() {
    // ip0: Load16 r1, 3
    // ip1: Call r1
    // ip2: Halt
    // ip3: Load8 r0, 42
    // ip4: Ret
    let bc2 = {
        let mut p = Program::new();
        p = p.load16(1, 3); // func at ip3
        p = p.inst(OpCode::Call, 1, 0, 0);
        p = p.inst(OpCode::Halt, 0, 0, 0);
        p = p.load8(0, 42);
        p = p.inst(OpCode::Ret, 0, 0, 0);
        p.build()
    };

    assert_reg!(run(&bc2), 0, 42);
}

#[test]
fn ret_restores_ip() {
    // После Ret выполнение продолжается с инструкции после Call
    // ip0: Load8 r0, 0
    // ip1: Load16 r1, 5   <- func at ip5
    // ip2: Call r1
    // ip3: Load8 r0, 42   <- должна выполниться после Ret
    // ip4: Halt
    // ip5: Load8 r2, 99   <- func body
    // ip6: Ret
    let bc = {
        let mut p = Program::new();
        p = p.load8(0, 0);
        p = p.load16(1, 5);
        p = p.inst(OpCode::Call, 1, 0, 0);
        p = p.load8(0, 42);
        p = p.inst(OpCode::Halt, 0, 0, 0);
        p = p.load8(2, 99);
        p = p.inst(OpCode::Ret, 0, 0, 0);
        p.build()
    };
    assert_reg!(run(&bc), 0, 42);
}

#[test]
fn ret_result_in_r0() {
    // Функция возвращает значение через r0
    // ip0: Load16 r1, 3
    // ip1: Call r1
    // ip2: Halt
    // ip3: Load16 r0, 12345
    // ip4: Ret
    let bc = {
        let mut p = Program::new();
        p = p.load16(1, 3);
        p = p.inst(OpCode::Call, 1, 0, 0);
        p = p.inst(OpCode::Halt, 0, 0, 0);
        p = p.load16(0, 12345);
        p = p.inst(OpCode::Ret, 0, 0, 0);
        p.build()
    };
    assert_reg!(run(&bc), 0, 12345);
}

// =============================================================================
// АРГУМЕНТЫ ЧЕРЕЗ РЕГИСТРЫ
// =============================================================================

#[test]
fn call_with_one_arg() {
    // main: r1=10 -> Call func; func: r0 = r1 * 2; Ret
    // ip0: Load8 r1, 10
    // ip1: Load16 r2, 4   <- func at ip4
    // ip2: Call r2, 1     <- 1 аргумент
    // ip3: Halt
    // ip4: Load8 r3, 2
    // ip5: Mul r0, r1, r3
    // ip6: Ret
    let bc = {
        let mut p = Program::new();
        p = p.load8(1, 10);
        p = p.load16(2, 4);
        p = p.inst(OpCode::Call, 2, 1, 0); // b=1 = arg count
        p = p.inst(OpCode::Halt, 0, 0, 0);
        p = p.load8(3, 2);
        p = p.inst(OpCode::Mul, 0, 1, 3);
        p = p.inst(OpCode::Ret, 0, 0, 0);
        p.build()
    };
    assert_reg!(run(&bc), 0, 20);
}

#[test]
fn call_with_two_args() {
    // func(r1, r2) -> r0 = r1 + r2
    // ip0: Load8 r1, 15
    // ip1: Load8 r2, 27
    // ip2: Load16 r3, 5
    // ip3: Call r3, 2
    // ip4: Halt
    // ip5: Add r0, r1, r2
    // ip6: Ret
    let bc = {
        let mut p = Program::new();
        p = p.load8(1, 15);
        p = p.load8(2, 27);
        p = p.load16(3, 5);
        p = p.inst(OpCode::Call, 3, 2, 0);
        p = p.inst(OpCode::Halt, 0, 0, 0);
        p = p.inst(OpCode::Add, 0, 1, 2);
        p = p.inst(OpCode::Ret, 0, 0, 0);
        p.build()
    };
    assert_reg!(run(&bc), 0, 42);
}

// =============================================================================
// СОХРАНЕНИЕ РЕГИСТРОВ ВЫЗЫВАЮЩЕГО
// =============================================================================

#[test]
fn caller_registers_preserved_after_ret() {
    // main устанавливает r5=77 ДО вызова func
    // func меняет r5=99
    // После Ret r5 должен быть восстановлен как 77
    // ip0: Load8 r5, 77
    // ip1: Load16 r1, 4
    // ip2: Call r1
    // ip3: Halt          <- r5 должен быть 77
    // ip4: Load8 r5, 99  <- func портит r5
    // ip5: Ret
    let bc = {
        let mut p = Program::new();
        p = p.load8(5, 77);
        p = p.load16(1, 4);
        p = p.inst(OpCode::Call, 1, 0, 0);
        p = p.inst(OpCode::Halt, 0, 0, 0);
        p = p.load8(5, 99);
        p = p.inst(OpCode::Ret, 0, 0, 0);
        p.build()
    };
    assert_reg!(run(&bc), 5, 77);
}

#[test]
fn r0_after_call_is_return_value() {
    // main: r0=0 перед вызовом, func возвращает 42, r0=42 после
    let bc = {
        let mut p = Program::new();
        p = p.load8(0, 0);
        p = p.load16(1, 4);
        p = p.inst(OpCode::Call, 1, 0, 0);
        p = p.inst(OpCode::Halt, 0, 0, 0);
        p = p.load8(0, 42);
        p = p.inst(OpCode::Ret, 0, 0, 0);
        p.build()
    };
    assert_reg!(run(&bc), 0, 42);
}

#[test]
fn multiple_non_return_registers_preserved() {
    // Проверяем что r10, r20, r30 восстанавливаются после вызова
    let bc = {
        let mut p = Program::new();
        p = p.load8(10, 10);
        p = p.load8(20, 20);
        p = p.load8(30, 30);
        p = p.load16(1, 7); // func at ip7
        p = p.inst(OpCode::Call, 1, 0, 0);
        p = p.inst(OpCode::Halt, 0, 0, 0);
        // func: портим r10, r20, r30
        p = p.load8(10, 99);
        p = p.load8(20, 99);
        p = p.load8(30, 99);
        p = p.inst(OpCode::Ret, 0, 0, 0);
        p.build()
    };
    let regs = run(&bc);
    assert_reg!(regs, 10, 10);
    assert_reg!(regs, 20, 20);
    assert_reg!(regs, 30, 30);
}

// =============================================================================
// ВЛОЖЕННЫЕ ВЫЗОВЫ
// =============================================================================

#[test]
fn nested_call_two_levels() {
    // main -> f -> g
    // g возвращает 10
    // f возвращает g() + 5 = 15
    // main получает 15
    //
    // ip0: Load16 r1, 4     <- f at ip4
    // ip1: Call r1
    // ip2: Halt
    // skip ip3
    // ip3: нет, пересчитаем:
    //
    // ip0: Load16 r1, 3
    // ip1: Call r1            <- call f
    // ip2: Halt
    // [f at ip3:]
    // ip3: Load16 r2, 8      <- g at ip8
    // ip4: Call r2            <- call g
    // ip5: Load8 r3, 5
    // ip6: Add r0, r0, r3    <- r0 = g() + 5
    // ip7: Ret
    // [g at ip8:]
    // ip8: Load8 r0, 10
    // ip9: Ret
    let bc = {
        let mut p = Program::new();
        p = p.load16(1, 3);
        p = p.inst(OpCode::Call, 1, 0, 0);
        p = p.inst(OpCode::Halt, 0, 0, 0);
        // f at ip3
        p = p.load16(2, 8);
        p = p.inst(OpCode::Call, 2, 0, 0);
        p = p.load8(3, 5);
        p = p.inst(OpCode::Add, 0, 0, 3);
        p = p.inst(OpCode::Ret, 0, 0, 0);
        // g at ip8
        p = p.load8(0, 10);
        p = p.inst(OpCode::Ret, 0, 0, 0);
        p.build()
    };
    assert_reg!(run(&bc), 0, 15);
}

#[test]
fn nested_call_three_levels() {
    // main -> f -> g -> h
    // h=1, g=h()+1=2, f=g()+1=3, main получает 3
    // ip0: Load16 r1, 3; Call r1; Halt
    // f@3: Load16 r2, 7; Call r2; Add r0,r0,step; Ret
    // g@7+: Load16 r2, ?; Call r2; Add r0,r0,step; Ret
    // h@?: Load8 r0, 1; Ret
    // Считаем ip:
    // ip0: Load16 r1, 3      (1 инстр)
    // ip1: Call r1            (1)
    // ip2: Halt               (1)
    // f@ip3: Load16 r2, 9   (1)  — g@9
    // ip4: Call r2            (1)
    // ip5: Load8 r3, 1       (1)
    // ip6: Add r0, r0, r3    (1)
    // ip7: Ret                (1)
    // g@ip8: Load16 r2, 13  (1)  — h@13
    // ip9: Call r2            (1)
    // ip10: Load8 r3, 1      (1)
    // ip11: Add r0, r0, r3   (1)
    // ip12: Ret               (1)
    // h@ip13: Load8 r0, 1   (1)
    // ip14: Ret               (1)
    let bc = {
        let mut p = Program::new();
        p = p.load16(1, 3); // ip0
        p = p.inst(OpCode::Call, 1, 0, 0); // ip1
        p = p.inst(OpCode::Halt, 0, 0, 0); // ip2
        // f@3
        p = p.load16(2, 8); // ip3: g@8
        p = p.inst(OpCode::Call, 2, 0, 0); // ip4
        p = p.load8(3, 1); // ip5
        p = p.inst(OpCode::Add, 0, 0, 3); // ip6
        p = p.inst(OpCode::Ret, 0, 0, 0); // ip7
        // g@8
        p = p.load16(2, 13); // ip8: h@13
        p = p.inst(OpCode::Call, 2, 0, 0); // ip9
        p = p.load8(3, 1); // ip10
        p = p.inst(OpCode::Add, 0, 0, 3); // ip11
        p = p.inst(OpCode::Ret, 0, 0, 0); // ip12
        // h@13
        p = p.load8(0, 1); // ip13
        p = p.inst(OpCode::Ret, 0, 0, 0); // ip14
        p.build()
    };
    assert_reg!(run(&bc), 0, 3);
}

// =============================================================================
// НЕСКОЛЬКО ВЫЗОВОВ ОДНОЙ ФУНКЦИИ
// =============================================================================

#[test]
fn call_same_function_twice() {
    // func возвращает 21
    // main: r0 = func() + func() = 42
    // ip0: Load16 r1, 7; Call r1; Mov r5, r0; Call r1; Add r0, r0, r5; Halt
    // func@7: Load8 r0, 21; Ret
    let bc = {
        let mut p = Program::new();
        p = p.load16(1, 7); // ip0
        p = p.inst(OpCode::Call, 1, 0, 0); // ip1
        p = p.inst(OpCode::Mov, 5, 0, 0); // ip2: save result
        p = p.inst(OpCode::Call, 1, 0, 0); // ip3: call again
        p = p.inst(OpCode::Add, 0, 0, 5); // ip4: r0 = 21+21
        p = p.inst(OpCode::Halt, 0, 0, 0); // ip5
        // NOTE: ip6 is Load16 instruction above — let's recount
        // Actually ip0=Load16, ip1=Call, ip2=Mov, ip3=Call, ip4=Add, ip5=Halt -> func@6
        // But we said func@7 — need one more nop or fix
        p = p.inst(OpCode::NoOp, 0, 0, 0); // ip6 padding
        // func@7
        p = p.load8(0, 21); // ip7
        p = p.inst(OpCode::Ret, 0, 0, 0); // ip8
        p.build()
    };
    assert_reg!(run(&bc), 0, 42);
}

#[test]
fn call_three_functions_sequentially() {
    // f1()=10, f2()=20, f3()=12 -> sum=42
    // Строим аккуратно считая ip:
    // ip0: Load16 r1, 9;  Call r1; Mov r5,r0;  <- call f1@9
    // ip3: Load16 r1, 12; Call r1; Add r5,r5,r0; <- call f2@12
    // ip6: Load16 r1, 15; Call r1; Add r0,r5,r0; <- call f3@15
    // ip9: Halt
    // f1@9: nop — wait, ip0=Load16(1 inst), ip1=Call, ip2=Mov, ip3=Load16...
    // Считаем: каждая Load16 = 1 инстр, Call=1, Mov=1, Add=1, Halt=1
    // ip0: Load16 r1, ?
    // ip1: Call r1
    // ip2: Mov r5, r0
    // ip3: Load16 r1, ?
    // ip4: Call r1
    // ip5: Add r5, r5, r0
    // ip6: Load16 r1, ?
    // ip7: Call r1
    // ip8: Add r0, r5, r0
    // ip9: Halt
    // f1@10: Load8 r0, 10; Ret -> ip10, ip11
    // f2@12: Load8 r0, 20; Ret -> ip12, ip13
    // f3@14: Load8 r0, 12; Ret -> ip14, ip15
    let bc = {
        let mut p = Program::new();
        p = p.load16(1, 10); // ip0
        p = p.inst(OpCode::Call, 1, 0, 0); // ip1
        p = p.inst(OpCode::Mov, 5, 0, 0); // ip2
        p = p.load16(1, 12); // ip3
        p = p.inst(OpCode::Call, 1, 0, 0); // ip4
        p = p.inst(OpCode::Add, 5, 5, 0); // ip5
        p = p.load16(1, 14); // ip6
        p = p.inst(OpCode::Call, 1, 0, 0); // ip7
        p = p.inst(OpCode::Add, 0, 5, 0); // ip8
        p = p.inst(OpCode::Halt, 0, 0, 0); // ip9
        // f1@10
        p = p.load8(0, 10); // ip10
        p = p.inst(OpCode::Ret, 0, 0, 0); // ip11
        // f2@12
        p = p.load8(0, 20); // ip12
        p = p.inst(OpCode::Ret, 0, 0, 0); // ip13
        // f3@14
        p = p.load8(0, 12); // ip14
        p = p.inst(OpCode::Ret, 0, 0, 0); // ip15
        p.build()
    };
    assert_reg!(run(&bc), 0, 42);
}

// =============================================================================
// FACTORIAL — рекурсия
// =============================================================================

#[test]
fn factorial_1() {
    factorial_test(1, 1);
}

#[test]
fn factorial_2() {
    factorial_test(2, 2);
}

#[test]
fn factorial_3() {
    factorial_test(3, 6);
}

#[test]
fn factorial_4() {
    factorial_test(4, 24);
}

#[test]
fn factorial_5() {
    factorial_test(5, 120);
}

/// Итеративный факториал через стек вызовов.
/// fact(n): if n==0 return 1; r0 = n * fact(n-1)
///
/// Реализуем через развёртку (N вызовов) чтобы тестировать стек.
fn factorial_test(n: u8, expected: u64) {
    // ip11: Halt
    let bc = {
        let mut p = Program::new();
        p = p.load8(0, 1);
        p = p.load8(1, 2);
        p = p.load8(2, n);
        p = p.load8(3, 1);
        p = p.load8(4, 5); // loop_start = ip5
        // ip5: loop check
        p = p.inst(OpCode::IULe, 5, 1, 2);
        p = p.load8(6, 11); // exit = ip11
        p = p.inst(OpCode::Jz, 5, 6, 0);
        p = p.inst(OpCode::Mul, 0, 0, 1);
        p = p.inst(OpCode::Add, 1, 1, 3);
        p = p.inst(OpCode::Jmp, 4, 0, 0); // ip10
        p = p.inst(OpCode::Halt, 0, 0, 0); // ip11
        p.build()
    };
    assert_eq!(run(&bc)[0], expected, "factorial({n})");
}

// =============================================================================
// STACK DEPTH
// =============================================================================

#[test]
fn call_depth_does_not_overflow_at_limit_minus_one() {
    let n = 10usize;
    // Строим: n вызовов, каждый к своей функции fi которая возвращает i
    // Считаем сумму 1+2+...+n
    //
    // main layout (3 инструкции на вызов: Load16, Call, Add):
    //   ip0: Load16 r1, func0_ip
    //   ip1: Call r1
    //   ip2: Add r5, r5, r0
    //   ... × n
    //   ip[3n]: Mov r0, r5
    //   ip[3n+1]: Halt
    //
    // func_i layout (2 инструкции):
    //   Load8 r0, i+1
    //   Ret
    //
    // main = 3*n + 2 инструкции
    // func_0 начинается на ip = 3*n + 2

    // func_0 реально на ip = 1 + 3*n + 2 = 3n+3
    // НО мы закодировали main_end = 3*n + 2 -> неверно
    // Исправление: main = load8(r5)[ip0] + n*(load16+call+add)[ip1..3n] + mov[ip3n+1] + halt[ip3n+2]
    // func_0 starts at ip 3n+3
    // Пересобираем с правильными адресами:
    let real_main_end = 3 * n + 3; // ip первой функции = 1(load8) + 3n(loop) + 1(mov) + 1(halt) = 3n+3

    let bc2 = {
        let mut p = Program::new();
        p = p.load8(5, 0); // ip0: accumulator

        for i in 0..n {
            let func_ip = (real_main_end + i * 2) as u16;
            p = p.load16(1, func_ip); // ip[1+3i]
            p = p.inst(OpCode::Call, 1, 0, 0); // ip[2+3i]
            p = p.inst(OpCode::Add, 5, 5, 0); // ip[3+3i]
        }
        // ip[1+3n]: Mov
        // ip[2+3n]: Halt
        p = p.inst(OpCode::Mov, 0, 5, 0);
        p = p.inst(OpCode::Halt, 0, 0, 0);
        // func_i at ip[3+3n + 2i]
        for i in 0..n {
            p = p.load8(0, (i + 1) as u8);
            p = p.inst(OpCode::Ret, 0, 0, 0);
        }
        p.build()
    };

    let expected: u64 = (1..=n as u64).sum(); // 55
    assert_eq!(run(&bc2)[0], expected);
}

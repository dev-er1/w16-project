// w16-core\src\interpreter\evms\orca_evm.rs
//
//! # Orca EVM
//!
//! Orca EVM - это экспериментальная виртуальная машина W16, где главный
//! приоритет один: максимальная скорость исполнения bytecode.
//!
//! EVM означает **Experimental Virtual Machine**, а модуль `evms` означает
//! **Experimental Virtual Machines**.
//!
//! ## Главная идея
//!
//! Обычная VM из `interpreter::vm` остается эталонной реализацией: она
//! возвращает `Result`, сообщает ошибки, проверяет опасные ситуации и подходит
//! для запуска непроверенного кода.
//!
//! Orca устроена наоборот. Она считает, что bytecode уже проверен verifier-ом
//! или сгенерирован доверенным компилятором. Поэтому hot loop превращен в
//! unchecked raw-pointer interpreter:
//!
//! - `pc` хранится как `*const Instruction`, а не как индекс;
//! - fallthrough делает `pc = pc.add(1)`;
//! - `Jmp`, `Jnz`, `Jz`, `Call` сразу строят новый `pc` через `code_base.add`;
//! - нет проверки `pc < code_len` на каждой инструкции;
//! - нет `Result` и error path;
//! - нет проверок деления на ноль;
//! - нет проверок границ памяти и constant pool;
//! - регистры и память читаются через raw pointers;
//! - call stack работает через preallocated `MaybeUninit` buffer без `Vec::push`;
//! - `Print*` вынесены в cold-функции, чтобы не раздувать горячий dispatch-код.
//!
//! ## Safety contract
//!
//! [`OrcaEvm::run_unchecked`] является `unsafe`. Caller обязан гарантировать:
//!
//! - `bytecode.instructions` не пустой;
//! - программа завершится через `Halt` или `Ret` из entry-функции;
//! - все opcode-ы валидны;
//! - все переходы и вызовы указывают на существующие инструкции;
//! - все обращения к памяти находятся внутри `self.memory`;
//! - все обращения к constant pool валидны;
//! - все строки в constant pool валидны как UTF-8, если используется `PrintStr`;
//! - делители для `UDiv`, `IDiv`, `URem`, `IRem` не равны нулю;
//! - `i64::MIN / -1` и `i64::MIN % -1` не встречаются в signed division/rem;
//! - глубина вызовов не превышает capacity call stack.
//!
//! Если контракт нарушен, Orca может вернуть неправильный результат, зависнуть,
//! вызвать panic, испортить память процесса или привести к undefined behavior.

use std::mem::MaybeUninit;

use crate::bytecode::{Bytecode, Instruction, OpCode};
use crate::interpreter::vm::REGISTER_COUNT;

const DEFAULT_CALL_DEPTH: usize = 64;

/// Кадр ручного call stack.
///
/// `return_pc` хранится как raw pointer, чтобы `Ret` не пересчитывал индекс в
/// pointer. Регистры сохраняются целиком ради совместимости с текущим W16 ABI.
#[derive(Clone, Copy)]
struct OrcaCallFrame {
    registers: [u64; REGISTER_COUNT],
    return_pc: *const Instruction,
}

/// Сверхбыстрая экспериментальная VM для доверенного W16 bytecode.
///
/// Это не безопасный runtime. Это инструмент для бенчмарков и экспериментов с
/// предельно агрессивным interpreter dispatch.
pub struct OrcaEvm {
    /// Регистровый файл W16: 256 регистров по 64 бита.
    pub registers: [u64; REGISTER_COUNT],
    /// Линейная память программы.
    pub memory: Vec<u8>,
    /// Preallocated call stack. Длина `Vec` намеренно остается нулевой; Orca
    /// пишет прямо в spare capacity.
    call_stack: Vec<MaybeUninit<OrcaCallFrame>>,
    call_depth: usize,
}

impl OrcaEvm {
    /// Создает Orca EVM с памятью `memory_size` и стандартным запасом call stack.
    pub fn new(memory_size: usize) -> Self {
        Self::with_call_capacity(memory_size, DEFAULT_CALL_DEPTH)
    }

    /// Создает Orca EVM с памятью `memory_size` и ручным capacity call stack.
    ///
    /// `call_capacity` является частью safety contract: если bytecode сделает
    /// больше вложенных вызовов, Orca запишет за пределы выделенного буфера.
    pub fn with_call_capacity(memory_size: usize, call_capacity: usize) -> Self {
        Self {
            registers: [0; REGISTER_COUNT],
            memory: vec![0; memory_size],
            call_stack: Vec::with_capacity(call_capacity),
            call_depth: 0,
        }
    }

    /// Сбрасывает регистры и call stack, но не зануляет память.
    #[inline(always)]
    pub fn reset_registers(&mut self) {
        self.registers = [0; REGISTER_COUNT];
        self.call_depth = 0;
    }

    /// Возвращает capacity ручного call stack.
    #[inline(always)]
    pub fn call_capacity(&self) -> usize {
        self.call_stack.capacity()
    }

    /// Запускает bytecode в максимально агрессивном unchecked-режиме.
    ///
    /// # Safety
    ///
    /// Caller обязан выполнить safety contract из документации модуля.
    #[inline(never)]
    pub unsafe fn run_unchecked(&mut self, bytecode: &Bytecode) {
        unsafe {
            let code_base = bytecode.instructions.as_ptr();
            let const_base = bytecode.constant_pool.data.as_ptr();
            let regs = self.registers.as_mut_ptr();
            let mem = self.memory.as_mut_ptr();
            let stack = self.call_stack.as_mut_ptr();

            let mut call_depth = 0usize;
            let mut pc = code_base;

            loop {
                let inst = *pc;

                match inst.opcode {
                    OpCode::Halt => break,
                    OpCode::NoOp => pc = pc.add(1),

                    OpCode::Mov => {
                        *reg(regs, inst.a) = *reg(regs, inst.b);
                        pc = pc.add(1);
                    }
                    OpCode::Load8 => {
                        *reg(regs, inst.a) = inst.b as u64;
                        pc = pc.add(1);
                    }
                    OpCode::Load16 => {
                        *reg(regs, inst.a) = imm16(inst) as u64;
                        pc = pc.add(1);
                    }
                    OpCode::LoadConst => {
                        *reg(regs, inst.a) = load_u64(const_base, imm16(inst) as usize);
                        pc = pc.add(1);
                    }

                    OpCode::Ld8 => {
                        *reg(regs, inst.a) = *mem.add(*reg(regs, inst.b) as usize) as u64;
                        pc = pc.add(1);
                    }
                    OpCode::Ld16 => {
                        *reg(regs, inst.a) = mem
                            .add(*reg(regs, inst.b) as usize)
                            .cast::<u16>()
                            .read_unaligned() as u64;
                        pc = pc.add(1);
                    }
                    OpCode::Ld32 => {
                        *reg(regs, inst.a) = mem
                            .add(*reg(regs, inst.b) as usize)
                            .cast::<u32>()
                            .read_unaligned() as u64;
                        pc = pc.add(1);
                    }
                    OpCode::Ld64 => {
                        *reg(regs, inst.a) = mem
                            .add(*reg(regs, inst.b) as usize)
                            .cast::<u64>()
                            .read_unaligned();
                        pc = pc.add(1);
                    }
                    OpCode::St8 => {
                        *mem.add(*reg(regs, inst.a) as usize) = *reg(regs, inst.b) as u8;
                        pc = pc.add(1);
                    }
                    OpCode::St16 => {
                        mem.add(*reg(regs, inst.a) as usize)
                            .cast::<u16>()
                            .write_unaligned(*reg(regs, inst.b) as u16);
                        pc = pc.add(1);
                    }
                    OpCode::St32 => {
                        mem.add(*reg(regs, inst.a) as usize)
                            .cast::<u32>()
                            .write_unaligned(*reg(regs, inst.b) as u32);
                        pc = pc.add(1);
                    }
                    OpCode::St64 => {
                        mem.add(*reg(regs, inst.a) as usize)
                            .cast::<u64>()
                            .write_unaligned(*reg(regs, inst.b));
                        pc = pc.add(1);
                    }

                    OpCode::Add => {
                        *reg(regs, inst.a) = (*reg(regs, inst.b)).wrapping_add(*reg(regs, inst.c));
                        pc = pc.add(1);
                    }
                    OpCode::Sub => {
                        *reg(regs, inst.a) = (*reg(regs, inst.b)).wrapping_sub(*reg(regs, inst.c));
                        pc = pc.add(1);
                    }
                    OpCode::Mul => {
                        *reg(regs, inst.a) = (*reg(regs, inst.b)).wrapping_mul(*reg(regs, inst.c));
                        pc = pc.add(1);
                    }
                    OpCode::UDiv => {
                        *reg(regs, inst.a) = *reg(regs, inst.b) / *reg(regs, inst.c);
                        pc = pc.add(1);
                    }
                    OpCode::IDiv => {
                        *reg(regs, inst.a) =
                            ((*reg(regs, inst.b) as i64) / (*reg(regs, inst.c) as i64)) as u64;
                        pc = pc.add(1);
                    }
                    OpCode::URem => {
                        *reg(regs, inst.a) = *reg(regs, inst.b) % *reg(regs, inst.c);
                        pc = pc.add(1);
                    }
                    OpCode::IRem => {
                        *reg(regs, inst.a) =
                            ((*reg(regs, inst.b) as i64) % (*reg(regs, inst.c) as i64)) as u64;
                        pc = pc.add(1);
                    }
                    OpCode::Neg => {
                        *reg(regs, inst.a) = (*reg(regs, inst.b) as i64).wrapping_neg() as u64;
                        pc = pc.add(1);
                    }

                    OpCode::FAdd => {
                        *reg(regs, inst.a) = (f64::from_bits(*reg(regs, inst.b))
                            + f64::from_bits(*reg(regs, inst.c)))
                        .to_bits();
                        pc = pc.add(1);
                    }
                    OpCode::FSub => {
                        *reg(regs, inst.a) = (f64::from_bits(*reg(regs, inst.b))
                            - f64::from_bits(*reg(regs, inst.c)))
                        .to_bits();
                        pc = pc.add(1);
                    }
                    OpCode::FMul => {
                        *reg(regs, inst.a) = (f64::from_bits(*reg(regs, inst.b))
                            * f64::from_bits(*reg(regs, inst.c)))
                        .to_bits();
                        pc = pc.add(1);
                    }
                    OpCode::FDiv => {
                        *reg(regs, inst.a) = (f64::from_bits(*reg(regs, inst.b))
                            / f64::from_bits(*reg(regs, inst.c)))
                        .to_bits();
                        pc = pc.add(1);
                    }
                    OpCode::FRem => {
                        *reg(regs, inst.a) = (f64::from_bits(*reg(regs, inst.b))
                            % f64::from_bits(*reg(regs, inst.c)))
                        .to_bits();
                        pc = pc.add(1);
                    }
                    OpCode::FNeg => {
                        *reg(regs, inst.a) = (-f64::from_bits(*reg(regs, inst.b))).to_bits();
                        pc = pc.add(1);
                    }
                    OpCode::FAbs => {
                        *reg(regs, inst.a) = f64::from_bits(*reg(regs, inst.b)).abs().to_bits();
                        pc = pc.add(1);
                    }

                    OpCode::IEq => {
                        *reg(regs, inst.a) = (*reg(regs, inst.b) == *reg(regs, inst.c)) as u64;
                        pc = pc.add(1);
                    }
                    OpCode::INe => {
                        *reg(regs, inst.a) = (*reg(regs, inst.b) != *reg(regs, inst.c)) as u64;
                        pc = pc.add(1);
                    }
                    OpCode::ISLt => {
                        *reg(regs, inst.a) =
                            ((*reg(regs, inst.b) as i64) < (*reg(regs, inst.c) as i64)) as u64;
                        pc = pc.add(1);
                    }
                    OpCode::ISLe => {
                        *reg(regs, inst.a) =
                            ((*reg(regs, inst.b) as i64) <= (*reg(regs, inst.c) as i64)) as u64;
                        pc = pc.add(1);
                    }
                    OpCode::ISGt => {
                        *reg(regs, inst.a) =
                            ((*reg(regs, inst.b) as i64) > (*reg(regs, inst.c) as i64)) as u64;
                        pc = pc.add(1);
                    }
                    OpCode::ISGe => {
                        *reg(regs, inst.a) =
                            ((*reg(regs, inst.b) as i64) >= (*reg(regs, inst.c) as i64)) as u64;
                        pc = pc.add(1);
                    }
                    OpCode::IULt => {
                        *reg(regs, inst.a) = (*reg(regs, inst.b) < *reg(regs, inst.c)) as u64;
                        pc = pc.add(1);
                    }
                    OpCode::IULe => {
                        *reg(regs, inst.a) = (*reg(regs, inst.b) <= *reg(regs, inst.c)) as u64;
                        pc = pc.add(1);
                    }
                    OpCode::IUGt => {
                        *reg(regs, inst.a) = (*reg(regs, inst.b) > *reg(regs, inst.c)) as u64;
                        pc = pc.add(1);
                    }
                    OpCode::IUGe => {
                        *reg(regs, inst.a) = (*reg(regs, inst.b) >= *reg(regs, inst.c)) as u64;
                        pc = pc.add(1);
                    }

                    OpCode::FEq => {
                        *reg(regs, inst.a) = (f64::from_bits(*reg(regs, inst.b))
                            == f64::from_bits(*reg(regs, inst.c)))
                            as u64;
                        pc = pc.add(1);
                    }
                    OpCode::FNe => {
                        *reg(regs, inst.a) = (f64::from_bits(*reg(regs, inst.b))
                            != f64::from_bits(*reg(regs, inst.c)))
                            as u64;
                        pc = pc.add(1);
                    }
                    OpCode::FLt => {
                        *reg(regs, inst.a) = (f64::from_bits(*reg(regs, inst.b))
                            < f64::from_bits(*reg(regs, inst.c)))
                            as u64;
                        pc = pc.add(1);
                    }
                    OpCode::FLe => {
                        *reg(regs, inst.a) = (f64::from_bits(*reg(regs, inst.b))
                            <= f64::from_bits(*reg(regs, inst.c)))
                            as u64;
                        pc = pc.add(1);
                    }
                    OpCode::FGt => {
                        *reg(regs, inst.a) = (f64::from_bits(*reg(regs, inst.b))
                            > f64::from_bits(*reg(regs, inst.c)))
                            as u64;
                        pc = pc.add(1);
                    }
                    OpCode::FGe => {
                        *reg(regs, inst.a) = (f64::from_bits(*reg(regs, inst.b))
                            >= f64::from_bits(*reg(regs, inst.c)))
                            as u64;
                        pc = pc.add(1);
                    }

                    OpCode::Select => {
                        *reg(regs, inst.a) = if *reg(regs, inst.a) != 0 {
                            *reg(regs, inst.b)
                        } else {
                            *reg(regs, inst.c)
                        };
                        pc = pc.add(1);
                    }

                    OpCode::And => {
                        *reg(regs, inst.a) = *reg(regs, inst.b) & *reg(regs, inst.c);
                        pc = pc.add(1);
                    }
                    OpCode::Or => {
                        *reg(regs, inst.a) = *reg(regs, inst.b) | *reg(regs, inst.c);
                        pc = pc.add(1);
                    }
                    OpCode::Xor => {
                        *reg(regs, inst.a) = *reg(regs, inst.b) ^ *reg(regs, inst.c);
                        pc = pc.add(1);
                    }
                    OpCode::Not => {
                        *reg(regs, inst.a) = !*reg(regs, inst.b);
                        pc = pc.add(1);
                    }
                    OpCode::Shl => {
                        *reg(regs, inst.a) =
                            (*reg(regs, inst.b)).wrapping_shl(*reg(regs, inst.c) as u32);
                        pc = pc.add(1);
                    }
                    OpCode::Shr => {
                        *reg(regs, inst.a) =
                            (*reg(regs, inst.b)).wrapping_shr(*reg(regs, inst.c) as u32);
                        pc = pc.add(1);
                    }
                    OpCode::Sar => {
                        *reg(regs, inst.a) = ((*reg(regs, inst.b) as i64)
                            .wrapping_shr(*reg(regs, inst.c) as u32))
                            as u64;
                        pc = pc.add(1);
                    }

                    OpCode::Jmp => pc = code_base.add(*reg(regs, inst.a) as usize),
                    OpCode::Jnz => {
                        pc = if *reg(regs, inst.a) != 0 {
                            code_base.add(*reg(regs, inst.b) as usize)
                        } else {
                            pc.add(1)
                        };
                    }
                    OpCode::Jz => {
                        pc = if *reg(regs, inst.a) == 0 {
                            code_base.add(*reg(regs, inst.b) as usize)
                        } else {
                            pc.add(1)
                        };
                    }
                    OpCode::Call => {
                        let target_pc = code_base.add(*reg(regs, inst.a) as usize);
                        stack.add(call_depth).write(MaybeUninit::new(OrcaCallFrame {
                            registers: regs.cast::<[u64; REGISTER_COUNT]>().read(),
                            return_pc: pc.add(1),
                        }));
                        call_depth += 1;

                        clear_after_args(regs, inst.b as usize);
                        pc = target_pc;
                    }
                    OpCode::Ret => {
                        let return_value = *regs;
                        if call_depth == 0 {
                            break;
                        }

                        call_depth -= 1;
                        let frame = stack.add(call_depth).read().assume_init();
                        regs.cast::<[u64; REGISTER_COUNT]>().write(frame.registers);
                        *regs = return_value;
                        pc = frame.return_pc;
                    }

                    OpCode::F2I => {
                        *reg(regs, inst.a) = f64::from_bits(*reg(regs, inst.b)) as i64 as u64;
                        pc = pc.add(1);
                    }
                    OpCode::I2F => {
                        *reg(regs, inst.a) = ((*reg(regs, inst.b) as i64) as f64).to_bits();
                        pc = pc.add(1);
                    }
                    OpCode::F2U => {
                        *reg(regs, inst.a) = f64::from_bits(*reg(regs, inst.b)) as u64;
                        pc = pc.add(1);
                    }
                    OpCode::U2F => {
                        *reg(regs, inst.a) = (*reg(regs, inst.b) as f64).to_bits();
                        pc = pc.add(1);
                    }

                    OpCode::PrintStr => {
                        print_str_cold(const_base, *reg(regs, inst.a) as usize);
                        pc = pc.add(1);
                    }
                    OpCode::PrintInt => {
                        print_int_cold(*reg(regs, inst.a));
                        pc = pc.add(1);
                    }
                    OpCode::PrintUInt => {
                        print_uint_cold(*reg(regs, inst.a));
                        pc = pc.add(1);
                    }
                    OpCode::PrintFloat => {
                        print_float_cold(*reg(regs, inst.a));
                        pc = pc.add(1);
                    }
                }
            }

            self.call_depth = call_depth;
        }
    }
}

#[inline(always)]
fn imm16(inst: Instruction) -> u16 {
    ((inst.c as u16) << 8) | inst.b as u16
}

#[inline(always)]
unsafe fn reg(regs: *mut u64, index: u8) -> *mut u64 {
    unsafe { regs.add(index as usize) }
}

#[inline(always)]
unsafe fn load_u64(base: *const u8, offset: usize) -> u64 {
    unsafe { base.add(offset).cast::<u64>().read_unaligned() }
}

#[inline(always)]
unsafe fn clear_after_args(regs: *mut u64, arg_count: usize) {
    unsafe {
        *regs = 0;
        let start = arg_count + 1;
        if start < REGISTER_COUNT {
            std::ptr::write_bytes(regs.add(start), 0, REGISTER_COUNT - start);
        }
    }
}

#[cold]
#[inline(never)]
unsafe fn print_str_cold(const_base: *const u8, index: usize) {
    unsafe {
        let len = load_u64(const_base, index) as usize;
        let bytes = std::slice::from_raw_parts(const_base.add(index + 8), len);
        print!("{}", std::str::from_utf8_unchecked(bytes));
    }
}

#[cold]
#[inline(never)]
fn print_int_cold(value: u64) {
    println!("{}", value as i64);
}

#[cold]
#[inline(never)]
fn print_uint_cold(value: u64) {
    println!("{value}");
}

#[cold]
#[inline(never)]
fn print_float_cold(value: u64) {
    println!("{}", f64::from_bits(value));
}

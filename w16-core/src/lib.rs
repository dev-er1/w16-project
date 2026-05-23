pub mod bytecode;
pub mod interpreter;
pub mod jit;

use std::fmt;

pub use crate::bytecode::{Bytecode, ConstantPool, Instruction, OpCode};
pub use crate::interpreter::vm::{REGISTER_COUNT, VM};
use crate::jit::jit_compiler::{JIT, JitError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VMError {
    InvalidOpCode(u8),
    MemoryAccessViolation(usize),
    ConstantPoolError,
    DivisionByZero,
}

impl fmt::Display for VMError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VMError::InvalidOpCode(op) => write!(f, "invalid OpCode: {op:#04X}"),
            VMError::MemoryAccessViolation(addr) => {
                write!(f, "memory access violation at address: {addr:#X}")
            }
            VMError::ConstantPoolError => write!(f, "failed to read from Constant Pool"),
            VMError::DivisionByZero => write!(f, "integer division by zero"),
        }
    }
}

impl std::error::Error for VMError {}

/// Версии для CLI
/// Версия виртуальной машины
pub const VM_VERSION: &str = "0.1.4";
/// Версия JIT-компилятора
pub const JIT_VERSION: &str = "0.1.4";

#[inline]
pub fn run(bytecode: &Bytecode, memory_size: usize) -> Result<[u64; REGISTER_COUNT], VMError> {
    let mut vm = VM::new(memory_size);
    vm.run(bytecode)?;
    Ok(vm.registers)
}

#[inline]
pub fn run_by_jit(bytecode: &Bytecode) -> Result<[u64; REGISTER_COUNT], JitError> {
    let mut jit = JIT::new();
    let code_ptr = jit.try_compile(bytecode)?;

    let mut registers = [0u64; REGISTER_COUNT];
    let mut memory = vec![0u8; 1024 * 1024];

    let code_fn: unsafe extern "C" fn(*mut u64, *mut u8, usize, *const u8, usize) =
        unsafe { std::mem::transmute(code_ptr) };

    unsafe {
        code_fn(
            registers.as_mut_ptr(),
            memory.as_mut_ptr(),
            memory.len(),
            bytecode.constant_pool.data.as_ptr(),
            bytecode.constant_pool.data.len(),
        );
    }

    Ok(registers)
}

#[macro_export]
macro_rules! inst {
    ($op:expr, $a:expr, $b:expr, $c:expr) => {
        $crate::bytecode::Instruction {
            opcode: $op,
            a: $a as u8,
            b: $b as u8,
            c: $c as u8,
        }
    };
}

#[macro_export]
macro_rules! inst_imm {
    ($op:expr, $a:expr, $imm:expr) => {{
        let val = $imm as u16;
        $crate::bytecode::Instruction {
            opcode: $op,
            a: $a as u8,
            b: (val & 0xFF) as u8,
            c: (val >> 8) as u8,
        }
    }};
}

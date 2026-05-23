//! w16-core\tests\vm_tests\helpers.rs
//!
//! Вспомогательные утилиты для тестов VM/JIT.
//! Подключается через `#[path = "helpers.rs"] mod helpers;` в каждом тест-файле.

use w16_core::{
    REGISTER_COUNT, VMError,
    bytecode::{Bytecode, ConstantPool, Instruction, OpCode},
};

// =============================================================================
// СТРОИТЕЛИ БАЙТКОДА
// =============================================================================

/// Строитель программы для тестов.
/// Позволяет цепочкой добавлять инструкции и запускать VM.
pub struct Program {
    instructions: Vec<Instruction>,
    pool: ConstantPool,
}

impl Program {
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
            pool: ConstantPool::new(),
        }
    }

    /// Добавляет инструкцию.
    pub fn inst(mut self, op: OpCode, a: u8, b: u8, c: u8) -> Self {
        self.instructions.push(Instruction {
            opcode: op,
            a,
            b,
            c,
        });
        self
    }

    /// Добавляет Load8: reg = imm8.
    pub fn load8(self, reg: u8, val: u8) -> Self {
        self.inst(OpCode::Load8, reg, 0, val)
    }

    /// Добавляет Load16: reg = imm16. b=lo, c=hi.
    pub fn load16(self, reg: u8, val: u16) -> Self {
        let lo = (val & 0xFF) as u8;
        let hi = (val >> 8) as u8;
        self.inst(OpCode::Load16, reg, lo, hi)
    }

    /// Добавляет LoadConst: reg = pool[byte_offset].
    pub fn load_const(self, reg: u8, byte_offset: u16) -> Self {
        let lo = (byte_offset & 0xFF) as u8;
        let hi = (byte_offset >> 8) as u8;
        self.inst(OpCode::LoadConst, reg, lo, hi)
    }

    /// Добавляет u64 в пул констант, возвращает байтовый offset.
    pub fn add_u64(mut self, val: u64) -> (Self, u16) {
        let offset = self.pool.data.len() as u16;
        self.pool.data.extend_from_slice(&val.to_le_bytes());
        (self, offset)
    }

    /// Добавляет f64 в пул констант, возвращает байтовый offset.
    pub fn add_f64(mut self, val: f64) -> (Self, u16) {
        let offset = self.pool.data.len() as u16;
        self.pool
            .data
            .extend_from_slice(&val.to_bits().to_le_bytes());
        (self, offset)
    }

    /// Добавляет Halt в конец и возвращает Bytecode.
    pub fn halt(self) -> Bytecode {
        let s = self.inst(OpCode::Halt, 0, 0, 0);
        Bytecode::new(s.instructions, s.pool)
    }

    /// Возвращает Bytecode без Halt (для тестов где Halt уже добавлен вручную).
    pub fn build(self) -> Bytecode {
        Bytecode::new(self.instructions, self.pool)
    }
}

// =============================================================================
// ЗАПУСК И ПРОВЕРКА
// =============================================================================

/// Запускает программу и возвращает массив регистров.
pub fn run(bytecode: &Bytecode) -> [u64; REGISTER_COUNT] {
    w16_core::run(bytecode, 1024 * 1024).expect("VM error")
}

/// Запускает программу с заданным размером памяти.
pub fn run_mem(bytecode: &Bytecode, mem: usize) -> [u64; REGISTER_COUNT] {
    w16_core::run(bytecode, mem).expect("VM error")
}

/// Запускает программу и ожидает ошибку.
pub fn run_err(bytecode: &Bytecode) -> VMError {
    w16_core::run(bytecode, 1024 * 1024).expect_err("Expected VM error")
}

/// Запускает программу с маленькой памятью и ожидает ошибку.
pub fn run_err_mem(bytecode: &Bytecode, mem: usize) -> VMError {
    w16_core::run(bytecode, mem).expect_err("Expected VM error")
}

// =============================================================================
// МАКРОСЫ
// =============================================================================

/// Проверяет что регистр содержит ожидаемое значение u64.
#[macro_export]
macro_rules! assert_reg {
    ($regs:expr, $reg:expr, $expected:expr) => {
        assert_eq!(
            $regs[$reg as usize], $expected as u64,
            "r{} expected {}, got {}",
            $reg, $expected, $regs[$reg as usize]
        )
    };
}

/// Проверяет что регистр содержит ожидаемое значение f64.
#[macro_export]
macro_rules! assert_reg_f64 {
    ($regs:expr, $reg:expr, $expected:expr) => {{
        let got = f64::from_bits($regs[$reg as usize]);
        let exp = $expected as f64;
        assert!(
            (got - exp).abs() < 1e-9 || (got.is_nan() && exp.is_nan()),
            "r{} expected {}, got {}",
            $reg,
            exp,
            got
        )
    }};
}

/// Проверяет тип VMError.
#[macro_export]
macro_rules! assert_vm_error {
    ($err:expr, VMError::DivisionByZero) => {
        assert!(
            matches!($err, VMError::DivisionByZero),
            "Expected DivisionByZero, got {:?}",
            $err
        )
    };
    ($err:expr, VMError::MemoryAccessViolation) => {
        assert!(
            matches!($err, VMError::MemoryAccessViolation(_)),
            "Expected MemoryAccessViolation, got {:?}",
            $err
        )
    };
    ($err:expr, VMError::ConstantPoolError) => {
        assert!(
            matches!($err, VMError::ConstantPoolError),
            "Expected ConstantPoolError, got {:?}",
            $err
        )
    };
}

// w16-core\tests\jit_tests\main_jit.rs
//
//! Полное покрытие JIT-компилятора.
//!
//! Особенности JIT относительно VM:
//! - Переходы только по константным регистрам (Load8/Load16 -> Jmp/Jnz/Jz)
//! - Нет Call/Ret (UnsupportedOpcode или DynamicJumpTarget)
//! - Выход за границы памяти -> 0 (не VMError, JIT продолжает)
//! - Все 16 сравнений поддерживаются

use w16_core::REGISTER_COUNT;
use w16_core::bytecode::{Bytecode, ConstantPool, Instruction, OpCode};
use w16_core::jit::jit_compiler::JIT;

struct Prog {
    instructions: Vec<Instruction>,
    pool: ConstantPool,
}

impl Prog {
    fn new() -> Self {
        Self {
            instructions: Vec::new(),
            pool: ConstantPool::new(),
        }
    }
    fn inst(mut self, op: OpCode, a: u8, b: u8, c: u8) -> Self {
        self.instructions.push(Instruction {
            opcode: op,
            a,
            b,
            c,
        });
        self
    }
    fn load8(self, reg: u8, val: u8) -> Self {
        self.inst(OpCode::Load8, reg, 0, val)
    }
    fn load16(self, reg: u8, val: u16) -> Self {
        let lo = (val & 0xFF) as u8;
        let hi = (val >> 8) as u8;
        self.inst(OpCode::Load16, reg, lo, hi)
    }
    fn add_u64(&mut self, val: u64) -> u16 {
        let off = self.pool.data.len() as u16;
        self.pool.data.extend_from_slice(&val.to_le_bytes());
        off
    }
    fn add_f64(&mut self, val: f64) -> u16 {
        let off = self.pool.data.len() as u16;
        self.pool
            .data
            .extend_from_slice(&val.to_bits().to_le_bytes());
        off
    }
    fn load_const(self, reg: u8, off: u16) -> Self {
        let lo = (off & 0xFF) as u8;
        let hi = (off >> 8) as u8;
        self.inst(OpCode::LoadConst, reg, lo, hi)
    }
    fn halt(self) -> Bytecode {
        let s = self.inst(OpCode::Halt, 0, 0, 0);
        Bytecode::new(s.instructions, s.pool)
    }
    fn build(self) -> Bytecode {
        Bytecode::new(self.instructions, self.pool)
    }
}

fn jit_run(bc: &Bytecode) -> [u64; REGISTER_COUNT] {
    let mut jit = JIT::new();
    let mem_size = 1024 * 1024usize;
    let mut regs = [0u64; REGISTER_COUNT];
    let mut memory = vec![0u8; mem_size];
    let fn_ptr = jit.compile(bc);
    unsafe {
        let f: unsafe extern "C" fn(*mut u64, *mut u8, usize, *const u8, usize) =
            std::mem::transmute(fn_ptr);
        f(
            regs.as_mut_ptr(),
            memory.as_mut_ptr(),
            mem_size,
            bc.constant_pool.data.as_ptr(),
            bc.constant_pool.data.len(),
        );
    }
    regs
}

fn jit_run_mem(bc: &Bytecode, mem_size: usize) -> (Vec<u8>, [u64; REGISTER_COUNT]) {
    let mut jit = JIT::new();
    let mut regs = [0u64; REGISTER_COUNT];
    let mut memory = vec![0u8; mem_size];
    let fn_ptr = jit.compile(bc);
    unsafe {
        let f: unsafe extern "C" fn(*mut u64, *mut u8, usize, *const u8, usize) =
            std::mem::transmute(fn_ptr);
        f(
            regs.as_mut_ptr(),
            memory.as_mut_ptr(),
            mem_size,
            bc.constant_pool.data.as_ptr(),
            bc.constant_pool.data.len(),
        );
    }
    (memory, regs)
}

fn jit_try(bc: &Bytecode) -> Result<*const u8, w16_core::jit::jit_compiler::JitError> {
    let mut jit = JIT::new();
    jit.try_compile(bc)
}

macro_rules! assert_reg {
    ($regs:expr, $r:expr, $v:expr) => {
        assert_eq!(
            $regs[$r as usize], $v as u64,
            "r{} = {}, expected {}",
            $r, $regs[$r as usize], $v as u64
        )
    };
}
macro_rules! assert_f64 {
    ($regs:expr, $r:expr, $v:expr) => {{
        let got = f64::from_bits($regs[$r as usize]);
        assert!(
            (got - ($v as f64)).abs() < 1e-9 || (got.is_nan() && ($v as f64).is_nan()),
            "r{} = {got}, expected {}",
            $r,
            $v as f64
        );
    }};
}

// =============================================================================
// LOAD / MOV
// =============================================================================

#[test]
fn jit_load8_zero() {
    let bc = Prog::new().load8(0, 0).halt();
    assert_reg!(jit_run(&bc), 0, 0);
}
#[test]
fn jit_load8_max() {
    let bc = Prog::new().load8(0, 255).halt();
    assert_reg!(jit_run(&bc), 0, 255);
}
#[test]
fn jit_load8_r255() {
    let bc = Prog::new().load8(255, 42).halt();
    assert_eq!(jit_run(&bc)[255], 42);
}
#[test]
fn jit_load16_basic() {
    let bc = Prog::new().load16(0, 1000).halt();
    assert_reg!(jit_run(&bc), 0, 1000);
}
#[test]
fn jit_load16_max() {
    let bc = Prog::new().load16(0, 0xFFFF).halt();
    assert_reg!(jit_run(&bc), 0, 0xFFFF);
}
#[test]
fn jit_load16_256() {
    let bc = Prog::new().load16(0, 256).halt();
    assert_reg!(jit_run(&bc), 0, 256);
}

#[test]
fn jit_load_const_u64() {
    let mut p = Prog::new();
    let off = p.add_u64(u64::MAX);
    let bc = p.load_const(0, off).halt();
    assert_reg!(jit_run(&bc), 0, u64::MAX);
}
#[test]
fn jit_load_const_f64() {
    let mut p = Prog::new();
    let off = p.add_f64(3.14);
    let bc = p.load_const(0, off).halt();
    assert_f64!(jit_run(&bc), 0, 3.14);
}
#[test]
fn jit_load_const_zero() {
    let mut p = Prog::new();
    let off = p.add_u64(0);
    let bc = p.load_const(0, off).halt();
    assert_reg!(jit_run(&bc), 0, 0);
}
#[test]
fn jit_load_const_multiple() {
    let mut p = Prog::new();
    let o1 = p.add_u64(111);
    let o2 = p.add_u64(222);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::Add, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 333);
}

#[test]
fn jit_mov_basic() {
    let bc = Prog::new().load8(1, 42).inst(OpCode::Mov, 0, 1, 0).halt();
    assert_reg!(jit_run(&bc), 0, 42);
}
#[test]
fn jit_mov_chain() {
    let bc = Prog::new()
        .load8(1, 99)
        .inst(OpCode::Mov, 2, 1, 0)
        .inst(OpCode::Mov, 3, 2, 0)
        .halt();
    let r = jit_run(&bc);
    assert_reg!(r, 2, 99);
    assert_reg!(r, 3, 99);
}
#[test]
fn jit_all_regs_default_zero() {
    let bc = Prog::new().halt();
    let r = jit_run(&bc);
    assert_eq!(r[0], 0);
    assert_eq!(r[128], 0);
    assert_eq!(r[255], 0);
}

// =============================================================================
// INTEGER ARITHMETIC
// =============================================================================

#[test]
fn jit_add_basic() {
    let bc = Prog::new()
        .load8(1, 10)
        .load8(2, 32)
        .inst(OpCode::Add, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 42);
}
#[test]
fn jit_add_zero() {
    let bc = Prog::new()
        .load8(1, 42)
        .load8(2, 0)
        .inst(OpCode::Add, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 42);
}
#[test]
fn jit_add_wrap() {
    let mut p = Prog::new();
    let o = p.add_u64(u64::MAX);
    let bc = p
        .load_const(1, o)
        .load8(2, 1)
        .inst(OpCode::Add, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 0); // wrapping
}
#[test]
fn jit_sub_basic() {
    let bc = Prog::new()
        .load8(1, 50)
        .load8(2, 8)
        .inst(OpCode::Sub, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 42);
}
#[test]
fn jit_sub_to_zero() {
    let bc = Prog::new()
        .load8(1, 42)
        .load8(2, 42)
        .inst(OpCode::Sub, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 0);
}
#[test]
fn jit_sub_wrap() {
    let bc = Prog::new()
        .load8(1, 0)
        .load8(2, 1)
        .inst(OpCode::Sub, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, u64::MAX);
}
#[test]
fn jit_mul_basic() {
    let bc = Prog::new()
        .load8(1, 6)
        .load8(2, 7)
        .inst(OpCode::Mul, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 42);
}
#[test]
fn jit_mul_by_zero() {
    let bc = Prog::new()
        .load8(1, 100)
        .load8(2, 0)
        .inst(OpCode::Mul, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 0);
}
#[test]
fn jit_mul_by_one() {
    let bc = Prog::new()
        .load8(1, 42)
        .load8(2, 1)
        .inst(OpCode::Mul, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 42);
}
#[test]
fn jit_udiv_basic() {
    let bc = Prog::new()
        .load8(1, 84)
        .load8(2, 2)
        .inst(OpCode::UDiv, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 42);
}
#[test]
fn jit_udiv_by_zero_returns_zero() {
    // JIT возвращает 0 при делении на ноль (не ошибку)
    let bc = Prog::new()
        .load8(1, 10)
        .load8(2, 0)
        .inst(OpCode::UDiv, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 0);
}
#[test]
fn jit_idiv_negative() {
    let mut p = Prog::new();
    let o = p.add_u64((-42i64) as u64);
    let bc = p
        .load_const(1, o)
        .load8(2, 6)
        .inst(OpCode::IDiv, 0, 1, 2)
        .halt();
    assert_eq!(jit_run(&bc)[0] as i64, -7);
}
#[test]
fn jit_idiv_by_zero_returns_zero() {
    let bc = Prog::new()
        .load8(1, 42)
        .load8(2, 0)
        .inst(OpCode::IDiv, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 0);
}
#[test]
fn jit_urem_basic() {
    let bc = Prog::new()
        .load8(1, 17)
        .load8(2, 5)
        .inst(OpCode::URem, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 2);
}
#[test]
fn jit_urem_by_zero_returns_zero() {
    let bc = Prog::new()
        .load8(1, 17)
        .load8(2, 0)
        .inst(OpCode::URem, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 0);
}
#[test]
fn jit_irem_negative() {
    let mut p = Prog::new();
    let o = p.add_u64((-17i64) as u64);
    let bc = p
        .load_const(1, o)
        .load8(2, 5)
        .inst(OpCode::IRem, 0, 1, 2)
        .halt();
    assert_eq!(jit_run(&bc)[0] as i64, -2);
}
#[test]
fn jit_neg_positive() {
    let bc = Prog::new().load8(1, 42).inst(OpCode::Neg, 0, 1, 0).halt();
    assert_eq!(jit_run(&bc)[0] as i64, -42);
}
#[test]
fn jit_neg_negative() {
    let mut p = Prog::new();
    let o = p.add_u64((-42i64) as u64);
    let bc = p.load_const(1, o).inst(OpCode::Neg, 0, 1, 0).halt();
    assert_eq!(jit_run(&bc)[0] as i64, 42);
}
#[test]
fn jit_neg_zero() {
    let bc = Prog::new().load8(1, 0).inst(OpCode::Neg, 0, 1, 0).halt();
    assert_reg!(jit_run(&bc), 0, 0);
}

// =============================================================================
// FLOAT ARITHMETIC
// =============================================================================

#[test]
fn jit_fadd() {
    let mut p = Prog::new();
    let o1 = p.add_f64(1.5);
    let o2 = p.add_f64(2.5);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::FAdd, 0, 1, 2)
        .halt();
    assert_f64!(jit_run(&bc), 0, 4.0);
}
#[test]
fn jit_fsub() {
    let mut p = Prog::new();
    let o1 = p.add_f64(10.0);
    let o2 = p.add_f64(3.5);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::FSub, 0, 1, 2)
        .halt();
    assert_f64!(jit_run(&bc), 0, 6.5);
}
#[test]
fn jit_fmul() {
    let mut p = Prog::new();
    let o1 = p.add_f64(3.0);
    let o2 = p.add_f64(14.0);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::FMul, 0, 1, 2)
        .halt();
    assert_f64!(jit_run(&bc), 0, 42.0);
}
#[test]
fn jit_fdiv() {
    let mut p = Prog::new();
    let o1 = p.add_f64(84.0);
    let o2 = p.add_f64(2.0);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::FDiv, 0, 1, 2)
        .halt();
    assert_f64!(jit_run(&bc), 0, 42.0);
}
#[test]
fn jit_fdiv_by_zero_inf() {
    let mut p = Prog::new();
    let o1 = p.add_f64(1.0);
    let o2 = p.add_f64(0.0);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::FDiv, 0, 1, 2)
        .halt();
    assert!(f64::from_bits(jit_run(&bc)[0]).is_infinite());
}
#[test]
fn jit_frem() {
    let mut p = Prog::new();
    let o1 = p.add_f64(10.5);
    let o2 = p.add_f64(3.0);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::FRem, 0, 1, 2)
        .halt();
    let got = f64::from_bits(jit_run(&bc)[0]);
    assert!((got - 1.5).abs() < 1e-10);
}
#[test]
fn jit_fneg() {
    let mut p = Prog::new();
    let o = p.add_f64(42.0);
    let bc = p.load_const(1, o).inst(OpCode::FNeg, 0, 1, 0).halt();
    assert_f64!(jit_run(&bc), 0, -42.0);
}
#[test]
fn jit_fabs_negative() {
    let mut p = Prog::new();
    let o = p.add_f64(-42.0);
    let bc = p.load_const(1, o).inst(OpCode::FAbs, 0, 1, 0).halt();
    assert_f64!(jit_run(&bc), 0, 42.0);
}
#[test]
fn jit_fabs_positive() {
    let mut p = Prog::new();
    let o = p.add_f64(42.0);
    let bc = p.load_const(1, o).inst(OpCode::FAbs, 0, 1, 0).halt();
    assert_f64!(jit_run(&bc), 0, 42.0);
}
#[test]
fn jit_fadd_nan_propagates() {
    let mut p = Prog::new();
    let o1 = p.add_f64(f64::NAN);
    let o2 = p.add_f64(1.0);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::FAdd, 0, 1, 2)
        .halt();
    assert!(f64::from_bits(jit_run(&bc)[0]).is_nan());
}
#[test]
fn jit_fadd_inf() {
    let mut p = Prog::new();
    let o1 = p.add_f64(f64::INFINITY);
    let o2 = p.add_f64(1.0);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::FAdd, 0, 1, 2)
        .halt();
    assert_eq!(f64::from_bits(jit_run(&bc)[0]), f64::INFINITY);
}

// =============================================================================
// CAST
// =============================================================================

#[test]
fn jit_i2f() {
    let bc = Prog::new().load8(1, 42).inst(OpCode::I2F, 0, 1, 0).halt();
    assert_f64!(jit_run(&bc), 0, 42.0);
}
#[test]
fn jit_f2i() {
    let mut p = Prog::new();
    let o = p.add_f64(42.7);
    let bc = p.load_const(1, o).inst(OpCode::F2I, 0, 1, 0).halt();
    assert_eq!(jit_run(&bc)[0] as i64, 42);
}
#[test]
fn jit_f2i_negative() {
    let mut p = Prog::new();
    let o = p.add_f64(-7.9);
    let bc = p.load_const(1, o).inst(OpCode::F2I, 0, 1, 0).halt();
    assert_eq!(jit_run(&bc)[0] as i64, -7);
}
#[test]
fn jit_u2f() {
    let bc = Prog::new().load8(1, 100).inst(OpCode::U2F, 0, 1, 0).halt();
    assert_f64!(jit_run(&bc), 0, 100.0);
}
#[test]
fn jit_f2u() {
    let mut p = Prog::new();
    let o = p.add_f64(42.9);
    let bc = p.load_const(1, o).inst(OpCode::F2U, 0, 1, 0).halt();
    assert_reg!(jit_run(&bc), 0, 42u64);
}
#[test]
fn jit_roundtrip_i2f_f2i() {
    let bc = Prog::new()
        .load8(1, 99)
        .inst(OpCode::I2F, 2, 1, 0)
        .inst(OpCode::F2I, 0, 2, 0)
        .halt();
    assert_reg!(jit_run(&bc), 0, 99);
}

// =============================================================================
// BITWISE
// =============================================================================

#[test]
fn jit_and() {
    let bc = Prog::new()
        .load8(1, 0b1100)
        .load8(2, 0b1010)
        .inst(OpCode::And, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 0b1000);
}
#[test]
fn jit_or() {
    let bc = Prog::new()
        .load8(1, 0b1100)
        .load8(2, 0b1010)
        .inst(OpCode::Or, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 0b1110);
}
#[test]
fn jit_xor() {
    let bc = Prog::new()
        .load8(1, 0b1100)
        .load8(2, 0b1010)
        .inst(OpCode::Xor, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 0b0110);
}
#[test]
fn jit_not() {
    let bc = Prog::new().load8(1, 0).inst(OpCode::Not, 0, 1, 0).halt();
    assert_reg!(jit_run(&bc), 0, u64::MAX);
}
#[test]
fn jit_shl() {
    let bc = Prog::new()
        .load8(1, 1)
        .load8(2, 10)
        .inst(OpCode::Shl, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 1024);
}
#[test]
fn jit_shr() {
    let bc = Prog::new()
        .load8(1, 128)
        .load8(2, 3)
        .inst(OpCode::Shr, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 16);
}
#[test]
fn jit_sar() {
    let mut p = Prog::new();
    let o = p.add_u64((-8i64) as u64);
    let bc = p
        .load_const(1, o)
        .load8(2, 1)
        .inst(OpCode::Sar, 0, 1, 2)
        .halt();
    assert_eq!(jit_run(&bc)[0] as i64, -4);
}
#[test]
fn jit_xor_self_zero() {
    let bc = Prog::new().load8(1, 0xFF).inst(OpCode::Xor, 0, 1, 1).halt();
    assert_reg!(jit_run(&bc), 0, 0);
}
#[test]
fn jit_and_mask() {
    let mut p = Prog::new();
    let o1 = p.add_u64(0xDEADBEEF);
    let o2 = p.add_u64(0x0000FFFF);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::And, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 0x0000BEEF);
}

// =============================================================================
// COMPARISONS — все 16
// =============================================================================

#[test]
fn jit_ieq_true() {
    let bc = Prog::new()
        .load8(1, 5)
        .load8(2, 5)
        .inst(OpCode::IEq, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 1);
}
#[test]
fn jit_ieq_false() {
    let bc = Prog::new()
        .load8(1, 5)
        .load8(2, 6)
        .inst(OpCode::IEq, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 0);
}
#[test]
fn jit_ine_true() {
    let bc = Prog::new()
        .load8(1, 5)
        .load8(2, 6)
        .inst(OpCode::INe, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 1);
}
#[test]
fn jit_ine_false() {
    let bc = Prog::new()
        .load8(1, 5)
        .load8(2, 5)
        .inst(OpCode::INe, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 0);
}
#[test]
fn jit_islt_true() {
    let bc = Prog::new()
        .load8(1, 3)
        .load8(2, 5)
        .inst(OpCode::ISLt, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 1);
}
#[test]
fn jit_islt_false() {
    let bc = Prog::new()
        .load8(1, 5)
        .load8(2, 3)
        .inst(OpCode::ISLt, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 0);
}
#[test]
fn jit_isle_eq() {
    let bc = Prog::new()
        .load8(1, 5)
        .load8(2, 5)
        .inst(OpCode::ISLe, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 1);
}
#[test]
fn jit_isle_lt() {
    let bc = Prog::new()
        .load8(1, 3)
        .load8(2, 5)
        .inst(OpCode::ISLe, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 1);
}
#[test]
fn jit_isle_false() {
    let bc = Prog::new()
        .load8(1, 6)
        .load8(2, 5)
        .inst(OpCode::ISLe, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 0);
}
#[test]
fn jit_isgt_true() {
    let bc = Prog::new()
        .load8(1, 10)
        .load8(2, 5)
        .inst(OpCode::ISGt, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 1);
}
#[test]
fn jit_isgt_false() {
    let bc = Prog::new()
        .load8(1, 5)
        .load8(2, 10)
        .inst(OpCode::ISGt, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 0);
}
#[test]
fn jit_isge_eq() {
    let bc = Prog::new()
        .load8(1, 5)
        .load8(2, 5)
        .inst(OpCode::ISGe, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 1);
}
#[test]
fn jit_isge_gt() {
    let bc = Prog::new()
        .load8(1, 6)
        .load8(2, 5)
        .inst(OpCode::ISGe, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 1);
}
#[test]
fn jit_iult_true() {
    let bc = Prog::new()
        .load8(1, 3)
        .load8(2, 5)
        .inst(OpCode::IULt, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 1);
}
#[test]
fn jit_iult_false() {
    let bc = Prog::new()
        .load8(1, 5)
        .load8(2, 3)
        .inst(OpCode::IULt, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 0);
}
#[test]
fn jit_iule_eq() {
    let bc = Prog::new()
        .load8(1, 5)
        .load8(2, 5)
        .inst(OpCode::IULe, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 1);
}
#[test]
fn jit_iule_lt() {
    let bc = Prog::new()
        .load8(1, 3)
        .load8(2, 5)
        .inst(OpCode::IULe, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 1);
}
#[test]
fn jit_iule_false() {
    let bc = Prog::new()
        .load8(1, 6)
        .load8(2, 5)
        .inst(OpCode::IULe, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 0);
}
#[test]
fn jit_iugt_true() {
    let bc = Prog::new()
        .load8(1, 5)
        .load8(2, 3)
        .inst(OpCode::IUGt, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 1);
}
#[test]
fn jit_iugt_false() {
    let bc = Prog::new()
        .load8(1, 3)
        .load8(2, 5)
        .inst(OpCode::IUGt, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 0);
}
#[test]
fn jit_iuge_eq() {
    let bc = Prog::new()
        .load8(1, 5)
        .load8(2, 5)
        .inst(OpCode::IUGe, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 1);
}
#[test]
fn jit_iuge_gt() {
    let bc = Prog::new()
        .load8(1, 6)
        .load8(2, 5)
        .inst(OpCode::IUGe, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 1);
}
#[test]
fn jit_iuge_false() {
    let bc = Prog::new()
        .load8(1, 3)
        .load8(2, 5)
        .inst(OpCode::IUGe, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 0);
}
#[test]
fn jit_feq_true() {
    let mut p = Prog::new();
    let o1 = p.add_f64(1.0);
    let o2 = p.add_f64(1.0);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::FEq, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 1);
}
#[test]
fn jit_feq_false() {
    let mut p = Prog::new();
    let o1 = p.add_f64(1.0);
    let o2 = p.add_f64(2.0);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::FEq, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 0);
}
#[test]
fn jit_flt_true() {
    let mut p = Prog::new();
    let o1 = p.add_f64(1.0);
    let o2 = p.add_f64(2.0);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::FLt, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 1);
}
#[test]
fn jit_fgt_true() {
    let mut p = Prog::new();
    let o1 = p.add_f64(2.0);
    let o2 = p.add_f64(1.0);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::FGt, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 1);
}
#[test]
fn jit_fle_eq() {
    let mut p = Prog::new();
    let o1 = p.add_f64(1.0);
    let o2 = p.add_f64(1.0);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::FLe, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 1);
}
#[test]
fn jit_fge_eq() {
    let mut p = Prog::new();
    let o1 = p.add_f64(1.0);
    let o2 = p.add_f64(1.0);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::FGe, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 1);
}
#[test]
fn jit_fne_true() {
    let mut p = Prog::new();
    let o1 = p.add_f64(1.0);
    let o2 = p.add_f64(2.0);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .inst(OpCode::FNe, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 1);
}
#[test]
fn jit_islt_signed_negative() {
    let mut p = Prog::new();
    let o = p.add_u64((-5i64) as u64);
    let bc = p
        .load_const(1, o)
        .load8(2, 0)
        .inst(OpCode::ISLt, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 1); // -5 < 0 signed
}
#[test]
fn jit_iult_unsigned_large() {
    let mut p = Prog::new();
    let o = p.add_u64(u64::MAX);
    let bc = p
        .load_const(1, o)
        .load8(2, 5)
        .inst(OpCode::IULt, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 0); // MAX не < 5 unsigned
}

// =============================================================================
// SELECT
// =============================================================================

#[test]
fn jit_select_nonzero() {
    let bc = Prog::new()
        .load8(0, 1)
        .load8(1, 10)
        .load8(2, 20)
        .inst(OpCode::Select, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 10);
}
#[test]
fn jit_select_zero() {
    let bc = Prog::new()
        .load8(0, 0)
        .load8(1, 10)
        .load8(2, 20)
        .inst(OpCode::Select, 0, 1, 2)
        .halt();
    assert_reg!(jit_run(&bc), 0, 20);
}
#[test]
fn jit_select_after_cmp() {
    let bc = Prog::new()
        .load8(1, 5)
        .load8(2, 3)
        .inst(OpCode::ISGt, 0, 1, 2)
        .load8(3, 100)
        .load8(4, 200)
        .inst(OpCode::Select, 0, 3, 4)
        .halt();
    assert_reg!(jit_run(&bc), 0, 100);
}

// =============================================================================
// CONTROL FLOW — Jmp / Jnz / Jz (только константные адреса)
// =============================================================================

#[test]
fn jit_halt_stops_execution() {
    // ip0: load8 r0,42; ip1: halt; ip2: load8 r0,99 (не выполняется)
    let bc = Prog::new()
        .load8(0, 42)
        .inst(OpCode::Halt, 0, 0, 0)
        .load8(0, 99)
        .build();
    assert_reg!(jit_run(&bc), 0, 42);
}

#[test]
fn jit_jmp_constant_forward() {
    // ip0: load8 r1,3 (target=ip3=Halt); ip1: jmp r1; ip2: load8 r0,99; ip3: halt
    let bc = Prog::new()
        .load8(1, 3)
        .inst(OpCode::Jmp, 1, 0, 0)
        .load8(0, 99)
        .inst(OpCode::Halt, 0, 0, 0)
        .build();
    assert_reg!(jit_run(&bc), 0, 0);
}

#[test]
fn jit_jnz_taken() {
    // ip0: load8 r0,1; ip1: load8 r1,4; ip2: jnz r0,r1; ip3: load8 r10,99; ip4: halt
    let bc = Prog::new()
        .load8(0, 1)
        .load8(1, 4)
        .inst(OpCode::Jnz, 0, 1, 0)
        .load8(10, 99)
        .inst(OpCode::Halt, 0, 0, 0)
        .build();
    assert_reg!(jit_run(&bc), 10, 0);
}

#[test]
fn jit_jnz_not_taken() {
    let bc = Prog::new()
        .load8(0, 0)
        .load8(1, 4)
        .inst(OpCode::Jnz, 0, 1, 0)
        .load8(10, 99)
        .inst(OpCode::Halt, 0, 0, 0)
        .build();
    assert_reg!(jit_run(&bc), 10, 99);
}

#[test]
fn jit_jz_taken() {
    let bc = Prog::new()
        .load8(0, 0)
        .load8(1, 4)
        .inst(OpCode::Jz, 0, 1, 0)
        .load8(10, 99)
        .inst(OpCode::Halt, 0, 0, 0)
        .build();
    assert_reg!(jit_run(&bc), 10, 0);
}

#[test]
fn jit_jz_not_taken() {
    let bc = Prog::new()
        .load8(0, 1)
        .load8(1, 4)
        .inst(OpCode::Jz, 0, 1, 0)
        .load8(10, 99)
        .inst(OpCode::Halt, 0, 0, 0)
        .build();
    assert_reg!(jit_run(&bc), 10, 99);
}

#[test]
fn jit_loop_count_down() {
    // ip0: r0=5; ip1: r1=1; ip2: r2=2(loop); ip3: r3=5(halt)
    // loop@ip2: nope — только константные адреса
    // ip0: load8 r0,5
    // ip1: load8 r1,1    step
    // ip2: load8 r2,3    loop_start=ip3
    // ip3[LOOP]: sub r0,r0,r1
    // ip4: load8 r3,6    halt_ip=ip6
    // ip5: jnz r0,r2
    // ip6: halt
    // wait: jnz использует r2 как target — r2=3 (load8 r2,3) — это константный регистр ✓
    let bc = Prog::new()
        .load8(0, 5)
        .load8(1, 1)
        .load8(2, 3) // loop_ip=3
        .inst(OpCode::Sub, 0, 0, 1) // ip3
        .load8(3, 6) // ip4: halt=ip6
        .inst(OpCode::Jnz, 0, 2, 0) // ip5: jnz r0, r2(=3)
        .inst(OpCode::Halt, 0, 0, 0) // ip6
        .build();
    assert_reg!(jit_run(&bc), 0, 0);
}

#[test]
fn jit_loop_sum_1_to_10() {
    // ip0: r0=0(sum) ip1: r1=1(i) ip2: r2=10(lim) ip3: r3=1(step) ip4: r4=5(loop)
    // ip5[LOOP]: IULe r5,r1,r2
    // ip6: load8 r6,12(halt)
    // ip7: jz r5,r6
    // ip8: add r0,r0,r1
    // ip9: add r1,r1,r3
    // ip10: jmp r4   <- r4=5 константный
    // ip11: halt
    let bc = Prog::new()
        .load8(0, 0)
        .load8(1, 1)
        .load8(2, 10)
        .load8(3, 1)
        .load8(4, 5)
        .inst(OpCode::IULe, 5, 1, 2) // ip5
        .load8(6, 11) // ip6: halt=ip11
        .inst(OpCode::Jz, 5, 6, 0) // ip7
        .inst(OpCode::Add, 0, 0, 1) // ip8
        .inst(OpCode::Add, 1, 1, 3) // ip9
        .inst(OpCode::Jmp, 4, 0, 0) // ip10: r4=5
        .inst(OpCode::Halt, 0, 0, 0) // ip11
        .build();
    assert_reg!(jit_run(&bc), 0, 55);
}

#[test]
fn jit_conditional_if_else() {
    let bc = Prog::new()
        .load8(1, 7)
        .load8(2, 3) // ip0,1
        .inst(OpCode::ISGt, 3, 1, 2) // ip2
        .load8(4, 8) // ip3: else=ip8
        .inst(OpCode::Jz, 3, 4, 0) // ip4
        .load8(0, 10) // ip5: then-body
        .load8(5, 9) // ip6: after_else=ip9
        .inst(OpCode::Jmp, 5, 0, 0) // ip7: skip else
        .load8(0, 20) // ip8: else-body
        .inst(OpCode::Halt, 0, 0, 0) // ip9
        .build();
    assert_reg!(jit_run(&bc), 0, 10); // 7>3 = true -> then
}

// =============================================================================
// MEMORY
// =============================================================================

#[test]
fn jit_st8_ld8() {
    let bc = Prog::new()
        .load8(1, 0)
        .load8(2, 42)
        .inst(OpCode::St8, 1, 2, 0)
        .inst(OpCode::Ld8, 0, 1, 0)
        .halt();
    assert_reg!(jit_run(&bc), 0, 42);
}
#[test]
fn jit_st16_ld16() {
    let bc = Prog::new()
        .load8(1, 0)
        .load16(2, 0x1234)
        .inst(OpCode::St16, 1, 2, 0)
        .inst(OpCode::Ld16, 0, 1, 0)
        .halt();
    assert_reg!(jit_run(&bc), 0, 0x1234);
}
#[test]
fn jit_st32_ld32() {
    let bc = Prog::new()
        .load8(1, 0)
        .load16(2, 0xBEEF)
        .inst(OpCode::St32, 1, 2, 0)
        .inst(OpCode::Ld32, 0, 1, 0)
        .halt();
    assert_reg!(jit_run(&bc), 0, 0xBEEF);
}
#[test]
fn jit_st64_ld64() {
    let mut p = Prog::new();
    let o = p.add_u64(0xCAFEBABEDEADBEEF);
    let bc = p
        .load8(1, 0)
        .load_const(2, o)
        .inst(OpCode::St64, 1, 2, 0)
        .inst(OpCode::Ld64, 0, 1, 0)
        .halt();
    assert_reg!(jit_run(&bc), 0, 0xCAFEBABEDEADBEEFu64);
}
#[test]
fn jit_memory_little_endian() {
    // Пишем u16=0x1234, читаем первый байт = 0x34
    let bc = Prog::new()
        .load8(1, 0)
        .load16(2, 0x1234)
        .inst(OpCode::St16, 1, 2, 0)
        .inst(OpCode::Ld8, 0, 1, 0)
        .halt();
    assert_reg!(jit_run(&bc), 0, 0x34);
}
#[test]
fn jit_memory_out_of_bounds_gives_zero() {
    // JIT не падает при выходе за границы — возвращает 0
    let bc = Prog::new()
        .load16(1, 9999)
        .inst(OpCode::Ld8, 0, 1, 0)
        .halt();
    let (_, r) = jit_run_mem(&bc, 64);
    assert_reg!(r, 0, 0);
}
#[test]
fn jit_memory_store_oob_safe() {
    // Запись за границу — тихо игнорируется
    let bc = Prog::new()
        .load16(1, 9999)
        .load8(2, 42)
        .inst(OpCode::St8, 1, 2, 0)
        .halt();
    let (mem, _) = jit_run_mem(&bc, 64);
    assert_eq!(mem[0], 0); // память не изменилась
}
#[test]
fn jit_multiple_memory_ops() {
    let bc = Prog::new()
        .load8(1, 0)
        .load8(2, 10)
        .load8(3, 20)
        .load8(10, 0xAA)
        .load8(11, 0xBB)
        .inst(OpCode::St8, 1, 10, 0)
        .inst(OpCode::St8, 2, 11, 0)
        .inst(OpCode::Ld8, 20, 1, 0)
        .inst(OpCode::Ld8, 21, 2, 0)
        .halt();
    let r = jit_run(&bc);
    assert_reg!(r, 20, 0xAA);
    assert_reg!(r, 21, 0xBB);
}

// =============================================================================
// ERROR CASES — DynamicJumpTarget
// =============================================================================

#[test]
fn jit_rejects_dynamic_jump() {
    // r0 загружается через Add (не константа) -> DynamicJumpTarget
    let bc = Prog::new()
        .load8(1, 3)
        .load8(2, 0)
        .inst(OpCode::Add, 0, 1, 2) // r0 = non-const
        .inst(OpCode::Jmp, 0, 0, 0)
        .inst(OpCode::Halt, 0, 0, 0)
        .build();
    assert!(jit_try(&bc).is_err());
}

#[test]
fn jit_rejects_dynamic_jnz_target() {
    let bc = Prog::new()
        .load8(0, 1)
        .load8(1, 3)
        .load8(2, 0)
        .inst(OpCode::Add, 3, 1, 2) // r3=non-const
        .inst(OpCode::Jnz, 0, 3, 0)
        .inst(OpCode::Halt, 0, 0, 0)
        .build();
    assert!(jit_try(&bc).is_err());
}

#[test]
fn jit_rejects_const_pool_out_of_bounds() {
    // LoadConst с offset=0 при пустом пуле
    let bc = Prog::new().inst(OpCode::LoadConst, 0, 0, 0).halt();
    assert!(jit_try(&bc).is_err());
}

#[test]
fn jit_accepts_constant_jump() {
    // Load8 -> const register -> Jmp OK
    let bc = Prog::new()
        .load8(1, 2)
        .inst(OpCode::Jmp, 1, 0, 0)
        .inst(OpCode::Halt, 0, 0, 0)
        .build();
    assert!(jit_try(&bc).is_ok());
}

#[test]
fn jit_accepts_mov_propagated_const() {
    // Load8 r1,3 -> Mov r2,r1 -> Jmp r2: r2 это константа через propagation
    let bc = Prog::new()
        .load8(1, 3)
        .inst(OpCode::Mov, 2, 1, 0)
        .inst(OpCode::Jmp, 2, 0, 0)
        .inst(OpCode::Halt, 0, 0, 0)
        .build();
    assert!(jit_try(&bc).is_ok());
}

// =============================================================================
// COMPLEX PROGRAMS
// =============================================================================

#[test]
fn jit_fibonacci_7_unrolled() {
    // fib(7)=13 через развёртку
    let bc = Prog::new()
        .load8(0, 0)
        .load8(1, 1)
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
    assert_reg!(jit_run(&bc), 0, 13);
}

#[test]
fn jit_sum_closed_form_result() {
    // sum(0..100)=4950 через константу
    let mut p = Prog::new();
    let o = p.add_u64(4950);
    let bc = p.load_const(0, o).halt();
    assert_reg!(jit_run(&bc), 0, 4950);
}

#[test]
fn jit_quadratic_3x2_plus_2x_plus_1_at_x4() {
    // 3*16 + 2*4 + 1 = 48+8+1 = 57
    let bc = Prog::new()
        .load8(1, 3)
        .load8(2, 4)
        .inst(OpCode::Mul, 3, 2, 2) // x^2=16
        .inst(OpCode::Mul, 4, 1, 3) // 3*16=48
        .load8(5, 2)
        .inst(OpCode::Mul, 6, 5, 2) // 2*4=8
        .inst(OpCode::Add, 7, 4, 6) // 56
        .load8(8, 1)
        .inst(OpCode::Add, 0, 7, 8) // 57
        .halt();
    assert_reg!(jit_run(&bc), 0, 57);
}

#[test]
fn jit_max_of_two() {
    // max(13,42)=42
    let bc = Prog::new()
        .load8(1, 13)
        .load8(2, 42)
        .inst(OpCode::ISGt, 3, 1, 2)
        .inst(OpCode::Select, 3, 1, 2)
        .inst(OpCode::Mov, 0, 3, 0)
        .halt();
    assert_reg!(jit_run(&bc), 0, 42);
}

#[test]
fn jit_abs_negative() {
    let mut p = Prog::new();
    let o = p.add_u64((-42i64) as u64);
    let bc = p
        .load_const(1, o)
        .inst(OpCode::Neg, 2, 1, 0)
        .load8(3, 0)
        .inst(OpCode::ISGt, 4, 1, 3)
        .inst(OpCode::Select, 4, 1, 2)
        .inst(OpCode::Mov, 0, 4, 0)
        .halt();
    assert_eq!(jit_run(&bc)[0] as i64, 42);
}

#[test]
fn jit_complex_float_expression() {
    // (1.5+2.5) * (10.0-8.0) / 2.0 = 4.0 * 2.0 / 2.0 = 4.0
    let mut p = Prog::new();
    let o1 = p.add_f64(1.5);
    let o2 = p.add_f64(2.5);
    let o3 = p.add_f64(10.0);
    let o4 = p.add_f64(8.0);
    let o5 = p.add_f64(2.0);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .load_const(3, o3)
        .load_const(4, o4)
        .load_const(5, o5)
        .inst(OpCode::FAdd, 6, 1, 2)
        .inst(OpCode::FSub, 7, 3, 4)
        .inst(OpCode::FMul, 8, 6, 7)
        .inst(OpCode::FDiv, 0, 8, 5)
        .halt();
    assert_f64!(jit_run(&bc), 0, 4.0);
}

#[test]
fn jit_register_independence() {
    let bc = Prog::new()
        .load8(10, 1)
        .load8(11, 2)
        .load8(12, 3)
        .inst(OpCode::Add, 20, 10, 11)
        .inst(OpCode::Mul, 21, 11, 12)
        .inst(OpCode::Sub, 22, 12, 10)
        .halt();
    let r = jit_run(&bc);
    assert_reg!(r, 20, 3);
    assert_reg!(r, 21, 6);
    assert_reg!(r, 22, 2);
    assert_reg!(r, 10, 1);
    assert_reg!(r, 11, 2);
    assert_reg!(r, 12, 3);
}

#[test]
fn jit_noopcount() {
    // NoOp не меняет состояние
    let bc = Prog::new()
        .load8(0, 42)
        .inst(OpCode::NoOp, 0, 0, 0)
        .inst(OpCode::NoOp, 0, 0, 0)
        .halt();
    assert_reg!(jit_run(&bc), 0, 42);
}

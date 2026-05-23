// w16-core\src\jit\jit_compiler.rs
//
//! JIT-компилятор для runtime W16.
//!
//! Модуль переводит W16 `Bytecode` в нативный машинный код с помощью Cranelift.
//! Выполняется упрощённый статический анализ переходов с разрешением константных
//! меток, проверяется доступ к пулу констант, и поддерживаемые инструкции W16
//! понижаются в нативную функцию, которая может выполняться на хостовом CPU.

use std::collections::BTreeSet;
use std::fmt;

use crate::{Bytecode, OpCode, REGISTER_COUNT};
use cranelift::codegen::ir::BlockArg;
use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};

extern "C" fn w16_jit_frem(lhs: u64, rhs: u64) -> u64 {
    (f64::from_bits(lhs) % f64::from_bits(rhs)).to_bits()
}

extern "C" fn w16_jit_print_int(value: u64) {
    println!("{}", value as i64);
}

extern "C" fn w16_jit_print_uint(value: u64) {
    println!("{value}");
}

extern "C" fn w16_jit_print_float(value: u64) {
    println!("{}", f64::from_bits(value));
}

extern "C" fn w16_jit_print_str(consts: *const u8, consts_len: usize, index: u64) {
    let index = index as usize;
    if consts.is_null() || index.checked_add(8).is_none_or(|end| end > consts_len) {
        return;
    }

    unsafe {
        let len_bytes = std::slice::from_raw_parts(consts.add(index), 8);
        let str_len = u64::from_le_bytes(len_bytes.try_into().unwrap_unchecked()) as usize;
        let Some(start) = index.checked_add(8) else {
            return;
        };
        let Some(end) = start.checked_add(str_len) else {
            return;
        };
        if end > consts_len {
            return;
        }

        let bytes = std::slice::from_raw_parts(consts.add(start), str_len);
        if let Ok(text) = std::str::from_utf8(bytes) {
            print!("{text}");
        }
    }
}

/// Ошибки, которые могут возникнуть при компиляции W16 байткода JIT.
///
/// Включает неподдерживаемые инструкции, некорректные обращения к пулу констант,
/// динамические цели переходов, неверные адреса переходов через регистр и
/// ошибки самого Cranelift.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JitError {
    UnsupportedOpcode { ip: usize, opcode: OpCode },
    ConstantPoolOutOfBounds { ip: usize, offset: usize },
    DynamicJumpTarget { ip: usize, register: u8 },
    InvalidRegisterJump { ip: usize, target: usize },
    Cranelift(String),
}

impl fmt::Display for JitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JitError::UnsupportedOpcode { ip, opcode } => {
                write!(f, "JIT does not support {opcode:?} at ip {ip}")
            }
            JitError::ConstantPoolOutOfBounds { ip, offset } => {
                write!(
                    f,
                    "LoadConst at ip {ip} reads past constant pool at offset {offset}"
                )
            }
            JitError::DynamicJumpTarget { ip, register } => {
                write!(
                    f,
                    "jump at ip {ip} uses non-constant target register r{register}"
                )
            }
            JitError::InvalidRegisterJump { ip, target } => {
                write!(f, "jump at ip {ip} targets invalid instruction {target}")
            }
            JitError::Cranelift(message) => write!(f, "Cranelift error: {message}"),
        }
    }
}

impl std::error::Error for JitError {}

/// JIT-компилятор на базе Cranelift для W16 байткода.
///
/// Компилятор создаёт нативную функцию, которая обновляет регистровый файл W16,
/// выполняет проверки границ памяти и вызывает runtime-хелперы для операций,
/// которые нельзя выразить напрямую через Cranelift.
pub struct JIT {
    module: JITModule,
    ctx: codegen::Context,
    builder_context: FunctionBuilderContext,
    function_counter: usize,
}

#[derive(Clone)]
struct JumpTargets {
    jmp: Vec<Option<usize>>,
    branch: Vec<Option<usize>>,
}

#[derive(Clone, Copy)]
struct Imports {
    frem: FuncId,
    print_int: FuncId,
    print_uint: FuncId,
    print_float: FuncId,
    print_str: FuncId,
}

impl JIT {
    /// Создать новый экземпляр JIT-компилятора.
    ///
    /// Инициализирует Cranelift для текущей хостовой ISA, регистрирует
    /// внешние символы-специализированные функции и подготавливает пустой
    /// контекст компиляции.
    pub fn new() -> Self {
        let mut flag_builder = settings::builder();
        flag_builder.set("opt_level", "speed").unwrap();

        let isa_builder = cranelift_native::builder().unwrap_or_else(|msg| {
            panic!("Host ISA not supported: {msg}");
        });
        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .unwrap();

        let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
        builder.symbol("w16_jit_frem", w16_jit_frem as *const u8);
        builder.symbol("w16_jit_print_int", w16_jit_print_int as *const u8);
        builder.symbol("w16_jit_print_uint", w16_jit_print_uint as *const u8);
        builder.symbol("w16_jit_print_float", w16_jit_print_float as *const u8);
        builder.symbol("w16_jit_print_str", w16_jit_print_str as *const u8);
        let module = JITModule::new(builder);

        Self {
            module,
            ctx: codegen::Context::new(),
            builder_context: FunctionBuilderContext::new(),
            function_counter: 0,
        }
    }

    /// Скомпилировать W16 байткод в исполняемую нативную функцию.
    ///
    /// При ошибке компиляции вызывает `panic`. Для обработки ошибок используйте
    /// `try_compile`.
    pub fn compile(&mut self, bytecode: &Bytecode) -> *const u8 {
        self.try_compile(bytecode)
            .unwrap_or_else(|error| panic!("JIT compilation failed: {error}"))
    }

    /// Попытаться скомпилировать W16 байткод в нативный код.
    ///
    /// Возвращает указатель на сгенерированную функцию при успехе или `JitError`,
    /// описывающую причину неудачи.
    pub fn try_compile(&mut self, bytecode: &Bytecode) -> Result<*const u8, JitError> {
        let jump_targets = self.analyze_jumps(bytecode)?;
        let (read_regs, written_regs) = collect_registers(bytecode);
        validate_constants(bytecode)?;

        let int_ptr = self.module.target_config().pointer_type();
        let imports = self.declare_imports(int_ptr)?;
        self.ctx.func.signature.params.clear();
        self.ctx.func.signature.returns.clear();
        self.ctx.func.signature.call_conv = self.module.target_config().default_call_conv;
        self.ctx.func.signature.params.push(AbiParam::new(int_ptr));
        self.ctx.func.signature.params.push(AbiParam::new(int_ptr));
        self.ctx.func.signature.params.push(AbiParam::new(int_ptr));
        self.ctx.func.signature.params.push(AbiParam::new(int_ptr));
        self.ctx.func.signature.params.push(AbiParam::new(int_ptr));

        {
            let mut builder = FunctionBuilder::new(&mut self.ctx.func, &mut self.builder_context);
            let entry_block = builder.create_block();
            builder.append_block_params_for_function_params(entry_block);
            builder.switch_to_block(entry_block);

            let regs_ptr = builder.block_params(entry_block)[0];
            let memory_ptr = builder.block_params(entry_block)[1];
            let memory_len = builder.block_params(entry_block)[2];
            let consts_ptr = builder.block_params(entry_block)[3];
            let consts_len = builder.block_params(entry_block)[4];

            let frem_func = self.module.declare_func_in_func(imports.frem, builder.func);
            let print_int_func = self
                .module
                .declare_func_in_func(imports.print_int, builder.func);
            let print_uint_func = self
                .module
                .declare_func_in_func(imports.print_uint, builder.func);
            let print_float_func = self
                .module
                .declare_func_in_func(imports.print_float, builder.func);
            let print_str_func = self
                .module
                .declare_func_in_func(imports.print_str, builder.func);

            let mut vars: Vec<Option<Variable>> = vec![None; REGISTER_COUNT];
            for reg in read_regs.union(&written_regs) {
                let var = builder.declare_var(types::I64);
                let value =
                    builder
                        .ins()
                        .load(types::I64, MemFlags::new(), regs_ptr, (*reg * 8) as i32);
                builder.def_var(var, value);
                vars[*reg] = Some(var);
            }

            let mut blocks = Vec::with_capacity(bytecode.instructions.len());
            for _ in 0..bytecode.instructions.len() {
                blocks.push(builder.create_block());
            }
            let exit_block = builder.create_block();

            if let Some(first_block) = blocks.first().copied() {
                builder.ins().jump(first_block, &[]);
            } else {
                builder.ins().jump(exit_block, &[]);
            }

            for (ip, instr) in bytecode.instructions.iter().enumerate() {
                builder.switch_to_block(blocks[ip]);

                match instr.opcode {
                    OpCode::Halt | OpCode::Ret => {
                        builder.ins().jump(exit_block, &[]);
                    }
                    OpCode::NoOp => jump_to_next(&mut builder, &blocks, exit_block, ip),
                    OpCode::Mov => {
                        let value = use_reg(&mut builder, &vars, instr.b);
                        def_reg(&mut builder, &vars, instr.a, value);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::Load8 => {
                        let value = builder.ins().iconst(types::I64, instr.c as i64);
                        def_reg(&mut builder, &vars, instr.a, value);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::Load16 => {
                        let value = builder.ins().iconst(types::I64, instr.imm16() as i64);
                        def_reg(&mut builder, &vars, instr.a, value);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::LoadConst => {
                        let value = builder.ins().load(
                            types::I64,
                            MemFlags::new(),
                            consts_ptr,
                            instr.imm16() as i32,
                        );
                        def_reg(&mut builder, &vars, instr.a, value);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::Add => {
                        emit_binop(&mut builder, &vars, instr, |b, c, builder| {
                            builder.ins().iadd(b, c)
                        });
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::Sub => {
                        emit_binop(&mut builder, &vars, instr, |b, c, builder| {
                            builder.ins().isub(b, c)
                        });
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::Mul => {
                        emit_binop(&mut builder, &vars, instr, |b, c, builder| {
                            builder.ins().imul(b, c)
                        });
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::And => {
                        emit_binop(&mut builder, &vars, instr, |b, c, builder| {
                            builder.ins().band(b, c)
                        });
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::Or => {
                        emit_binop(&mut builder, &vars, instr, |b, c, builder| {
                            builder.ins().bor(b, c)
                        });
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::Xor => {
                        emit_binop(&mut builder, &vars, instr, |b, c, builder| {
                            builder.ins().bxor(b, c)
                        });
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::Shl => {
                        emit_binop(&mut builder, &vars, instr, |b, c, builder| {
                            builder.ins().ishl(b, c)
                        });
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::Shr => {
                        emit_binop(&mut builder, &vars, instr, |b, c, builder| {
                            builder.ins().ushr(b, c)
                        });
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::Sar => {
                        emit_binop(&mut builder, &vars, instr, |b, c, builder| {
                            builder.ins().sshr(b, c)
                        });
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::Not => {
                        let b = use_reg(&mut builder, &vars, instr.b);
                        let value = builder.ins().bnot(b);
                        def_reg(&mut builder, &vars, instr.a, value);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::Neg => {
                        let b = use_reg(&mut builder, &vars, instr.b);
                        let value = builder.ins().ineg(b);
                        def_reg(&mut builder, &vars, instr.a, value);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::UDiv => {
                        emit_checked_div(&mut builder, &vars, instr, false, false);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::URem => {
                        emit_checked_div(&mut builder, &vars, instr, false, true);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::IDiv => {
                        emit_checked_div(&mut builder, &vars, instr, true, false);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::IRem => {
                        emit_checked_div(&mut builder, &vars, instr, true, true);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::IEq => {
                        emit_icmp(&mut builder, &vars, instr, IntCC::Equal);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::INe => {
                        emit_icmp(&mut builder, &vars, instr, IntCC::NotEqual);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::ISLt => {
                        emit_icmp(&mut builder, &vars, instr, IntCC::SignedLessThan);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::ISLe => {
                        emit_icmp(&mut builder, &vars, instr, IntCC::SignedLessThanOrEqual);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::ISGt => {
                        emit_icmp(&mut builder, &vars, instr, IntCC::SignedGreaterThan);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::ISGe => {
                        emit_icmp(&mut builder, &vars, instr, IntCC::SignedGreaterThanOrEqual);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::IULt => {
                        emit_icmp(&mut builder, &vars, instr, IntCC::UnsignedLessThan);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::IULe => {
                        emit_icmp(&mut builder, &vars, instr, IntCC::UnsignedLessThanOrEqual);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::IUGt => {
                        emit_icmp(&mut builder, &vars, instr, IntCC::UnsignedGreaterThan);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::IUGe => {
                        emit_icmp(
                            &mut builder,
                            &vars,
                            instr,
                            IntCC::UnsignedGreaterThanOrEqual,
                        );
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::FAdd => {
                        emit_float_binop(&mut builder, &vars, instr, |b, c, builder| {
                            builder.ins().fadd(b, c)
                        });
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::FSub => {
                        emit_float_binop(&mut builder, &vars, instr, |b, c, builder| {
                            builder.ins().fsub(b, c)
                        });
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::FMul => {
                        emit_float_binop(&mut builder, &vars, instr, |b, c, builder| {
                            builder.ins().fmul(b, c)
                        });
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::FDiv => {
                        emit_float_binop(&mut builder, &vars, instr, |b, c, builder| {
                            builder.ins().fdiv(b, c)
                        });
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::FRem => {
                        let lhs = use_reg(&mut builder, &vars, instr.b);
                        let rhs = use_reg(&mut builder, &vars, instr.c);
                        let call = builder.ins().call(frem_func, &[lhs, rhs]);
                        let value = builder.inst_results(call)[0];
                        def_reg(&mut builder, &vars, instr.a, value);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::FNeg => {
                        let b_bits = use_reg(&mut builder, &vars, instr.b);
                        let b = int_to_float(&mut builder, b_bits);
                        let value = builder.ins().fneg(b);
                        let bits = float_to_int(&mut builder, value);
                        def_reg(&mut builder, &vars, instr.a, bits);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::FAbs => {
                        let b_bits = use_reg(&mut builder, &vars, instr.b);
                        let b = int_to_float(&mut builder, b_bits);
                        let value = builder.ins().fabs(b);
                        let bits = float_to_int(&mut builder, value);
                        def_reg(&mut builder, &vars, instr.a, bits);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::FEq => {
                        emit_fcmp(&mut builder, &vars, instr, FloatCC::Equal);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::FNe => {
                        emit_fcmp(&mut builder, &vars, instr, FloatCC::NotEqual);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::FLt => {
                        emit_fcmp(&mut builder, &vars, instr, FloatCC::LessThan);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::FLe => {
                        emit_fcmp(&mut builder, &vars, instr, FloatCC::LessThanOrEqual);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::FGt => {
                        emit_fcmp(&mut builder, &vars, instr, FloatCC::GreaterThan);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::FGe => {
                        emit_fcmp(&mut builder, &vars, instr, FloatCC::GreaterThanOrEqual);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::I2F => {
                        let b = use_reg(&mut builder, &vars, instr.b);
                        let value = builder.ins().fcvt_from_sint(types::F64, b);
                        let bits = float_to_int(&mut builder, value);
                        def_reg(&mut builder, &vars, instr.a, bits);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::U2F => {
                        let b = use_reg(&mut builder, &vars, instr.b);
                        let value = builder.ins().fcvt_from_uint(types::F64, b);
                        let bits = float_to_int(&mut builder, value);
                        def_reg(&mut builder, &vars, instr.a, bits);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::F2I => {
                        let b_bits = use_reg(&mut builder, &vars, instr.b);
                        let b = int_to_float(&mut builder, b_bits);
                        let value = builder.ins().fcvt_to_sint(types::I64, b);
                        def_reg(&mut builder, &vars, instr.a, value);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::F2U => {
                        let b_bits = use_reg(&mut builder, &vars, instr.b);
                        let b = int_to_float(&mut builder, b_bits);
                        let value = builder.ins().fcvt_to_uint(types::I64, b);
                        def_reg(&mut builder, &vars, instr.a, value);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::Select => {
                        let cond = use_reg(&mut builder, &vars, instr.a);
                        let cond = builder.ins().icmp_imm(IntCC::NotEqual, cond, 0);
                        let b = use_reg(&mut builder, &vars, instr.b);
                        let c = use_reg(&mut builder, &vars, instr.c);
                        let value = builder.ins().select(cond, b, c);
                        def_reg(&mut builder, &vars, instr.a, value);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::Jmp | OpCode::Call => {
                        jump_to_target(&mut builder, &blocks, exit_block, jump_targets.jmp[ip]);
                    }
                    OpCode::Jnz => {
                        let cond = use_reg(&mut builder, &vars, instr.a);
                        let cond = builder.ins().icmp_imm(IntCC::NotEqual, cond, 0);
                        let target = target_block(&blocks, exit_block, jump_targets.branch[ip]);
                        let next = next_block(&blocks, exit_block, ip);
                        builder.ins().brif(cond, target, &[], next, &[]);
                    }
                    OpCode::Jz => {
                        let cond = use_reg(&mut builder, &vars, instr.a);
                        let cond = builder.ins().icmp_imm(IntCC::Equal, cond, 0);
                        let target = target_block(&blocks, exit_block, jump_targets.branch[ip]);
                        let next = next_block(&blocks, exit_block, ip);
                        builder.ins().brif(cond, target, &[], next, &[]);
                    }
                    OpCode::Ld8 => {
                        emit_checked_load(
                            &mut builder,
                            &vars,
                            instr,
                            memory_ptr,
                            memory_len,
                            types::I8,
                            1,
                        );
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::Ld16 => {
                        emit_checked_load(
                            &mut builder,
                            &vars,
                            instr,
                            memory_ptr,
                            memory_len,
                            types::I16,
                            2,
                        );
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::Ld32 => {
                        emit_checked_load(
                            &mut builder,
                            &vars,
                            instr,
                            memory_ptr,
                            memory_len,
                            types::I32,
                            4,
                        );
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::Ld64 => {
                        emit_checked_load(
                            &mut builder,
                            &vars,
                            instr,
                            memory_ptr,
                            memory_len,
                            types::I64,
                            8,
                        );
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::St8 => {
                        emit_checked_store(
                            &mut builder,
                            &vars,
                            instr,
                            memory_ptr,
                            memory_len,
                            types::I8,
                            1,
                        );
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::St16 => {
                        emit_checked_store(
                            &mut builder,
                            &vars,
                            instr,
                            memory_ptr,
                            memory_len,
                            types::I16,
                            2,
                        );
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::St32 => {
                        emit_checked_store(
                            &mut builder,
                            &vars,
                            instr,
                            memory_ptr,
                            memory_len,
                            types::I32,
                            4,
                        );
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::St64 => {
                        emit_checked_store(
                            &mut builder,
                            &vars,
                            instr,
                            memory_ptr,
                            memory_len,
                            types::I64,
                            8,
                        );
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::PrintInt => {
                        let value = use_reg(&mut builder, &vars, instr.a);
                        builder.ins().call(print_int_func, &[value]);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::PrintUInt => {
                        let value = use_reg(&mut builder, &vars, instr.a);
                        builder.ins().call(print_uint_func, &[value]);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::PrintFloat => {
                        let value = use_reg(&mut builder, &vars, instr.a);
                        builder.ins().call(print_float_func, &[value]);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                    OpCode::PrintStr => {
                        let index = use_reg(&mut builder, &vars, instr.a);
                        builder
                            .ins()
                            .call(print_str_func, &[consts_ptr, consts_len, index]);
                        jump_to_next(&mut builder, &blocks, exit_block, ip);
                    }
                }
            }

            builder.seal_all_blocks();
            builder.switch_to_block(exit_block);
            for reg in &written_regs {
                let value = builder.use_var(vars[*reg].expect("written register must have a var"));
                builder
                    .ins()
                    .store(MemFlags::new(), value, regs_ptr, (*reg * 8) as i32);
            }
            builder.ins().return_(&[]);
            builder.finalize();
        }

        let name = format!("w16_main_{}", self.function_counter);
        self.function_counter += 1;
        let id = self
            .module
            .declare_function(&name, Linkage::Export, &self.ctx.func.signature)
            .map_err(|error| JitError::Cranelift(error.to_string()))?;
        self.module
            .define_function(id, &mut self.ctx)
            .map_err(|error| JitError::Cranelift(error.to_string()))?;
        self.module
            .finalize_definitions()
            .map_err(|error| JitError::Cranelift(error.to_string()))?;

        let code_ptr = self.module.get_finalized_function(id);
        self.ctx.clear();
        Ok(code_ptr)
    }

    fn declare_imports(&mut self, int_ptr: Type) -> Result<Imports, JitError> {
        let mut frem_sig = self.module.make_signature();
        frem_sig.call_conv = self.module.target_config().default_call_conv;
        frem_sig.params.push(AbiParam::new(types::I64));
        frem_sig.params.push(AbiParam::new(types::I64));
        frem_sig.returns.push(AbiParam::new(types::I64));

        let mut print_one_sig = self.module.make_signature();
        print_one_sig.call_conv = self.module.target_config().default_call_conv;
        print_one_sig.params.push(AbiParam::new(types::I64));

        let mut print_str_sig = self.module.make_signature();
        print_str_sig.call_conv = self.module.target_config().default_call_conv;
        print_str_sig.params.push(AbiParam::new(int_ptr));
        print_str_sig.params.push(AbiParam::new(int_ptr));
        print_str_sig.params.push(AbiParam::new(types::I64));

        let frem = self
            .module
            .declare_function("w16_jit_frem", Linkage::Import, &frem_sig)
            .map_err(|error| JitError::Cranelift(error.to_string()))?;
        let print_int = self
            .module
            .declare_function("w16_jit_print_int", Linkage::Import, &print_one_sig)
            .map_err(|error| JitError::Cranelift(error.to_string()))?;
        let print_uint = self
            .module
            .declare_function("w16_jit_print_uint", Linkage::Import, &print_one_sig)
            .map_err(|error| JitError::Cranelift(error.to_string()))?;
        let print_float = self
            .module
            .declare_function("w16_jit_print_float", Linkage::Import, &print_one_sig)
            .map_err(|error| JitError::Cranelift(error.to_string()))?;
        let print_str = self
            .module
            .declare_function("w16_jit_print_str", Linkage::Import, &print_str_sig)
            .map_err(|error| JitError::Cranelift(error.to_string()))?;

        Ok(Imports {
            frem,
            print_int,
            print_uint,
            print_float,
            print_str,
        })
    }

    /// Анализирует инструкции перехода и разрешает статические цели ветвления.
    ///
    /// Проход отслеживает регистры, которые содержат константы, и проверяет,
    /// что цели для `Jmp`, `Jz` и `Jnz` известны на этапе компиляции.
    /// Динамические цели переходов отклоняются с `JitError::DynamicJumpTarget`.
    fn analyze_jumps(&self, bytecode: &Bytecode) -> Result<JumpTargets, JitError> {
        let mut const_regs = [None; REGISTER_COUNT];
        let mut jmp = vec![None; bytecode.instructions.len()];
        let mut branch = vec![None; bytecode.instructions.len()];

        for (ip, instr) in bytecode.instructions.iter().enumerate() {
            match instr.opcode {
                OpCode::Jmp | OpCode::Call => {
                    jmp[ip] = Some(resolve_target(
                        ip,
                        instr.a,
                        &const_regs,
                        bytecode.instructions.len(),
                    )?);
                }
                OpCode::Jz | OpCode::Jnz => {
                    branch[ip] = Some(resolve_target(
                        ip,
                        instr.b,
                        &const_regs,
                        bytecode.instructions.len(),
                    )?);
                }
                _ => {}
            }

            update_const_regs(&mut const_regs, instr);
        }

        Ok(JumpTargets { jmp, branch })
    }
}

/// Проверяет, что каждая инструкция `LoadConst` читается из пула констант
/// полностью, как `u64`.
fn validate_constants(bytecode: &Bytecode) -> Result<(), JitError> {
    for (ip, instr) in bytecode.instructions.iter().enumerate() {
        if instr.opcode == OpCode::LoadConst {
            let offset = instr.imm16() as usize;
            if offset + 8 > bytecode.constant_pool.data.len() {
                return Err(JitError::ConstantPoolOutOfBounds { ip, offset });
            }
        }
    }
    Ok(())
}

/// Разрешает цель перехода, заданную регистром, с помощью константного
/// распространения.
///
/// Возвращает статический индекс инструкции или ошибку, если регистр не
/// содержит константу или цель недопустима.
fn resolve_target(
    ip: usize,
    register: u8,
    const_regs: &[Option<u64>; REGISTER_COUNT],
    code_len: usize,
) -> Result<usize, JitError> {
    let target =
        const_regs[register as usize].ok_or(JitError::DynamicJumpTarget { ip, register })? as usize;
    if target > isize::MAX as usize {
        return Err(JitError::InvalidRegisterJump { ip, target });
    }
    Ok(target.min(code_len))
}

/// Обновляет таблицу константных регистров для статического анализа.
///
/// Хранит значение в регистре, если инструкция гарантированно задаёт
/// константу, иначе стирает информацию о константе.
fn update_const_regs(const_regs: &mut [Option<u64>; REGISTER_COUNT], instr: &crate::Instruction) {
    let a = instr.a as usize;
    const_regs[a] = match instr.opcode {
        OpCode::NoOp
        | OpCode::Halt
        | OpCode::Jmp
        | OpCode::Jnz
        | OpCode::Jz
        | OpCode::Call
        | OpCode::Ret => const_regs[a],
        OpCode::Load8 => Some(instr.c as u64),
        OpCode::Load16 => Some(instr.imm16() as u64),
        OpCode::Mov => const_regs[instr.b as usize],
        _ => None,
    };
}

/// Собирает множества регистров, которые читаются и пишутся в программе.
///
/// Используется для определения переменных, которые требуется загрузить в
/// Cranelift-переменные до начала исполнения и сохранить обратно после.
fn collect_registers(bytecode: &Bytecode) -> (BTreeSet<usize>, BTreeSet<usize>) {
    let mut reads = BTreeSet::new();
    let mut writes = BTreeSet::new();

    for instr in &bytecode.instructions {
        match instr.opcode {
            OpCode::Halt | OpCode::NoOp | OpCode::Ret => {}
            OpCode::Load8 | OpCode::Load16 | OpCode::LoadConst => {
                writes.insert(instr.a as usize);
            }
            OpCode::Mov
            | OpCode::Neg
            | OpCode::Not
            | OpCode::FNeg
            | OpCode::FAbs
            | OpCode::I2F
            | OpCode::U2F
            | OpCode::F2I
            | OpCode::F2U => {
                reads.insert(instr.b as usize);
                writes.insert(instr.a as usize);
            }
            OpCode::Jmp | OpCode::Call => {
                reads.insert(instr.a as usize);
            }
            OpCode::Jz | OpCode::Jnz => {
                reads.insert(instr.a as usize);
                reads.insert(instr.b as usize);
            }
            OpCode::St8 | OpCode::St16 | OpCode::St32 | OpCode::St64 => {
                reads.insert(instr.a as usize);
                reads.insert(instr.b as usize);
            }
            OpCode::PrintStr | OpCode::PrintInt | OpCode::PrintUInt | OpCode::PrintFloat => {
                reads.insert(instr.a as usize);
            }
            OpCode::Select => {
                reads.insert(instr.a as usize);
                reads.insert(instr.b as usize);
                reads.insert(instr.c as usize);
                writes.insert(instr.a as usize);
            }
            _ => {
                reads.insert(instr.b as usize);
                reads.insert(instr.c as usize);
                writes.insert(instr.a as usize);
            }
        }
    }

    (reads, writes)
}

/// Использовать ранее объявленную Cranelift-переменную для регистра W16.
fn use_reg(builder: &mut FunctionBuilder<'_>, vars: &[Option<Variable>], reg: u8) -> Value {
    builder.use_var(vars[reg as usize].expect("register must have a var"))
}

/// Записать значение в Cranelift-переменную, соответствующую регистру W16.
fn def_reg(builder: &mut FunctionBuilder<'_>, vars: &[Option<Variable>], reg: u8, value: Value) {
    builder.def_var(vars[reg as usize].expect("register must have a var"), value);
}

/// Получает блок для следующей инструкции или выходной блок, если это конец.
fn next_block(blocks: &[Block], exit_block: Block, ip: usize) -> Block {
    blocks.get(ip + 1).copied().unwrap_or(exit_block)
}

/// Получает блок для целевого перехода или выходной блок, если цель не задана.
fn target_block(blocks: &[Block], exit_block: Block, target: Option<usize>) -> Block {
    target
        .and_then(|target| blocks.get(target).copied())
        .unwrap_or(exit_block)
}

/// Сгенерировать переход на следующую инструкцию.
fn jump_to_next(builder: &mut FunctionBuilder<'_>, blocks: &[Block], exit_block: Block, ip: usize) {
    builder.ins().jump(next_block(blocks, exit_block, ip), &[]);
}

/// Сгенерировать переход на заранее разрешённую цель.
fn jump_to_target(
    builder: &mut FunctionBuilder<'_>,
    blocks: &[Block],
    exit_block: Block,
    target: Option<usize>,
) {
    builder
        .ins()
        .jump(target_block(blocks, exit_block, target), &[]);
}

/// Сгенерировать бинарную целочисленную операцию для двух регистров.
fn emit_binop(
    builder: &mut FunctionBuilder<'_>,
    vars: &[Option<Variable>],
    instr: &crate::Instruction,
    op: impl FnOnce(Value, Value, &mut FunctionBuilder<'_>) -> Value,
) {
    let b = use_reg(builder, vars, instr.b);
    let c = use_reg(builder, vars, instr.c);
    let value = op(b, c, builder);
    def_reg(builder, vars, instr.a, value);
}

/// Сгенерировать целочисленное сравнение и записать флаг результата.
fn emit_icmp(
    builder: &mut FunctionBuilder<'_>,
    vars: &[Option<Variable>],
    instr: &crate::Instruction,
    condition: IntCC,
) {
    let b = use_reg(builder, vars, instr.b);
    let c = use_reg(builder, vars, instr.c);
    let value = builder.ins().icmp(condition, b, c);
    let value = builder.ins().uextend(types::I64, value);
    def_reg(builder, vars, instr.a, value);
}

/// Сгенерировать сравнение чисел с плавающей запятой.
fn emit_fcmp(
    builder: &mut FunctionBuilder<'_>,
    vars: &[Option<Variable>],
    instr: &crate::Instruction,
    condition: FloatCC,
) {
    let b_bits = use_reg(builder, vars, instr.b);
    let c_bits = use_reg(builder, vars, instr.c);
    let b = int_to_float(builder, b_bits);
    let c = int_to_float(builder, c_bits);
    let value = builder.ins().fcmp(condition, b, c);
    let value = builder.ins().uextend(types::I64, value);
    def_reg(builder, vars, instr.a, value);
}

/// Сгенерировать бинарную операцию над значениями типа `f64`.
fn emit_float_binop(
    builder: &mut FunctionBuilder<'_>,
    vars: &[Option<Variable>],
    instr: &crate::Instruction,
    op: impl FnOnce(Value, Value, &mut FunctionBuilder<'_>) -> Value,
) {
    let b_bits = use_reg(builder, vars, instr.b);
    let c_bits = use_reg(builder, vars, instr.c);
    let b = int_to_float(builder, b_bits);
    let c = int_to_float(builder, c_bits);
    let value = op(b, c, builder);
    let bits = float_to_int(builder, value);
    def_reg(builder, vars, instr.a, bits);
}

/// Сгенерировать проверенное деление/остаток.
///
/// Для деления по нулю возвращает 0. Для signed-деления дополнительно
/// обрабатывает случай `i64::MIN / -1` во избежание переполнения.
fn emit_checked_div(
    builder: &mut FunctionBuilder<'_>,
    vars: &[Option<Variable>],
    instr: &crate::Instruction,
    signed: bool,
    remainder: bool,
) {
    let b = use_reg(builder, vars, instr.b);
    let c = use_reg(builder, vars, instr.c);
    let zero_block = builder.create_block();
    let div_block = builder.create_block();
    let cont_block = builder.create_block();
    builder.append_block_param(cont_block, types::I64);

    let c_nonzero = builder.ins().icmp_imm(IntCC::NotEqual, c, 0);
    builder
        .ins()
        .brif(c_nonzero, div_block, &[], zero_block, &[]);

    builder.switch_to_block(zero_block);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().jump(cont_block, &[BlockArg::Value(zero)]);

    builder.switch_to_block(div_block);
    if signed {
        let overflow_block = builder.create_block();
        let real_div_block = builder.create_block();
        let is_min = builder.ins().icmp_imm(IntCC::Equal, b, i64::MIN);
        builder
            .ins()
            .brif(is_min, overflow_block, &[], real_div_block, &[]);

        builder.switch_to_block(overflow_block);
        let is_neg_one = builder.ins().icmp_imm(IntCC::Equal, c, -1);
        builder
            .ins()
            .brif(is_neg_one, zero_block, &[], real_div_block, &[]);

        builder.switch_to_block(real_div_block);
    }

    let value = if signed {
        if remainder {
            builder.ins().srem(b, c)
        } else {
            builder.ins().sdiv(b, c)
        }
    } else if remainder {
        builder.ins().urem(b, c)
    } else {
        builder.ins().udiv(b, c)
    };
    builder.ins().jump(cont_block, &[BlockArg::Value(value)]);

    builder.switch_to_block(cont_block);
    let value = builder.block_params(cont_block)[0];
    def_reg(builder, vars, instr.a, value);
}

/// Генерирует Cranelift-код для чтения из памяти с проверкой границ.
///
/// Если запрошенная загрузка выходит за пределы буфера, код записывает ноль
/// в целевой регистр вместо того, чтобы вызвать исключение.
fn emit_checked_load(
    builder: &mut FunctionBuilder<'_>,
    vars: &[Option<Variable>],
    instr: &crate::Instruction,
    memory_ptr: Value,
    memory_len: Value,
    load_ty: Type,
    width: i64,
) {
    let address = use_reg(builder, vars, instr.b);
    let invalid_block = builder.create_block();
    let len_ok_block = builder.create_block();
    let load_block = builder.create_block();
    let cont_block = builder.create_block();
    builder.append_block_param(cont_block, types::I64);

    let len_ok = builder
        .ins()
        .icmp_imm(IntCC::UnsignedGreaterThanOrEqual, memory_len, width);
    builder
        .ins()
        .brif(len_ok, len_ok_block, &[], invalid_block, &[]);

    builder.switch_to_block(len_ok_block);
    let max_start = builder.ins().iadd_imm(memory_len, -width);
    let address_ok = builder
        .ins()
        .icmp(IntCC::UnsignedLessThanOrEqual, address, max_start);
    builder
        .ins()
        .brif(address_ok, load_block, &[], invalid_block, &[]);

    builder.switch_to_block(load_block);
    let ptr = builder.ins().iadd(memory_ptr, address);
    let loaded = builder.ins().load(load_ty, MemFlags::new(), ptr, 0);
    let loaded = if load_ty == types::I64 {
        loaded
    } else {
        builder.ins().uextend(types::I64, loaded)
    };
    builder.ins().jump(cont_block, &[BlockArg::Value(loaded)]);

    builder.switch_to_block(invalid_block);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().jump(cont_block, &[BlockArg::Value(zero)]);

    builder.switch_to_block(cont_block);
    let value = builder.block_params(cont_block)[0];
    def_reg(builder, vars, instr.a, value);
}

/// Генерирует проверенную запись в память.
///
/// Если запись выходит за пределы допустимого диапазона памяти, операция
/// просто пропускается, сохраняя поведение безопасной VM-имплементации.
fn emit_checked_store(
    builder: &mut FunctionBuilder<'_>,
    vars: &[Option<Variable>],
    instr: &crate::Instruction,
    memory_ptr: Value,
    memory_len: Value,
    store_ty: Type,
    width: i64,
) {
    let address = use_reg(builder, vars, instr.a);
    let value = use_reg(builder, vars, instr.b);
    let invalid_block = builder.create_block();
    let len_ok_block = builder.create_block();
    let store_block = builder.create_block();
    let cont_block = builder.create_block();

    let len_ok = builder
        .ins()
        .icmp_imm(IntCC::UnsignedGreaterThanOrEqual, memory_len, width);
    builder
        .ins()
        .brif(len_ok, len_ok_block, &[], invalid_block, &[]);

    builder.switch_to_block(len_ok_block);
    let max_start = builder.ins().iadd_imm(memory_len, -width);
    let address_ok = builder
        .ins()
        .icmp(IntCC::UnsignedLessThanOrEqual, address, max_start);
    builder
        .ins()
        .brif(address_ok, store_block, &[], invalid_block, &[]);

    builder.switch_to_block(store_block);
    let ptr = builder.ins().iadd(memory_ptr, address);
    let value = if store_ty == types::I64 {
        value
    } else {
        builder.ins().ireduce(store_ty, value)
    };
    builder.ins().store(MemFlags::new(), value, ptr, 0);
    builder.ins().jump(cont_block, &[]);

    builder.switch_to_block(invalid_block);
    builder.ins().jump(cont_block, &[]);

    builder.switch_to_block(cont_block);
}

/// Преобразует 64-битное целочисленное значение в f64 битовой интерпретацией.
fn int_to_float(builder: &mut FunctionBuilder<'_>, value: Value) -> Value {
    builder.ins().bitcast(types::F64, MemFlags::new(), value)
}

/// Преобразует 64-битное представление f64 обратно в целочисленный регистр.
fn float_to_int(builder: &mut FunctionBuilder<'_>, value: Value) -> Value {
    builder.ins().bitcast(types::I64, MemFlags::new(), value)
}

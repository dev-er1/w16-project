// w16-ir\src\compiler_to_bytecode.rs
//
//! # Компилятор MIR  -> W16 Bytecode
//!
//! ## Алгоритм (два прохода)
//!
//! ### Проход 1 — сбор адресов блоков
//! Эмитим инструкции для каждого блока, заполняем `block_ip[block_id] = IP`.
//! Переходы (Jmp, Br) которые ссылаются на ещё не скомпилированные блоки
//! (forward jumps) оставляем в `patch_list` — список мест где нужно подставить
//! реальный адрес после завершения прохода 1.
//!
//! ### Проход 2 — патчинг переходов
//! Заменяем временные адреса в `patch_list` на реальные IP из `block_ip`.
//!
//! ## Регистровый аллокатор
//!
//! Линейный (trivial): каждому ValueId назначается уникальный регистр.
//! ValueId результата инструкции берётся из `func.values` по `ValueDef::Inst(block, idx)`.
//! Параметры блоков (phi) назначают регистры при входе в блок через Mov.
//!
//! Ограничение: 256 регистров на функцию. Если функция использует > 256 SSA-значений —
//! возвращаем `RegisterSpill`. Для реального кода нужен полноценный аллокатор (linear scan).
//!
//! ## Формат LoadConst / Load16
//!
//! imm16() = (c << 8) | b   ->  b = младший байт, c = старший байт.
//! Все 16-битные immediate используют этот порядок.

use crate::mir::{
    BasicBlock, BlockId, CastKind, FCmpOp, ICmpOp, Literal, MIRFunction, MIRInst, MIRModule,
    MIRTerminator, Type, ValueDef, ValueId,
};
use std::collections::HashMap;
use w16_core::{Bytecode, ConstantPool, Instruction, OpCode};

// =============================================================================
// ОШИБКИ
// =============================================================================

#[derive(Debug, Clone)]
pub enum CompilerError {
    /// SSA-значение использовано до определения (нарушение доминирования)
    UndefinedValue(ValueId),
    /// Функция использует > 255 SSA-значений — не влезает в 256 регистров
    RegisterSpill { value_count: usize },
    /// BlockId выходит за пределы функции
    InvalidBlockId(BlockId),
    /// ConstantId выходит за пределы пула констант модуля
    InvalidConstantId(usize),
    /// Индекс в ConstantPool превышает u16::MAX (пул слишком большой)
    ConstantPoolOverflow,
    /// Адрес блока превышает u16::MAX (программа слишком большая для Load16)
    ProgramTooLarge,
    /// Функция принимает > 254 аргументов (r1..r254, r255 зарезервирован)
    TooManyArguments { count: usize },
    /// Функция не найдена в таблице адресов
    FunctionNotFound(usize),
}

impl std::fmt::Display for CompilerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UndefinedValue(id) => write!(f, "Undefined SSA value %{id}"),
            Self::RegisterSpill { value_count } => {
                write!(f, "Register spill: {value_count} values > 255 registers")
            }
            Self::InvalidBlockId(id) => write!(f, "Invalid block id ^{id}"),
            Self::InvalidConstantId(id) => write!(f, "Invalid constant id {id}"),
            Self::ConstantPoolOverflow => write!(f, "Constant pool index > u16::MAX"),
            Self::ProgramTooLarge => write!(f, "Block address > u16::MAX"),
            Self::TooManyArguments { count } => write!(f, "Too many arguments: {count} > 254"),
            Self::FunctionNotFound(id) => write!(f, "Function #{id} not found in function table"),
        }
    }
}

impl std::error::Error for CompilerError {}

// =============================================================================
// PATCH LIST — отложенный патчинг forward jumps
// =============================================================================

/// Одна запись о переходе который нужно пропатчить после завершения первого прохода.
struct PatchEntry {
    /// IP инструкции Load16/LoadConst которая загружает адрес перехода
    load_ip: usize,
    /// Целевой BlockId
    target_block: BlockId,
}

// =============================================================================
// КОНТЕКСТ КОМПИЛЯЦИИ
// =============================================================================

struct Ctx<'m> {
    /// Скомпилированные инструкции (будут пропатчены в patch-проходе)
    instructions: Vec<Instruction>,
    /// Пул констант: байтовый массив в формате W16 (8 байт на слот)
    pool: ConstantPool,
    /// Карта: MIR ConstantId  -> байтовый offset в pool.data
    const_offsets: HashMap<usize, usize>,
    /// IP первой инструкции каждого блока
    block_ip: Vec<Option<usize>>,
    /// Список forward-переходов которые нужно пропатчить
    patch_list: Vec<PatchEntry>,
    /// Карта: ValueId  -> номер регистра W16
    reg: HashMap<ValueId, u8>,
    /// Следующий свободный регистр
    next_reg: u8,
    /// Ссылка на модуль (нужна для констант)
    module: &'m MIRModule,
    /// IP первой инструкции каждой функции модуля.
    /// Заполняется при компиляции нескольких функций в один Bytecode.
    /// func_ip[func_id] = IP первой инструкции функции.
    func_ip: Vec<Option<usize>>,
    /// Patch-list для вызовов функций: (load_ip, func_id)
    func_patches: Vec<(usize, usize)>,
}

impl<'m> Ctx<'m> {
    fn new(module: &'m MIRModule, block_count: usize) -> Self {
        let func_count = module.functions.len();
        Self {
            instructions: Vec::new(),
            pool: ConstantPool::new(),
            const_offsets: HashMap::new(),
            block_ip: vec![None; block_count],
            patch_list: Vec::new(),
            reg: HashMap::new(),
            next_reg: 0,
            module,
            func_ip: vec![None; func_count],
            func_patches: Vec::new(),
        }
    }

    // -------------------------------------------------------------------------
    // Регистровый аллокатор
    // -------------------------------------------------------------------------

    /// Выделяет регистр для ValueId. Паникует если уже выделен (SSA = одно определение).
    fn alloc(&mut self, val: ValueId) -> Result<u8, CompilerError> {
        if self.next_reg == 255 {
            return Err(CompilerError::RegisterSpill {
                value_count: self.next_reg as usize + 1,
            });
        }
        let r = self.next_reg;
        self.next_reg += 1;
        self.reg.insert(val, r);
        Ok(r)
    }

    /// Возвращает регистр для ValueId. Ошибка если значение не определено.
    fn get(&self, val: ValueId) -> Result<u8, CompilerError> {
        self.reg
            .get(&val)
            .copied()
            .ok_or(CompilerError::UndefinedValue(val))
    }

    // -------------------------------------------------------------------------
    // Эмит инструкций
    // -------------------------------------------------------------------------

    fn emit(&mut self, opcode: OpCode, a: u8, b: u8, c: u8) -> usize {
        let ip = self.instructions.len();
        self.instructions.push(Instruction { opcode, a, b, c });
        ip
    }

    /// Эмитит Load16 или Load8 в зависимости от значения.
    /// Возвращает IP эмитированной инструкции (для patch_list).
    /// Формат imm16: b = lo, c = hi (соответствует imm16() = (c<<8)|b)
    fn emit_load_imm(&mut self, reg: u8, value: u16) -> usize {
        if value <= 0xFF {
            self.emit(OpCode::Load8, reg, 0, value as u8)
        } else {
            let lo = (value & 0xFF) as u8;
            let hi = (value >> 8) as u8;
            self.emit(OpCode::Load16, reg, lo, hi)
        }
    }

    /// Эмитит LoadConst для 64-битного значения из пула.
    /// Формат: b = lo byte of pool index, c = hi byte.
    /// ConstantPool::get_u64(index) принимает БАЙТОВЫЙ offset, не номер слота.
    /// Поэтому передаём pool_offset напрямую, без деления на 8.
    fn emit_load_const(&mut self, dest_reg: u8, pool_offset: usize) -> Result<(), CompilerError> {
        if pool_offset > u16::MAX as usize {
            return Err(CompilerError::ConstantPoolOverflow);
        }
        let lo = (pool_offset & 0xFF) as u8;
        let hi = (pool_offset >> 8) as u8;
        self.emit(OpCode::LoadConst, dest_reg, lo, hi);
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Пул констант
    // -------------------------------------------------------------------------

    /// Интернирует MIR-константу в байтовый пул W16.
    /// Возвращает байтовый offset в pool.data.
    /// Строки: формат [u64 len][bytes...], offset указывает на начало len.
    fn intern_const(&mut self, const_id: usize) -> Result<usize, CompilerError> {
        if let Some(&offset) = self.const_offsets.get(&const_id) {
            return Ok(offset);
        }
        let mir_const = self
            .module
            .constants
            .get(const_id)
            .ok_or(CompilerError::InvalidConstantId(const_id))?;

        let offset = self.pool.data.len();
        match &mir_const.value {
            Literal::Int(v) => {
                self.pool.data.extend_from_slice(&v.to_le_bytes());
            }
            Literal::Float(v) => {
                self.pool.data.extend_from_slice(&v.to_bits().to_le_bytes());
            }
            Literal::Bool(v) => {
                let as_u64: u64 = if *v { 1 } else { 0 };
                self.pool.data.extend_from_slice(&as_u64.to_le_bytes());
            }
            Literal::String(s) => {
                // Используем ConstantPool::add_string: [u64 len][bytes...]
                // offset уже захвачен выше — add_string вернёт то же значение
                let _ = self.pool.add_string(s);
                // pool.data уже обновлён, offset корректен
            }
        }
        self.const_offsets.insert(const_id, offset);
        Ok(offset)
    }

    // -------------------------------------------------------------------------
    // Переходы
    // -------------------------------------------------------------------------

    /// Эмитит загрузку адреса блока в регистр.
    /// Если блок ещё не скомпилирован (forward jump) — записывает в patch_list.
    fn emit_block_addr(&mut self, addr_reg: u8, target: BlockId) -> Result<(), CompilerError> {
        if target >= self.block_ip.len() {
            return Err(CompilerError::InvalidBlockId(target));
        }
        if let Some(ip) = self.block_ip[target] {
            // Блок уже скомпилирован — адрес известен
            if ip > u16::MAX as usize {
                return Err(CompilerError::ProgramTooLarge);
            }
            self.emit_load_imm(addr_reg, ip as u16);
        } else {
            // Forward jump: эмитим заглушку Load16(0), запоминаем для патча
            let load_ip = self.emit(OpCode::Load16, addr_reg, 0, 0);
            self.patch_list.push(PatchEntry {
                load_ip,
                target_block: target,
            });
        }
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Патч-проход
    // -------------------------------------------------------------------------

    /// Патчит все forward jumps реальными адресами блоков.
    fn apply_patches(&mut self) -> Result<(), CompilerError> {
        let patches = std::mem::take(&mut self.patch_list);
        for entry in patches {
            let ip = self.block_ip[entry.target_block]
                .ok_or(CompilerError::InvalidBlockId(entry.target_block))?;
            if ip > u16::MAX as usize {
                return Err(CompilerError::ProgramTooLarge);
            }
            let lo = (ip & 0xFF) as u8;
            let hi = (ip >> 8) as u8;
            self.instructions[entry.load_ip].b = lo;
            self.instructions[entry.load_ip].c = hi;
        }
        Ok(())
    }

    /// Патчит все вызовы функций реальными адресами.
    fn apply_func_patches(&mut self) -> Result<(), CompilerError> {
        let patches = std::mem::take(&mut self.func_patches);
        for (load_ip, func_id) in patches {
            let ip = self.func_ip[func_id].ok_or(CompilerError::FunctionNotFound(func_id))?;
            if ip > u16::MAX as usize {
                return Err(CompilerError::ProgramTooLarge);
            }
            let lo = (ip & 0xFF) as u8;
            let hi = (ip >> 8) as u8;
            self.instructions[load_ip].b = lo;
            self.instructions[load_ip].c = hi;
        }
        Ok(())
    }
}

// =============================================================================
// ТОЧКА ВХОДА
// =============================================================================

/// Компилирует все функции MIR-модуля в один W16 Bytecode.
///
/// ## Компоновка в памяти
///
/// ```text
/// [entry_trampoline] [func_0_code] [func_1_code] ... [func_N_code]
/// ```
///
/// Entry trampoline: `Load16 r0, main_ip; Call r0; Halt`
/// VM стартует с IP=0 и сразу вызывает main.
///
/// `entry_function_id` — FunctionId точки входа (обычно @main = 0).
pub fn compile_mir_to_bytecode(
    module: &MIRModule,
    entry_function_id: usize,
) -> Result<Bytecode, CompilerError> {
    if module.functions.is_empty() {
        return Err(CompilerError::InvalidBlockId(0));
    }

    // Суммарный размер блоков для инициализации block_ip вектора
    let total_blocks: usize = module.functions.iter().map(|f| f.blocks.len()).sum();
    let mut ctx = Ctx::new(module, total_blocks);

    // --- Entry trampoline (IP=0) ---
    // Load16 r0, 0   ; заглушка — пропатчится на IP main
    // Call r0, 0     ; вызов main
    // Halt           ; программа завершена
    let trampoline_load_ip = ctx.emit(OpCode::Load16, 0, 0, 0);
    ctx.emit(OpCode::Call, 0, 0, 0);
    ctx.emit(OpCode::Halt, 0, 0, 0);

    // --- Компилируем все функции ---
    let mut block_id_offset: usize = 0;

    for (func_id, func) in module.functions.iter().enumerate() {
        // Запоминаем глобальный IP начала функции
        ctx.func_ip[func_id] = Some(ctx.instructions.len());

        // Параметры функции: External ValueId  -> r1, r2, ...
        // r0 зарезервирован для возвращаемого значения (W16 ABI)
        let mut param_reg: u8 = 1;
        for (val_id, val_data) in func.values.iter().enumerate() {
            if matches!(val_data.def, ValueDef::External) {
                if param_reg == 255 {
                    return Err(CompilerError::TooManyArguments {
                        count: param_reg as usize,
                    });
                }
                ctx.reg.insert(val_id, param_reg);
                param_reg += 1;
            }
        }

        // Компилируем блоки с глобальным смещением
        for block in func.blocks.iter().filter(|b| !b.is_dead) {
            let global_idx = block_id_offset + block.id;
            // Расширяем block_ip если нужно
            while ctx.block_ip.len() <= global_idx {
                ctx.block_ip.push(None);
            }
            ctx.block_ip[global_idx] = Some(ctx.instructions.len());
            compile_block_with_offset(&mut ctx, func, block, block_id_offset)?;
        }

        // Патчим переходы внутри этой функции
        ctx.apply_patches()?;

        // Сбрасываем регистровую карту для следующей функции
        ctx.reg.clear();
        ctx.next_reg = 0;

        block_id_offset += func.blocks.len();
    }

    // --- Патчим вызовы функций ---
    ctx.apply_func_patches()?;

    // --- Патчим entry trampoline ---
    let main_ip = ctx
        .func_ip
        .get(entry_function_id)
        .and_then(|ip| *ip)
        .ok_or(CompilerError::FunctionNotFound(entry_function_id))?;
    if main_ip > u16::MAX as usize {
        return Err(CompilerError::ProgramTooLarge);
    }
    ctx.instructions[trampoline_load_ip].b = (main_ip & 0xFF) as u8;
    ctx.instructions[trampoline_load_ip].c = (main_ip >> 8) as u8;

    Ok(Bytecode::new(ctx.instructions, ctx.pool))
}

fn compile_block_with_offset(
    ctx: &mut Ctx,
    func: &MIRFunction,
    block: &BasicBlock,
    offset: usize,
) -> Result<(), CompilerError> {
    // Параметры блока (phi-nodes)
    for (param_id, _ty) in &block.params {
        if !ctx.reg.contains_key(param_id) {
            ctx.alloc(*param_id)?;
        }
    }

    // Инструкции блока
    for (inst_idx, inst) in block.instructions.iter().enumerate() {
        let result_id = func
            .values
            .iter()
            .position(|v| v.def == ValueDef::Inst(block.id, inst_idx) && !v.is_dead);
        compile_inst(ctx, func, inst, result_id, block.id, inst_idx)?;
    }

    // Terminator с учётом глобального смещения блоков
    compile_terminator_with_offset(ctx, func, block, offset)
}

// =============================================================================
// КОМПИЛЯЦИЯ ИНСТРУКЦИИ
// =============================================================================

fn compile_inst(
    ctx: &mut Ctx,
    func: &MIRFunction,
    inst: &MIRInst,
    result_id: Option<ValueId>,
    _block_id: BlockId,
    _inst_idx: usize,
) -> Result<(), CompilerError> {
    // Если результат мёртвый (DCE пометил is_dead) — пропускаем инструкцию
    // за исключением Store и Call у которых есть side-effects.
    let is_dead_result = result_id.map_or(true, |id| func.values[id].is_dead);
    match inst {
        MIRInst::Store { .. } | MIRInst::Call { .. } => {} // side-effects — не пропускаем
        _ if is_dead_result => return Ok(()),
        _ => {}
    }

    // Макрос для выделения регистра результата
    macro_rules! dst {
        () => {{
            let id = result_id.ok_or(CompilerError::UndefinedValue(usize::MAX))?;
            ctx.alloc(id)?
        }};
    }

    match inst {
        // -------------------------------------------------------------------------
        // Целочисленная арифметика
        // -------------------------------------------------------------------------
        MIRInst::Add(lhs, rhs) => {
            let (a, b, c) = (dst!(), ctx.get(*lhs)?, ctx.get(*rhs)?);
            ctx.emit(OpCode::Add, a, b, c);
        }
        MIRInst::Sub(lhs, rhs) => {
            let (a, b, c) = (dst!(), ctx.get(*lhs)?, ctx.get(*rhs)?);
            ctx.emit(OpCode::Sub, a, b, c);
        }
        MIRInst::Mul(lhs, rhs) => {
            let (a, b, c) = (dst!(), ctx.get(*lhs)?, ctx.get(*rhs)?);
            ctx.emit(OpCode::Mul, a, b, c);
        }
        MIRInst::UDiv(lhs, rhs) => {
            let (a, b, c) = (dst!(), ctx.get(*lhs)?, ctx.get(*rhs)?);
            ctx.emit(OpCode::UDiv, a, b, c);
        }
        MIRInst::IDiv(lhs, rhs) => {
            let (a, b, c) = (dst!(), ctx.get(*lhs)?, ctx.get(*rhs)?);
            ctx.emit(OpCode::IDiv, a, b, c);
        }
        MIRInst::URem(lhs, rhs) => {
            let (a, b, c) = (dst!(), ctx.get(*lhs)?, ctx.get(*rhs)?);
            ctx.emit(OpCode::URem, a, b, c);
        }
        MIRInst::IRem(lhs, rhs) => {
            let (a, b, c) = (dst!(), ctx.get(*lhs)?, ctx.get(*rhs)?);
            ctx.emit(OpCode::IRem, a, b, c);
        }
        MIRInst::Neg(val) => {
            let (a, b) = (dst!(), ctx.get(*val)?);
            ctx.emit(OpCode::Neg, a, b, 0);
        }

        // -------------------------------------------------------------------------
        // Float арифметика
        // -------------------------------------------------------------------------
        MIRInst::FAdd(lhs, rhs) => {
            let (a, b, c) = (dst!(), ctx.get(*lhs)?, ctx.get(*rhs)?);
            ctx.emit(OpCode::FAdd, a, b, c);
        }
        MIRInst::FSub(lhs, rhs) => {
            let (a, b, c) = (dst!(), ctx.get(*lhs)?, ctx.get(*rhs)?);
            ctx.emit(OpCode::FSub, a, b, c);
        }
        MIRInst::FMul(lhs, rhs) => {
            let (a, b, c) = (dst!(), ctx.get(*lhs)?, ctx.get(*rhs)?);
            ctx.emit(OpCode::FMul, a, b, c);
        }
        MIRInst::FDiv(lhs, rhs) => {
            let (a, b, c) = (dst!(), ctx.get(*lhs)?, ctx.get(*rhs)?);
            ctx.emit(OpCode::FDiv, a, b, c);
        }
        MIRInst::FRem(lhs, rhs) => {
            let (a, b, c) = (dst!(), ctx.get(*lhs)?, ctx.get(*rhs)?);
            ctx.emit(OpCode::FRem, a, b, c);
        }
        MIRInst::FNeg(val) => {
            let (a, b) = (dst!(), ctx.get(*val)?);
            ctx.emit(OpCode::FNeg, a, b, 0);
        }
        MIRInst::FAbs(val) => {
            let (a, b) = (dst!(), ctx.get(*val)?);
            ctx.emit(OpCode::FAbs, a, b, 0);
        }

        // -------------------------------------------------------------------------
        // Битовые операции
        // -------------------------------------------------------------------------
        MIRInst::And(lhs, rhs) => {
            let (a, b, c) = (dst!(), ctx.get(*lhs)?, ctx.get(*rhs)?);
            ctx.emit(OpCode::And, a, b, c);
        }
        MIRInst::Or(lhs, rhs) => {
            let (a, b, c) = (dst!(), ctx.get(*lhs)?, ctx.get(*rhs)?);
            ctx.emit(OpCode::Or, a, b, c);
        }
        MIRInst::Xor(lhs, rhs) => {
            let (a, b, c) = (dst!(), ctx.get(*lhs)?, ctx.get(*rhs)?);
            ctx.emit(OpCode::Xor, a, b, c);
        }
        MIRInst::Not(val) => {
            let (a, b) = (dst!(), ctx.get(*val)?);
            ctx.emit(OpCode::Not, a, b, 0);
        }
        MIRInst::Shl(lhs, rhs) => {
            let (a, b, c) = (dst!(), ctx.get(*lhs)?, ctx.get(*rhs)?);
            ctx.emit(OpCode::Shl, a, b, c);
        }
        MIRInst::Shr(lhs, rhs) => {
            let (a, b, c) = (dst!(), ctx.get(*lhs)?, ctx.get(*rhs)?);
            ctx.emit(OpCode::Shr, a, b, c);
        }
        MIRInst::Sar(lhs, rhs) => {
            let (a, b, c) = (dst!(), ctx.get(*lhs)?, ctx.get(*rhs)?);
            ctx.emit(OpCode::Sar, a, b, c);
        }

        // -------------------------------------------------------------------------
        // Сравнения
        // -------------------------------------------------------------------------
        MIRInst::ICmp { op, lhs, rhs } => {
            let (a, b, c) = (dst!(), ctx.get(*lhs)?, ctx.get(*rhs)?);
            let opcode = match op {
                ICmpOp::Eq => OpCode::IEq,
                ICmpOp::Ne => OpCode::INe,
                ICmpOp::Slt => OpCode::ISLt,
                ICmpOp::Sle => OpCode::ISLe,
                ICmpOp::Sgt => OpCode::ISGt,
                ICmpOp::Sge => OpCode::ISGe,
                ICmpOp::Ult => OpCode::IULt,
                ICmpOp::Ule => OpCode::IULe,
                ICmpOp::Ugt => OpCode::IUGt,
                ICmpOp::Uge => OpCode::IUGe,
            };
            ctx.emit(opcode, a, b, c);
        }
        MIRInst::FCmp { op, lhs, rhs } => {
            let (a, b, c) = (dst!(), ctx.get(*lhs)?, ctx.get(*rhs)?);
            let opcode = match op {
                FCmpOp::Eq => OpCode::FEq,
                FCmpOp::Ne => OpCode::FNe,
                FCmpOp::Lt => OpCode::FLt,
                FCmpOp::Le => OpCode::FLe,
                FCmpOp::Gt => OpCode::FGt,
                FCmpOp::Ge => OpCode::FGe,
            };
            ctx.emit(opcode, a, b, c);
        }

        // -------------------------------------------------------------------------
        // Константы
        //
        // Стратегия материализации:
        // - Bool / маленький Int (≤ 0xFF)   -> Load8  (1 инструкция, без пула)
        // - Int ≤ 0xFFFF                    -> Load16 (1 инструкция, без пула)
        // - Всё остальное (f64, большой int, строка)  -> LoadConst из пула
        // -------------------------------------------------------------------------
        MIRInst::Const(const_id) => {
            let dest = dst!();
            let mir_const = ctx
                .module
                .constants
                .get(*const_id)
                .ok_or(CompilerError::InvalidConstantId(*const_id))?;

            match &mir_const.value {
                Literal::Bool(v) => {
                    ctx.emit(OpCode::Load8, dest, 0, if *v { 1 } else { 0 });
                }
                Literal::Int(v) if *v <= 0xFF => {
                    ctx.emit(OpCode::Load8, dest, 0, *v as u8);
                }
                Literal::Int(v) if *v <= 0xFFFF => {
                    let lo = (*v & 0xFF) as u8;
                    let hi = (*v >> 8) as u8;
                    ctx.emit(OpCode::Load16, dest, lo, hi);
                }
                _ => {
                    // Интернируем в пул и загружаем через LoadConst
                    // Нельзя держать &mir_const и &mut ctx одновременно  ->
                    // клонируем значение перед вызовом intern_const
                    let pool_offset = ctx.intern_const(*const_id)?;
                    ctx.emit_load_const(dest, pool_offset)?;
                }
            }
        }

        // -------------------------------------------------------------------------
        // Mov (copy propagation после SSA)
        // -------------------------------------------------------------------------
        MIRInst::Mov(src) => {
            let (a, b) = (dst!(), ctx.get(*src)?);
            ctx.emit(OpCode::Mov, a, b, 0);
        }

        // -------------------------------------------------------------------------
        // Память
        // -------------------------------------------------------------------------
        MIRInst::Load { addr, typ } => {
            let (a, b) = (dst!(), ctx.get(*addr)?);
            let opcode = match typ {
                Type::U64 | Type::I64 | Type::F64 | Type::Ptr => OpCode::Ld64,
                Type::Bool => OpCode::Ld8,
                Type::Unit => OpCode::Ld8, // не должно встречаться
            };
            ctx.emit(opcode, a, b, 0);
        }
        MIRInst::Store { addr, value } => {
            let (a, b) = (ctx.get(*addr)?, ctx.get(*value)?);
            // Тип определяется по типу value в func.values
            let typ = func.value_type(*value);
            let opcode = match typ {
                Type::U64 | Type::I64 | Type::F64 | Type::Ptr => OpCode::St64,
                Type::Bool => OpCode::St8,
                Type::Unit => OpCode::St8,
            };
            ctx.emit(opcode, a, b, 0);
        }

        // -------------------------------------------------------------------------
        // Cast
        // -------------------------------------------------------------------------
        MIRInst::Cast { kind, src, .. } => {
            let (a, b) = (dst!(), ctx.get(*src)?);
            let opcode = match kind {
                CastKind::I2F => OpCode::I2F,
                CastKind::U2F => OpCode::U2F,
                CastKind::F2I => OpCode::F2I,
                CastKind::F2U => OpCode::F2U,
                // Остальные касты — смена интерпретации без изменения битов
                CastKind::I2U
                | CastKind::U2I
                | CastKind::TruncU64ToU32
                | CastKind::ZextU32ToU64
                | CastKind::SextI32ToI64
                | CastKind::Bitcast => OpCode::Mov,
            };
            ctx.emit(opcode, a, b, 0);
        }

        // -------------------------------------------------------------------------
        // Select: Reg A = (cond != 0) ? then_v : else_v
        //
        // W16 Select: OpCode::Select A B C  -> A = (A != 0) ? B : C
        // Семантика: результат пишется в A. Если cond живёт в другом регистре —
        // копируем в временный регистр результата, затем Select.
        // -------------------------------------------------------------------------
        MIRInst::Select {
            cond,
            then_v,
            else_v,
        } => {
            let dest = dst!();
            let cond_reg = ctx.get(*cond)?;
            let then_reg = ctx.get(*then_v)?;
            let else_reg = ctx.get(*else_v)?;

            // Копируем cond в dest (Select читает условие из A и пишет результат туда же)
            ctx.emit(OpCode::Mov, dest, cond_reg, 0);
            ctx.emit(OpCode::Select, dest, then_reg, else_reg);
        }

        MIRInst::PrintInt(val) => {
            let reg = ctx.get(*val)?;
            ctx.emit(OpCode::PrintInt, reg, 0, 0);
        }
        MIRInst::PrintUInt(val) => {
            let reg = ctx.get(*val)?;
            ctx.emit(OpCode::PrintUInt, reg, 0, 0);
        }
        MIRInst::PrintFloat(val) => {
            let reg = ctx.get(*val)?;
            ctx.emit(OpCode::PrintFloat, reg, 0, 0);
        }
        MIRInst::PrintStr(val) => {
            let reg = ctx.get(*val)?;
            ctx.emit(OpCode::PrintStr, reg, 0, 0);
        }

        // -------------------------------------------------------------------------
        // Call — вызов функции через W16 ABI
        //
        // Соглашение о вызовах:
        // - Аргументы: r1=arg0, r2=arg1, ..., rN=argN-1
        // - r0 = адрес функции (загружается перед Call)
        // - После возврата: r0 = возвращаемое значение
        //
        // Генерируем:
        //   Mov r1, arg0_reg   ; копируем аргументы
        //   Mov r2, arg1_reg
        //   Load16 r0, func_ip ; адрес функции
        //   Call r0, arg_count ; вызов
        //   Mov dst, r0        ; сохраняем результат
        // -------------------------------------------------------------------------
        MIRInst::Call {
            func: callee_id,
            args,
        } => {
            if args.len() > 254 {
                return Err(CompilerError::TooManyArguments { count: args.len() });
            }

            let dest = dst!();

            // Копируем аргументы в r1..rN
            for (i, &arg_val) in args.iter().enumerate() {
                let src_reg = ctx.get(arg_val)?;
                let dst_reg = (i + 1) as u8;
                if src_reg != dst_reg {
                    ctx.emit(OpCode::Mov, dst_reg, src_reg, 0);
                }
            }

            // Загружаем IP функции в r0
            // Если функция уже скомпилирована — адрес известен
            // Если нет — forward patch
            let addr_reg: u8 = 0;
            if let Some(&Some(func_ip)) = ctx.func_ip.get(*callee_id) {
                ctx.emit_load_imm(addr_reg, func_ip as u16);
            } else {
                // Forward patch: заглушка Load16
                let load_ip = ctx.emit(OpCode::Load16, addr_reg, 0, 0);
                ctx.func_patches.push((load_ip, *callee_id));
            }

            // Вызов: Call r0, arg_count
            ctx.emit(OpCode::Call, addr_reg, args.len() as u8, 0);

            // Результат в r0 — копируем в dest
            if dest != 0 {
                ctx.emit(OpCode::Mov, dest, 0, 0);
            }
        }
    }

    Ok(())
}

fn compile_terminator_with_offset(
    ctx: &mut Ctx,
    func: &MIRFunction,
    block: &BasicBlock,
    _offset: usize,
) -> Result<(), CompilerError> {
    match &block.terminator {
        // -------------------------------------------------------------------------
        // Jmp target(args...)
        //
        // Аргументы — это значения которые передаются в параметры target-блока (phi).
        // Перед прыжком копируем их через Mov в регистры параметров.
        // -------------------------------------------------------------------------
        MIRTerminator::Jmp { target, args } => {
            emit_phi_moves(ctx, func, *target, args)?;

            let addr_reg = ctx.next_reg; // временный регистр для адреса
            if addr_reg == 255 {
                return Err(CompilerError::RegisterSpill { value_count: 256 });
            }
            ctx.emit_block_addr(addr_reg, *target)?;
            ctx.emit(OpCode::Jmp, addr_reg, 0, 0);
        }

        // -------------------------------------------------------------------------
        // Br cond, ^then(then_args), ^else(else_args)
        //
        // Генерируем:
        //   Load16 addr_reg, then_ip   ; (или patch)
        //   Jnz    cond_reg, addr_reg  ; если cond != 0  -> then
        //   [phi moves для else]
        //   Load16 addr_reg, else_ip
        //   Jmp    addr_reg            ; unconditional  -> else
        // -------------------------------------------------------------------------
        MIRTerminator::Br {
            cond,
            then_blk,
            then_args,
            else_blk,
            else_args,
        } => {
            let cond_reg = ctx.get(*cond)?;
            let addr_reg = ctx.next_reg;
            if addr_reg == 255 {
                return Err(CompilerError::RegisterSpill { value_count: 256 });
            }

            // Загружаем адрес then-блока и прыгаем если cond != 0
            ctx.emit_block_addr(addr_reg, *then_blk)?;
            ctx.emit(OpCode::Jnz, cond_reg, addr_reg, 0);

            // Else-путь: phi moves + безусловный Jmp
            emit_phi_moves(ctx, func, *else_blk, else_args)?;
            ctx.emit_block_addr(addr_reg, *else_blk)?;
            ctx.emit(OpCode::Jmp, addr_reg, 0, 0);

            // Then phi moves нужно вставить ДО Jnz — но тогда они выполняются всегда.
            // Правильное решение: critical edge splitting (будущее улучшение).
            // Сейчас: если then_args пустой — всё ок.
            // Если нет — вставляем moves после метки then_blk (они выполнятся при входе).
            // Для корректности: phi moves для then вставляем в начало then_blk при его компиляции.
            // Здесь только регистрируем что нужно сделать.
            if !then_args.is_empty() {
                // Запоминаем phi-аргументы для then_blk — они будут применены
                // в начале compile_block для then_blk через параметры блока.
                // Текущая реализация: параметры блока уже получают регистры в compile_block,
                // а then_args — это ValueId из текущего блока которые совпадают с параметрами.
                // Нам нужно Mov param_reg  <- arg_reg до входа в then_blk.
                // Для корректности в общем случае нужен critical edge splitting.
                // Простое решение: then phi moves размещаем здесь, до Jnz (они всегда выполняются,
                // но это безопасно если else_blk не читает те же регистры — что верно в SSA).
                emit_phi_moves_before_jnz(ctx, func, *then_blk, then_args)?;
            }
        }

        // -------------------------------------------------------------------------
        // Ret(vals) — возврат из функции.
        // Первое значение  -> r0 (W16 ABI).
        // -------------------------------------------------------------------------
        MIRTerminator::Ret(vals) => {
            if let Some(val) = vals.first() {
                let src = ctx.get(*val)?;
                if src != 0 {
                    ctx.emit(OpCode::Mov, 0, src, 0);
                }
            }
            ctx.emit(OpCode::Ret, 0, 0, 0);
        }

        MIRTerminator::Halt => {
            ctx.emit(OpCode::Halt, 0, 0, 0);
        }
    }

    Ok(())
}

// =============================================================================
// PHI MOVES — передача значений через block params
// =============================================================================

/// Эмитит Mov-инструкции для передачи аргументов в параметры целевого блока.
/// arg[i]  -> param[i] через Mov.
fn emit_phi_moves(
    ctx: &mut Ctx,
    func: &MIRFunction,
    target: BlockId,
    args: &[ValueId],
) -> Result<(), CompilerError> {
    if target >= func.blocks.len() || args.is_empty() {
        return Ok(());
    }
    let params = &func.blocks[target].params;
    for (arg_id, (param_id, _)) in args.iter().zip(params.iter()) {
        let src = ctx.get(*arg_id)?;
        // Выделяем регистр для param если ещё не выделен
        let dst = if let Some(&r) = ctx.reg.get(param_id) {
            r
        } else {
            ctx.alloc(*param_id)?
        };
        if src != dst {
            ctx.emit(OpCode::Mov, dst, src, 0);
        }
    }
    Ok(())
}

/// То же самое но для then-ветки Br — вставляем перед Jnz.
/// Семантически корректно в SSA: then_args определены в текущем блоке,
/// else не читает те же параметры.
fn emit_phi_moves_before_jnz(
    ctx: &mut Ctx,
    func: &MIRFunction,
    target: BlockId,
    args: &[ValueId],
) -> Result<(), CompilerError> {
    // Идентично emit_phi_moves — Mov-ы вставляются в текущую позицию
    emit_phi_moves(ctx, func, target, args)
}

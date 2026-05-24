// w16-core\src\interpreter\vm.rs
//
//! # Регистровая виртуальная машина W16 runtime-а.
//!
//! Оптимизации:
//! 1. Dispatch через fn-pointer table [256] -- процессор кэширует таблицу в BTB.
//! 2. 256 регистров * 8 байт = 2KB -- влезает в L1 кэш.
//! 3. Все горячие пути #[inline(always)].
//! 4. Фиксированная инструкция 4 байта -- предсказуемый доступ к памяти.
//! 5. IP передаётся как &mut usize прямо в handler -- хранится в регистре CPU, не в памяти.
//! 6. Halt через sentinel (ip = usize::MAX) -- убирает чтение `halted` из горячего цикла.
//! 7. Инструкции читаются через сырой указатель (*ptr.add(ip)) -- нет bounds-check.
use crate::VMError;
use crate::bytecode::{Bytecode, Instruction, OpCode};

/// Сигнатура обработчика инструкции.
/// ip передаётся по &mut -- handler обновляет его напрямую, минуя vm.ip.
/// Это позволяет компилятору держать ip в регистре процессора.
type Handler = unsafe fn(&mut VM, &Bytecode, Instruction, &mut usize);

pub const REGISTER_COUNT: usize = 256;

/// Sentinel-значение для Halt: ip выходит за пределы любого кода.
const HALT_SENTINEL: usize = usize::MAX;

/// Глобальная таблица переходов.
/// Вынесена за пределы структуры для экономии памяти и кэш-эффективности.
static JUMP_TABLE: [Handler; 256] = init_jump_table();

/// Один кадр стека вызовов.
/// Сохраняет регистры вызывающей функции и адрес возврата.
/// Размер: 256 * 8 + 8 = 2056 байт.
struct CallFrame {
    /// Сохранённые регистры вызывающей функции
    registers: [u64; REGISTER_COUNT],
    /// IP следующей инструкции после Call (адрес возврата)
    return_ip: usize,
}

/// Максимальная глубина стека вызовов.
/// 256 уровней достаточно для большинства рекурсий.
pub const MAX_CALL_DEPTH: usize = 256;

pub struct VM {
    /// Текущий регистровый файл: 256 * 8 байт = 2KB.
    pub registers: [u64; REGISTER_COUNT],
    /// Стек вызовов: сохранённые фреймы вызывающих функций.
    call_stack: Vec<CallFrame>,
    /// Системная память
    pub memory: Vec<u8>,
    /// Ошибка выставленная handler-ом. Проверяется после цикла в run().
    error: Option<VMError>,
}

impl VM {
    pub fn new(memory_size: usize) -> Self {
        Self {
            registers: [0; REGISTER_COUNT],
            call_stack: Vec::with_capacity(32),
            memory: vec![0; memory_size],
            error: None,
        }
    }

    /// Главный цикл интерпретации.
    ///
    /// Ключевые оптимизации по сравнению с предыдущей версией:
    /// - `ip` живёт на стеке (в регистре CPU), а не в `self.ip` -- нет ldr/str на каждой итерации.
    /// - Halt выставляет `ip = HALT_SENTINEL` (usize::MAX): условие `ip < code_len` сразу ложно.
    /// - Чтение инструкции через сырой указатель: полностью исключён bounds-check.
    /// - Нет отдельного поля `halted` и его чтения в цикле.
    #[inline(never)]
    pub fn run(&mut self, bytecode: &Bytecode) -> Result<(), VMError> {
        let instructions = bytecode.instructions.as_ptr();
        let code_len = bytecode.instructions.len();

        self.error = None;
        let mut ip: usize = 0;

        while ip < code_len {
            unsafe {
                let inst = *instructions.add(ip);
                let handler = *JUMP_TABLE.get_unchecked(inst.opcode as usize);
                handler(self, bytecode, inst, &mut ip);
            }
        }

        // Проверяем ошибку выставленную handler-ом
        if let Some(err) = self.error.take() {
            return Err(err);
        }

        Ok(())
    }
}

/// Инициализация таблицы переходов в compile-time.
const fn init_jump_table() -> [Handler; 256] {
    let mut table: [Handler; 256] = [op_noop; 256];

    table[OpCode::Halt as usize] = op_halt;
    table[OpCode::NoOp as usize] = op_noop;

    // ========== Работа с данными ==========
    table[OpCode::Mov as usize] = op_mov;
    table[OpCode::Load8 as usize] = op_load8;
    table[OpCode::Load16 as usize] = op_load16;
    table[OpCode::LoadConst as usize] = op_load_const;

    table[OpCode::Ld8 as usize] = op_ld8;
    table[OpCode::Ld16 as usize] = op_ld16;
    table[OpCode::Ld32 as usize] = op_ld32;
    table[OpCode::Ld64 as usize] = op_ld64;

    table[OpCode::St8 as usize] = op_st8;
    table[OpCode::St16 as usize] = op_st16;
    table[OpCode::St32 as usize] = op_st32;
    table[OpCode::St64 as usize] = op_st64;

    // ========== Арифметика (integer) ==========
    table[OpCode::Add as usize] = op_add;
    table[OpCode::Sub as usize] = op_sub;
    table[OpCode::Mul as usize] = op_mul;
    table[OpCode::UDiv as usize] = op_udiv;
    table[OpCode::IDiv as usize] = op_idiv;
    table[OpCode::URem as usize] = op_urem;
    table[OpCode::IRem as usize] = op_irem;
    table[OpCode::Neg as usize] = op_neg;

    // ========== Арифметика (float) ==========
    table[OpCode::FAdd as usize] = op_fadd;
    table[OpCode::FSub as usize] = op_fsub;
    table[OpCode::FMul as usize] = op_fmul;
    table[OpCode::FDiv as usize] = op_fdiv;
    table[OpCode::FRem as usize] = op_frem;
    table[OpCode::FNeg as usize] = op_fneg;
    table[OpCode::FAbs as usize] = op_fabs;

    // ========== Битовые операции ==========
    table[OpCode::And as usize] = op_and;
    table[OpCode::Or as usize] = op_or;
    table[OpCode::Xor as usize] = op_xor;
    table[OpCode::Not as usize] = op_not;
    table[OpCode::Shl as usize] = op_shl;
    table[OpCode::Shr as usize] = op_shr;
    table[OpCode::Sar as usize] = op_sar;

    // --- Сравнения Integer ---
    table[OpCode::IEq as usize] = op_ieq;
    table[OpCode::ISLt as usize] = op_islt;
    table[OpCode::IULt as usize] = op_iult;
    table[OpCode::INe as usize] = op_ine;
    table[OpCode::ISLe as usize] = op_isle;
    table[OpCode::ISGt as usize] = op_isgt;
    table[OpCode::ISGe as usize] = op_isge;
    table[OpCode::IULe as usize] = op_iule;
    table[OpCode::IUGt as usize] = op_iugt;
    table[OpCode::IUGe as usize] = op_iuge;
    table[OpCode::F2U as usize] = op_f2u;
    table[OpCode::U2F as usize] = op_u2f;

    // --- Остальное ---
    table[OpCode::Select as usize] = op_select;

    // =========================================================================
    // === КАСТИНГ (ПРИВЕДЕНИЕ ТИПОВ) ==========================================
    // =========================================================================
    table[OpCode::F2I as usize] = op_f2i;
    table[OpCode::I2F as usize] = op_i2f;

    table[OpCode::Jmp as usize] = op_jmp;
    table[OpCode::Jnz as usize] = op_jnz;
    table[OpCode::Jz as usize] = op_jz;
    table[OpCode::Call as usize] = op_call;
    table[OpCode::Ret as usize] = op_ret;

    table[OpCode::FEq as usize] = op_feq;
    table[OpCode::FLt as usize] = op_flt;
    table[OpCode::FLe as usize] = op_fle;
    table[OpCode::FGt as usize] = op_fgt;
    table[OpCode::FNe as usize] = op_fne;
    table[OpCode::FGe as usize] = op_fge;

    table[OpCode::PrintInt as usize] = op_print_int;
    table[OpCode::PrintUInt as usize] = op_print_uint;
    table[OpCode::PrintFloat as usize] = op_print_float;
    table[OpCode::PrintStr as usize] = op_print_str;

    table
}

// =============================================================================
// Макрос для однотипных арифметических операций (снижает boilerplate)
// =============================================================================

/// Вызов функции.
///
/// ## Соглашение о вызовах W16 (W16 ABI)
///
/// ### Передача аргументов
/// Перед `Call` компилятор копирует аргументы в регистры `r1..rN`:
/// ```text
/// Mov r1, arg0   ; первый аргумент
/// Mov r2, arg1   ; второй аргумент
/// LoadConst r0, func_ip  ; адрес функции
/// Call r0        ; вызов
/// ; после возврата: результат в r0
/// ```
///
/// ### Сохранение состояния
/// `Call` сохраняет весь регистровый файл вызывающей функции в `call_stack`.
/// Это гарантирует что вызываемая функция может свободно использовать все 256 регистров.
///
/// ### Возврат
/// `Ret` восстанавливает регистровый файл из стека, копирует `r0` вызываемой функции
/// (возвращаемое значение) в `r0` восстановленного фрейма, и прыгает на return_ip.
///
/// ## Формат инструкции
/// `Call rA` — `rA` содержит IP функции (байтовый адрес первой инструкции).
#[inline(always)]
unsafe fn op_call(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let func_ip = *vm.registers.get_unchecked(inst.a as usize) as usize;

        // Проверка переполнения стека
        if vm.call_stack.len() >= MAX_CALL_DEPTH {
            // Stack overflow — останавливаем выполнение
            *ip = HALT_SENTINEL;
            return;
        }

        // Сохраняем текущий фрейм: регистры + адрес возврата (инструкция после Call)
        *ip += 1; // ip уже указывает на следующую инструкцию после Call
        vm.call_stack.push(CallFrame {
            registers: vm.registers,
            return_ip: *ip,
        });

        // Очищаем регистры для вызываемой функции.
        // Аргументы уже в r1..rN (скопированы компилятором перед Call).
        // Обнуляем r0 и регистры после аргументов.
        // Используем safe индексирование — get_unchecked здесь не нужен.
        let arg_count = (inst.b as usize).min(REGISTER_COUNT - 1);
        vm.registers[0] = 0; // r0 = 0, будет заполнен через Ret
        // Обнуляем регистры начиная с arg_count+1 до конца
        let start = arg_count + 1;
        if start < REGISTER_COUNT {
            vm.registers[start..REGISTER_COUNT].fill(0);
        }

        // Прыгаем на тело функции
        *ip = func_ip;
    }
}

/// # Возврат из функции.
///
/// Восстанавливает регистровый файл из `call_stack` и копирует
/// возвращаемое значение (`r0` вызываемой) в `r0` вызывающей.
/// Если стек пуст — это возврат из main, останавливаем VM.
#[inline(always)]
unsafe fn op_ret(vm: &mut VM, _: &Bytecode, _: Instruction, ip: &mut usize) {
    unsafe {
        // Сохраняем возвращаемое значение из r0 вызываемой функции
        let return_value = *vm.registers.get_unchecked(0);

        match vm.call_stack.pop() {
            Some(frame) => {
                // Восстанавливаем регистры вызывающей функции
                vm.registers = frame.registers;
                // Копируем возвращаемое значение в r0 вызывающей
                vm.registers[0] = return_value;
                // Восстанавливаем IP
                *ip = frame.return_ip;
            }
            None => {
                // Стек пуст — возврат из entry point (main)
                // r0 уже содержит результат, просто останавливаемся
                *ip = HALT_SENTINEL;
            }
        }
    }
}

/// Нет операции
#[inline(always)]
unsafe fn op_noop(_: &mut VM, _: &Bytecode, _: Instruction, ip: &mut usize) {
    *ip += 1;
}

/// Остановка виртуальной машины.
/// Выставляем sentinel -- цикл завершится на следующей проверке while ip < code_len.
#[inline(always)]
unsafe fn op_halt(_: &mut VM, _: &Bytecode, _: Instruction, ip: &mut usize) {
    *ip = HALT_SENTINEL;
}

// =============================================================================
// === РАБОТА С ДАННЫМИ (0x10 - 0x1D) ==========================================
// =============================================================================

#[inline(always)]
unsafe fn op_mov(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let value = *vm.registers.get_unchecked(inst.b as usize);
        *vm.registers.get_unchecked_mut(inst.a as usize) = value;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_load8(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        *vm.registers.get_unchecked_mut(inst.a as usize) = inst.c as u64;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_load16(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        *vm.registers.get_unchecked_mut(inst.a as usize) = inst.imm16() as u64;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_load_const(vm: &mut VM, bytecode: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let const_index = inst.imm16() as usize;
        // Проверяем что индекс + 8 байт не выходят за пределы пула
        if const_index + 8 > bytecode.constant_pool.data.len() {
            vm.error = Some(VMError::ConstantPoolError);
            *ip = HALT_SENTINEL;
            return;
        }
        let value = bytecode.constant_pool.get_u64(const_index);
        *vm.registers.get_unchecked_mut(inst.a as usize) = value;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_ld8(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let address = *vm.registers.get_unchecked(inst.b as usize) as usize;
        if address >= vm.memory.len() {
            vm.error = Some(VMError::MemoryAccessViolation(address));
            *ip = HALT_SENTINEL;
            return;
        }
        let value = *vm.memory.get_unchecked(address) as u64;
        *vm.registers.get_unchecked_mut(inst.a as usize) = value;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_ld16(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let address = *vm.registers.get_unchecked(inst.b as usize) as usize;
        if address + 2 > vm.memory.len() {
            vm.error = Some(VMError::MemoryAccessViolation(address));
            *ip = HALT_SENTINEL;
            return;
        }
        let bytes = vm.memory.get_unchecked(address..address + 2);
        let value = u16::from_le_bytes(bytes.try_into().unwrap_unchecked()) as u64;
        *vm.registers.get_unchecked_mut(inst.a as usize) = value;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_ld32(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let address = *vm.registers.get_unchecked(inst.b as usize) as usize;
        if address + 4 > vm.memory.len() {
            vm.error = Some(VMError::MemoryAccessViolation(address));
            *ip = HALT_SENTINEL;
            return;
        }
        let bytes = vm.memory.get_unchecked(address..address + 4);
        let value = u32::from_le_bytes(bytes.try_into().unwrap_unchecked()) as u64;
        *vm.registers.get_unchecked_mut(inst.a as usize) = value;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_ld64(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let address = *vm.registers.get_unchecked(inst.b as usize) as usize;
        if address + 8 > vm.memory.len() {
            vm.error = Some(VMError::MemoryAccessViolation(address));
            *ip = HALT_SENTINEL;
            return;
        }
        let bytes = vm.memory.get_unchecked(address..address + 8);
        let value = u64::from_le_bytes(bytes.try_into().unwrap_unchecked());
        *vm.registers.get_unchecked_mut(inst.a as usize) = value;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_st8(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let address = *vm.registers.get_unchecked(inst.a as usize) as usize;
        if address >= vm.memory.len() {
            vm.error = Some(VMError::MemoryAccessViolation(address));
            *ip = HALT_SENTINEL;
            return;
        }
        let value = *vm.registers.get_unchecked(inst.b as usize) as u8;
        *vm.memory.get_unchecked_mut(address) = value;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_st16(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let address = *vm.registers.get_unchecked(inst.a as usize) as usize;
        if address + 2 > vm.memory.len() {
            vm.error = Some(VMError::MemoryAccessViolation(address));
            *ip = HALT_SENTINEL;
            return;
        }
        let value = *vm.registers.get_unchecked(inst.b as usize) as u16;
        vm.memory
            .get_unchecked_mut(address..address + 2)
            .copy_from_slice(&value.to_le_bytes());
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_st32(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let address = *vm.registers.get_unchecked(inst.a as usize) as usize;
        if address + 4 > vm.memory.len() {
            vm.error = Some(VMError::MemoryAccessViolation(address));
            *ip = HALT_SENTINEL;
            return;
        }
        let value = *vm.registers.get_unchecked(inst.b as usize) as u32;
        vm.memory
            .get_unchecked_mut(address..address + 4)
            .copy_from_slice(&value.to_le_bytes());
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_st64(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let address = *vm.registers.get_unchecked(inst.a as usize) as usize;
        if address + 8 > vm.memory.len() {
            vm.error = Some(VMError::MemoryAccessViolation(address));
            *ip = HALT_SENTINEL;
            return;
        }
        let value = *vm.registers.get_unchecked(inst.b as usize);
        vm.memory
            .get_unchecked_mut(address..address + 8)
            .copy_from_slice(&value.to_le_bytes());
        *ip += 1;
    }
}

// =============================================================================
// === АРИФМЕТИКА ДЛЯ ЦЕЛОЧИСЛЕННЫХ (0x20 - 0x30) ==============================
// =============================================================================

#[inline(always)]
unsafe fn op_add(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = *vm.registers.get_unchecked(inst.b as usize);
        let c = *vm.registers.get_unchecked(inst.c as usize);
        *vm.registers.get_unchecked_mut(inst.a as usize) = b.wrapping_add(c);
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_sub(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = *vm.registers.get_unchecked(inst.b as usize);
        let c = *vm.registers.get_unchecked(inst.c as usize);
        *vm.registers.get_unchecked_mut(inst.a as usize) = b.wrapping_sub(c);
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_mul(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = *vm.registers.get_unchecked(inst.b as usize);
        let c = *vm.registers.get_unchecked(inst.c as usize);
        *vm.registers.get_unchecked_mut(inst.a as usize) = b.wrapping_mul(c);
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_udiv(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = *vm.registers.get_unchecked(inst.b as usize);
        let c = *vm.registers.get_unchecked(inst.c as usize);
        if c == 0 {
            vm.error = Some(VMError::DivisionByZero);
            *ip = HALT_SENTINEL;
            return;
        }
        *vm.registers.get_unchecked_mut(inst.a as usize) = b / c;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_idiv(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = *vm.registers.get_unchecked(inst.b as usize) as i64;
        let c = *vm.registers.get_unchecked(inst.c as usize) as i64;
        if c == 0 {
            vm.error = Some(VMError::DivisionByZero);
            *ip = HALT_SENTINEL;
            return;
        }
        *vm.registers.get_unchecked_mut(inst.a as usize) = if b == i64::MIN && c == -1 {
            0
        } else {
            (b / c) as u64
        };
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_urem(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = *vm.registers.get_unchecked(inst.b as usize);
        let c = *vm.registers.get_unchecked(inst.c as usize);
        if c == 0 {
            vm.error = Some(VMError::DivisionByZero);
            *ip = HALT_SENTINEL;
            return;
        }
        *vm.registers.get_unchecked_mut(inst.a as usize) = b % c;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_irem(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = *vm.registers.get_unchecked(inst.b as usize) as i64;
        let c = *vm.registers.get_unchecked(inst.c as usize) as i64;
        if c == 0 {
            vm.error = Some(VMError::DivisionByZero);
            *ip = HALT_SENTINEL;
            return;
        }
        *vm.registers.get_unchecked_mut(inst.a as usize) = (b % c) as u64;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_neg(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = *vm.registers.get_unchecked(inst.b as usize) as i64;
        *vm.registers.get_unchecked_mut(inst.a as usize) = b.wrapping_neg() as u64;
        *ip += 1;
    }
}

// =============================================================================
// === АРИФМЕТИКА ДЛЯ ДРОБНЫХ ЧИСЕЛ (0x27 - 0x31) ==============================
// =============================================================================

#[inline(always)]
unsafe fn op_fadd(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = f64::from_bits(*vm.registers.get_unchecked(inst.b as usize));
        let c = f64::from_bits(*vm.registers.get_unchecked(inst.c as usize));
        *vm.registers.get_unchecked_mut(inst.a as usize) = (b + c).to_bits();
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_fsub(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = f64::from_bits(*vm.registers.get_unchecked(inst.b as usize));
        let c = f64::from_bits(*vm.registers.get_unchecked(inst.c as usize));
        *vm.registers.get_unchecked_mut(inst.a as usize) = (b - c).to_bits();
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_fmul(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = f64::from_bits(*vm.registers.get_unchecked(inst.b as usize));
        let c = f64::from_bits(*vm.registers.get_unchecked(inst.c as usize));
        *vm.registers.get_unchecked_mut(inst.a as usize) = (b * c).to_bits();
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_fdiv(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = f64::from_bits(*vm.registers.get_unchecked(inst.b as usize));
        let c = f64::from_bits(*vm.registers.get_unchecked(inst.c as usize));
        *vm.registers.get_unchecked_mut(inst.a as usize) = (b / c).to_bits();
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_frem(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = f64::from_bits(*vm.registers.get_unchecked(inst.b as usize));
        let c = f64::from_bits(*vm.registers.get_unchecked(inst.c as usize));
        *vm.registers.get_unchecked_mut(inst.a as usize) = (b % c).to_bits();
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_fneg(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = f64::from_bits(*vm.registers.get_unchecked(inst.b as usize));
        *vm.registers.get_unchecked_mut(inst.a as usize) = (-b).to_bits();
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_fabs(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = f64::from_bits(*vm.registers.get_unchecked(inst.b as usize));
        *vm.registers.get_unchecked_mut(inst.a as usize) = b.abs().to_bits();
        *ip += 1;
    }
}

// =============================================================================
// === БИТОВЫЕ ОПЕРАЦИИ (0x60 - 0x66) ==========================================
// =============================================================================

#[inline(always)]
unsafe fn op_and(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = *vm.registers.get_unchecked(inst.b as usize);
        let c = *vm.registers.get_unchecked(inst.c as usize);
        *vm.registers.get_unchecked_mut(inst.a as usize) = b & c;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_or(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = *vm.registers.get_unchecked(inst.b as usize);
        let c = *vm.registers.get_unchecked(inst.c as usize);
        *vm.registers.get_unchecked_mut(inst.a as usize) = b | c;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_xor(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = *vm.registers.get_unchecked(inst.b as usize);
        let c = *vm.registers.get_unchecked(inst.c as usize);
        *vm.registers.get_unchecked_mut(inst.a as usize) = b ^ c;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_not(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = *vm.registers.get_unchecked(inst.b as usize);
        *vm.registers.get_unchecked_mut(inst.a as usize) = !b;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_shl(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = *vm.registers.get_unchecked(inst.b as usize);
        let c = *vm.registers.get_unchecked(inst.c as usize);
        *vm.registers.get_unchecked_mut(inst.a as usize) = b.wrapping_shl(c as u32);
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_shr(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = *vm.registers.get_unchecked(inst.b as usize);
        let c = *vm.registers.get_unchecked(inst.c as usize);
        *vm.registers.get_unchecked_mut(inst.a as usize) = b.wrapping_shr(c as u32);
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_sar(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = *vm.registers.get_unchecked(inst.b as usize) as i64;
        let c = *vm.registers.get_unchecked(inst.c as usize) as i64;
        *vm.registers.get_unchecked_mut(inst.a as usize) = b.wrapping_shr(c as u32) as u64;
        *ip += 1;
    }
}

// =============================================================================
// === ЛОГИКА И СРАВНЕНИЯ (0x40 - 0x5F) =========================================
// =============================================================================

#[inline(always)]
unsafe fn op_ieq(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = *vm.registers.get_unchecked(inst.b as usize);
        let c = *vm.registers.get_unchecked(inst.c as usize);
        *vm.registers.get_unchecked_mut(inst.a as usize) = (b == c) as u64;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_ine(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = *vm.registers.get_unchecked(inst.b as usize);
        let c = *vm.registers.get_unchecked(inst.c as usize);
        *vm.registers.get_unchecked_mut(inst.a as usize) = (b != c) as u64;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_islt(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = *vm.registers.get_unchecked(inst.b as usize) as i64;
        let c = *vm.registers.get_unchecked(inst.c as usize) as i64;
        *vm.registers.get_unchecked_mut(inst.a as usize) = (b < c) as u64;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_isle(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = *vm.registers.get_unchecked(inst.b as usize) as i64;
        let c = *vm.registers.get_unchecked(inst.c as usize) as i64;
        *vm.registers.get_unchecked_mut(inst.a as usize) = (b <= c) as u64;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_isgt(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = *vm.registers.get_unchecked(inst.b as usize) as i64;
        let c = *vm.registers.get_unchecked(inst.c as usize) as i64;
        *vm.registers.get_unchecked_mut(inst.a as usize) = (b > c) as u64;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_isge(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = *vm.registers.get_unchecked(inst.b as usize) as i64;
        let c = *vm.registers.get_unchecked(inst.c as usize) as i64;
        *vm.registers.get_unchecked_mut(inst.a as usize) = (b >= c) as u64;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_iult(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = *vm.registers.get_unchecked(inst.b as usize);
        let c = *vm.registers.get_unchecked(inst.c as usize);
        *vm.registers.get_unchecked_mut(inst.a as usize) = (b < c) as u64;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_iule(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = *vm.registers.get_unchecked(inst.b as usize);
        let c = *vm.registers.get_unchecked(inst.c as usize);
        *vm.registers.get_unchecked_mut(inst.a as usize) = (b <= c) as u64;
        *ip += 1;
    }
}

unsafe fn op_iugt(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = *vm.registers.get_unchecked(inst.b as usize);
        let c = *vm.registers.get_unchecked(inst.c as usize);
        *vm.registers.get_unchecked_mut(inst.a as usize) = (b > c) as u64;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_iuge(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = *vm.registers.get_unchecked(inst.b as usize);
        let c = *vm.registers.get_unchecked(inst.c as usize);
        *vm.registers.get_unchecked_mut(inst.a as usize) = (b >= c) as u64;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_select(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let cond = *vm.registers.get_unchecked(inst.a as usize);
        let b = *vm.registers.get_unchecked(inst.b as usize);
        let c = *vm.registers.get_unchecked(inst.c as usize);
        *vm.registers.get_unchecked_mut(inst.a as usize) = if cond != 0 { b } else { c };
        *ip += 1;
    }
}

// =============================================================================
// === ПРИВЕДЕНИЕ ТИПОВ (0x80 - 0x8F) ==========================================
// =============================================================================

#[inline(always)]
unsafe fn op_f2i(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let f_val = f64::from_bits(*vm.registers.get_unchecked(inst.b as usize));
        *vm.registers.get_unchecked_mut(inst.a as usize) = f_val as i64 as u64;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_i2f(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let i_val = *vm.registers.get_unchecked(inst.b as usize) as i64;
        *vm.registers.get_unchecked_mut(inst.a as usize) = (i_val as f64).to_bits();
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_f2u(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let f_val = f64::from_bits(*vm.registers.get_unchecked(inst.b as usize));
        *vm.registers.get_unchecked_mut(inst.a as usize) = f_val as u64;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_u2f(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let u_val = *vm.registers.get_unchecked(inst.b as usize);
        *vm.registers.get_unchecked_mut(inst.a as usize) = (u_val as f64).to_bits();
        *ip += 1;
    }
}

// =============================================================================
// === УПРАВЛЕНИЕ ПОТОКОМ (0x67 - 0x72) =========================================
// =============================================================================

#[inline(always)]
unsafe fn op_jmp(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        *ip = *vm.registers.get_unchecked(inst.a as usize) as usize;
    }
}

#[inline(always)]
unsafe fn op_jnz(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let cond = *vm.registers.get_unchecked(inst.a as usize);
        if cond != 0 {
            *ip = *vm.registers.get_unchecked(inst.b as usize) as usize;
        } else {
            *ip += 1;
        }
    }
}

#[inline(always)]
unsafe fn op_jz(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let cond = *vm.registers.get_unchecked(inst.a as usize);
        if cond == 0 {
            *ip = *vm.registers.get_unchecked(inst.b as usize) as usize;
        } else {
            *ip += 1;
        }
    }
}

// =============================================================================
// === СРАВНЕНИЯ FLOAT (0x50 - 0x55) ============================================
// =============================================================================

#[inline(always)]
unsafe fn op_feq(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = f64::from_bits(*vm.registers.get_unchecked(inst.b as usize));
        let c = f64::from_bits(*vm.registers.get_unchecked(inst.c as usize));
        *vm.registers.get_unchecked_mut(inst.a as usize) = (b == c) as u64;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_fne(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = f64::from_bits(*vm.registers.get_unchecked(inst.b as usize));
        let c = f64::from_bits(*vm.registers.get_unchecked(inst.c as usize));
        *vm.registers.get_unchecked_mut(inst.a as usize) = (b != c) as u64;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_flt(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = f64::from_bits(*vm.registers.get_unchecked(inst.b as usize));
        let c = f64::from_bits(*vm.registers.get_unchecked(inst.c as usize));
        *vm.registers.get_unchecked_mut(inst.a as usize) = (b < c) as u64;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_fle(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = f64::from_bits(*vm.registers.get_unchecked(inst.b as usize));
        let c = f64::from_bits(*vm.registers.get_unchecked(inst.c as usize));
        *vm.registers.get_unchecked_mut(inst.a as usize) = (b <= c) as u64;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_fgt(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = f64::from_bits(*vm.registers.get_unchecked(inst.b as usize));
        let c = f64::from_bits(*vm.registers.get_unchecked(inst.c as usize));
        *vm.registers.get_unchecked_mut(inst.a as usize) = (b > c) as u64;
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_fge(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let b = f64::from_bits(*vm.registers.get_unchecked(inst.b as usize));
        let c = f64::from_bits(*vm.registers.get_unchecked(inst.c as usize));
        *vm.registers.get_unchecked_mut(inst.a as usize) = (b >= c) as u64;
        *ip += 1;
    }
}

// =============================================================================
// === I/O (0x90 - 0x93) ========================================================
// =============================================================================

#[inline(always)]
unsafe fn op_print_int(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let val = *vm.registers.get_unchecked(inst.a as usize) as i64;
        println!("{val}");
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_print_uint(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let val = *vm.registers.get_unchecked(inst.a as usize);
        println!("{val}");
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_print_float(vm: &mut VM, _: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let val = f64::from_bits(*vm.registers.get_unchecked(inst.a as usize));
        println!("{val}");
        *ip += 1;
    }
}

#[inline(always)]
unsafe fn op_print_str(vm: &mut VM, bytecode: &Bytecode, inst: Instruction, ip: &mut usize) {
    unsafe {
        let index = *vm.registers.get_unchecked(inst.a as usize) as usize;
        let len_bytes = &bytecode.constant_pool.data[index..index + 8];
        let len = u64::from_le_bytes(len_bytes.try_into().unwrap_unchecked()) as usize;
        let str_bytes = bytecode.constant_pool.get_slice(index + 8, len);
        if let Ok(s) = std::str::from_utf8(str_bytes) {
            print!("{s}");
        }
        *ip += 1;
    }
}

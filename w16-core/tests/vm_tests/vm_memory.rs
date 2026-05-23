// w16-core\tests\vm_tests\vm_memory.rs
//
//! Тесты инструкций работы с памятью: Ld8/16/32/64, St8/16/32/64,
//! граничные адреса, выравнивание, little-endian порядок байт.
use crate::vm_tests::helpers::{Program, run, run_err, run_err_mem, run_mem};
use crate::*;
use w16_core::VMError;
use w16_core::bytecode::OpCode;

// =============================================================================
// ST8 / LD8
// =============================================================================

#[test]
fn st8_ld8_basic() {
    let bc = Program::new()
        .load8(1, 0) // addr = 0
        .load8(2, 42) // val = 42
        .inst(OpCode::St8, 1, 2, 0)
        .inst(OpCode::Ld8, 0, 1, 0)
        .halt();
    assert_reg!(run(&bc), 0, 42);
}

#[test]
fn st8_only_lowest_byte() {
    // St8 пишет только младший байт — старшие биты обнулены при чтении
    let (p, off) = Program::new().add_u64(0xFF_AA);
    let bc = p
        .load8(1, 0)
        .load_const(2, off)
        .inst(OpCode::St8, 1, 2, 0)
        .inst(OpCode::Ld8, 0, 1, 0)
        .halt();
    // Ld8 читает 1 байт и zero-extends: должно быть 0xAA
    assert_reg!(run(&bc), 0, 0xAA);
}

#[test]
fn st8_different_addresses() {
    let bc = Program::new()
        .load8(1, 0)
        .load8(2, 10)
        .load8(3, 20)
        .load8(10, 11)
        .load8(11, 22)
        .inst(OpCode::St8, 1, 10, 0) // mem[0] = 11
        .inst(OpCode::St8, 2, 11, 0) // mem[10] = 22
        .inst(OpCode::Ld8, 0, 1, 0)
        .inst(OpCode::Ld8, 4, 2, 0)
        .halt();
    let regs = run(&bc);
    assert_reg!(regs, 0, 11);
    assert_reg!(regs, 4, 22);
}

#[test]
fn ld8_zero_initialised() {
    // Память инициализирована нулями
    let bc = Program::new()
        .load8(1, 100)
        .inst(OpCode::Ld8, 0, 1, 0)
        .halt();
    assert_reg!(run(&bc), 0, 0);
}

#[test]
fn st8_overwrite() {
    let bc = Program::new()
        .load8(1, 0)
        .load8(2, 11)
        .load8(3, 22)
        .inst(OpCode::St8, 1, 2, 0)
        .inst(OpCode::St8, 1, 3, 0)
        .inst(OpCode::Ld8, 0, 1, 0)
        .halt();
    assert_reg!(run(&bc), 0, 22);
}

#[test]
fn ld8_out_of_bounds() {
    let bc = Program::new()
        .load8(1, 255) // addr = 255 в памяти размером 64 байта
        .inst(OpCode::Ld8, 0, 1, 0)
        .halt();
    // Используем маленькую память
    let bc_built = bc;
    let err = run_err_mem(&bc_built, 64);
    assert_vm_error!(err, VMError::MemoryAccessViolation);
}

#[test]
fn st8_out_of_bounds() {
    let bc = Program::new()
        .load8(1, 200)
        .load8(2, 42)
        .inst(OpCode::St8, 1, 2, 0)
        .halt();
    let err = run_err_mem(&bc, 64);
    assert_vm_error!(err, VMError::MemoryAccessViolation);
}

// =============================================================================
// ST16 / LD16
// =============================================================================

#[test]
fn st16_ld16_basic() {
    let bc = Program::new()
        .load16(1, 0)
        .load16(2, 0x1234)
        .inst(OpCode::St16, 1, 2, 0)
        .inst(OpCode::Ld16, 0, 1, 0)
        .halt();
    assert_reg!(run(&bc), 0, 0x1234);
}

#[test]
fn st16_little_endian() {
    // 0x1234 в little-endian: mem[0]=0x34, mem[1]=0x12
    let bc = Program::new()
        .load8(1, 0)
        .load16(2, 0x1234)
        .inst(OpCode::St16, 1, 2, 0)
        // Читаем первый байт
        .inst(OpCode::Ld8, 0, 1, 0)
        .halt();
    assert_reg!(run(&bc), 0, 0x34); // lo byte
}

#[test]
fn st16_ld8_low_byte() {
    let bc = Program::new()
        .load8(1, 0)
        .load16(2, 0xABCD)
        .inst(OpCode::St16, 1, 2, 0)
        .load8(3, 1)
        .inst(OpCode::Ld8, 0, 3, 0) // второй байт
        .halt();
    assert_reg!(run(&bc), 0, 0xAB);
}

#[test]
fn ld16_out_of_bounds() {
    // Адрес + 2 > memory_len
    let bc = Program::new()
        .load8(1, 63) // последний допустимый для Ld8 в 64-байтной памяти
        .inst(OpCode::Ld16, 0, 1, 0)
        .halt();
    let err = run_err_mem(&bc, 64);
    assert_vm_error!(err, VMError::MemoryAccessViolation);
}

// =============================================================================
// ST32 / LD32
// =============================================================================

#[test]
fn st32_little_endian() {
    let (p, off) = Program::new().add_u64(0x01020304u64);
    let bc = p
        .load8(1, 0)
        .load_const(2, off)
        .inst(OpCode::St32, 1, 2, 0)
        .inst(OpCode::Ld8, 3, 1, 0) // byte 0 = 0x04
        .halt();
    assert_reg!(run(&bc), 3, 0x04);
}

#[test]
fn ld32_out_of_bounds() {
    let bc = Program::new()
        .load8(1, 62)
        .inst(OpCode::Ld32, 0, 1, 0)
        .halt();
    let err = run_err_mem(&bc, 64);
    assert_vm_error!(err, VMError::MemoryAccessViolation);
}

// =============================================================================
// ST64 / LD64
// =============================================================================

#[test]
fn st64_ld64_basic() {
    let (p, off) = Program::new().add_u64(0xCAFEBABEDEADBEEF);
    let bc = p
        .load8(1, 0)
        .load_const(2, off)
        .inst(OpCode::St64, 1, 2, 0)
        .inst(OpCode::Ld64, 0, 1, 0)
        .halt();
    assert_reg!(run(&bc), 0, 0xCAFEBABEDEADBEEFu64);
}

#[test]
fn st64_ld64_roundtrip_max() {
    let (p, off) = Program::new().add_u64(u64::MAX);
    let bc = p
        .load8(1, 0)
        .load_const(2, off)
        .inst(OpCode::St64, 1, 2, 0)
        .inst(OpCode::Ld64, 0, 1, 0)
        .halt();
    assert_reg!(run(&bc), 0, u64::MAX);
}

#[test]
fn st64_ld8_bytes_correct() {
    // Записываем 0x0102030405060708, читаем по байтам
    let (p, off) = Program::new().add_u64(0x0102030405060708u64);
    let bc = p
        .load8(1, 0)
        .load_const(2, off)
        .inst(OpCode::St64, 1, 2, 0)
        .inst(OpCode::Ld8, 3, 1, 0) // byte 0 = 0x08 (little-endian)
        .halt();
    assert_reg!(run(&bc), 3, 0x08);
}

#[test]
fn ld64_out_of_bounds() {
    let bc = Program::new()
        .load8(1, 60)
        .inst(OpCode::Ld64, 0, 1, 0)
        .halt();
    let err = run_err_mem(&bc, 64);
    assert_vm_error!(err, VMError::MemoryAccessViolation);
}

#[test]
fn st64_out_of_bounds() {
    let bc = Program::new()
        .load8(1, 60)
        .load8(2, 42)
        .inst(OpCode::St64, 1, 2, 0)
        .halt();
    let err = run_err_mem(&bc, 64);
    assert_vm_error!(err, VMError::MemoryAccessViolation);
}

// =============================================================================
// OVERLAPPING — перекрывающиеся операции
// =============================================================================

#[test]
fn overlapping_write_read_different_widths() {
    // Пишем u64 = 0x0807060504030201, читаем u32 с того же адреса
    let (p, off) = Program::new().add_u64(0x0807060504030201u64);
    let bc = p
        .load8(1, 0)
        .load_const(2, off)
        .inst(OpCode::St64, 1, 2, 0)
        .inst(OpCode::Ld32, 0, 1, 0)
        .halt();
    assert_reg!(run(&bc), 0, 0x04030201); // little-endian lo 4 bytes
}

#[test]
fn partial_overwrite() {
    // Пишем u64 = 0xFFFFFFFFFFFFFFFF, потом u8 = 0 в начало
    let (p, off) = Program::new().add_u64(u64::MAX);
    let bc = p
        .load8(1, 0)
        .load_const(2, off)
        .inst(OpCode::St64, 1, 2, 0)
        .load8(3, 0)
        .inst(OpCode::St8, 1, 3, 0) // mem[0] = 0
        .inst(OpCode::Ld64, 0, 1, 0)
        .halt();
    // mem[0..8] = [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]
    assert_reg!(run(&bc), 0, 0xFFFFFFFFFFFFFF00u64);
}

#[test]
fn sequential_memory_cells() {
    // Заполняем 8 ячеек и читаем в обратном порядке
    let bc = Program::new()
        .load8(10, 10)
        .load8(11, 20)
        .load8(12, 30)
        .load8(13, 40)
        .load8(1, 0)
        .load8(2, 1)
        .load8(3, 2)
        .load8(4, 3)
        .inst(OpCode::St8, 1, 10, 0)
        .inst(OpCode::St8, 2, 11, 0)
        .inst(OpCode::St8, 3, 12, 0)
        .inst(OpCode::St8, 4, 13, 0)
        .inst(OpCode::Ld8, 20, 4, 0)
        .inst(OpCode::Ld8, 21, 3, 0)
        .inst(OpCode::Ld8, 22, 2, 0)
        .inst(OpCode::Ld8, 23, 1, 0)
        .halt();
    let regs = run(&bc);
    assert_reg!(regs, 20, 40);
    assert_reg!(regs, 21, 30);
    assert_reg!(regs, 22, 20);
    assert_reg!(regs, 23, 10);
}

// =============================================================================
// ГРАНИЧНЫЕ АДРЕСА
// =============================================================================

#[test]
fn last_valid_byte_st8_ld8() {
    // Последний байт памяти
    let mem = 1024usize;
    let last = (mem - 1) as u16;
    let bc = Program::new()
        .load16(1, last)
        .load8(2, 99)
        .inst(OpCode::St8, 1, 2, 0)
        .inst(OpCode::Ld8, 0, 1, 0)
        .halt();
    assert_reg!(run_mem(&bc, mem), 0, 99);
}

#[test]
fn last_valid_word_st16_ld16() {
    let mem = 1024usize;
    let last = (mem - 2) as u16;
    let bc = Program::new()
        .load16(1, last)
        .load16(2, 0xBEEF)
        .inst(OpCode::St16, 1, 2, 0)
        .inst(OpCode::Ld16, 0, 1, 0)
        .halt();
    assert_reg!(run_mem(&bc, mem), 0, 0xBEEF);
}

#[test]
fn zero_address_valid() {
    let bc = Program::new()
        .load8(1, 0)
        .load8(2, 123)
        .inst(OpCode::St8, 1, 2, 0)
        .inst(OpCode::Ld8, 0, 1, 0)
        .halt();
    assert_reg!(run(&bc), 0, 123);
}

#[test]
fn address_just_past_boundary_st8() {
    let mem = 64usize;
    let bc = Program::new()
        .load8(1, 64)
        .load8(2, 1) // addr = mem_size -> invalid
        .inst(OpCode::St8, 1, 2, 0)
        .halt();
    assert_vm_error!(run_err_mem(&bc, mem), VMError::MemoryAccessViolation);
}

#[test]
fn address_just_past_boundary_ld64() {
    let mem = 64usize;
    // addr 57 + 8 = 65 > 64
    let bc = Program::new()
        .load8(1, 57)
        .inst(OpCode::Ld64, 0, 1, 0)
        .halt();
    assert_vm_error!(run_err_mem(&bc, mem), VMError::MemoryAccessViolation);
}

// =============================================================================
// COPY ЧЕРЕЗ ПАМЯТЬ
// =============================================================================

#[test]
fn memory_copy_via_load_store() {
    // Копирование блока через последовательные load/store
    let bc = Program::new()
        .load8(1, 0)
        .load8(2, 100) // src=0, dst=100
        .load8(10, 0xAA)
        .load8(11, 0xBB)
        .load8(12, 0xCC)
        .inst(OpCode::St8, 1, 10, 0) // mem[0] = AA
        // src addr
        .load8(3, 0)
        .load8(4, 1)
        .load8(5, 2)
        // dst addr
        .load8(6, 100)
        .load8(7, 101)
        .load8(8, 102)
        .inst(OpCode::Ld8, 20, 3, 0)
        .inst(OpCode::St8, 6, 20, 0)
        .inst(OpCode::Ld8, 0, 6, 0)
        .halt();
    assert_reg!(run(&bc), 0, 0xAA);
}

#[test]
fn float_roundtrip_via_memory() {
    // Сохраняем f64 в память и восстанавливаем
    let (p, off) = Program::new().add_f64(3.14159265358979);
    let bc = p
        .load8(1, 0)
        .load_const(2, off)
        .inst(OpCode::St64, 1, 2, 0)
        .inst(OpCode::Ld64, 0, 1, 0)
        .halt();
    let regs = run(&bc);
    let got = f64::from_bits(regs[0]);
    assert!((got - 3.14159265358979).abs() < 1e-15);
}

// =============================================================================
// КОНСТАНТНЫЙ ПУЛ
// =============================================================================

#[test]
fn constant_pool_multiple_slots() {
    let (p, o1) = Program::new().add_u64(111);
    let (p, o2) = p.add_u64(222);
    let (p, o3) = p.add_u64(333);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .load_const(3, o3)
        .inst(OpCode::Add, 4, 1, 2)
        .inst(OpCode::Add, 0, 4, 3)
        .halt();
    assert_reg!(run(&bc), 0, 666);
}

#[test]
fn constant_pool_error_empty_pool() {
    let bc = Program::new()
        .inst(OpCode::LoadConst, 0, 0, 0) // index=0, pool пуст
        .halt();
    assert_vm_error!(run_err(&bc), VMError::ConstantPoolError);
}

#[test]
fn constant_pool_error_out_of_range() {
    let (p, _off) = Program::new().add_u64(42); // pool = 8 байт
    // Запрашиваем offset=8, но pool[8..16] не существует
    let bc = p.inst(OpCode::LoadConst, 0, 8, 0).halt();
    assert_vm_error!(run_err(&bc), VMError::ConstantPoolError);
}

#[test]
fn constant_pool_f64_and_u64_interleaved() {
    let (p, o1) = Program::new().add_u64(100);
    let (p, o2) = p.add_f64(2.5);
    let (p, o3) = p.add_u64(50);
    let bc = p
        .load_const(1, o1)
        .load_const(2, o2)
        .load_const(3, o3)
        .inst(OpCode::I2F, 4, 1, 0) // 100.0
        .inst(OpCode::FMul, 5, 4, 2) // 250.0
        .inst(OpCode::I2F, 6, 3, 0) // 50.0
        .inst(OpCode::FAdd, 0, 5, 6) // 300.0
        .halt();
    assert_reg_f64!(run(&bc), 0, 300.0);
}

// =============================================================================
// MEMORY ISOLATION — разные части памяти не пересекаются
// =============================================================================

#[test]
fn memory_isolation_regions() {
    // Записываем в разные регионы, проверяем что они независимы
    let bc = Program::new()
        .load8(1, 0)
        .load8(2, 50)
        .load8(3, 200)
        .load8(10, 0xAA)
        .load8(11, 0xBB)
        .load8(12, 0xCC)
        .inst(OpCode::St8, 1, 10, 0)
        .inst(OpCode::St8, 2, 11, 0)
        .inst(OpCode::St8, 3, 12, 0)
        .inst(OpCode::Ld8, 20, 1, 0)
        .inst(OpCode::Ld8, 21, 2, 0)
        .inst(OpCode::Ld8, 22, 3, 0)
        .halt();
    let regs = run(&bc);
    assert_reg!(regs, 20, 0xAA);
    assert_reg!(regs, 21, 0xBB);
    assert_reg!(regs, 22, 0xCC);
}

#[test]
fn memory_is_zeroed_initially() {
    // Первые 8 адресов должны быть нулями
    let bc = Program::new()
        .load8(1, 0)
        .load8(2, 4)
        .inst(OpCode::Ld64, 3, 1, 0)
        .inst(OpCode::Ld64, 4, 2, 0)
        .inst(OpCode::Add, 0, 3, 4)
        .halt();
    assert_reg!(run(&bc), 0, 0);
}

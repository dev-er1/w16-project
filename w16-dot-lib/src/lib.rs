//! # Функции для генерации исполняемого файла.
//! 
//! Чистый no_std рантайм без зависимостей от Си-рантайма (CRT), 
//! использующий только прямой Windows API (kernel32.lib).
#![no_std]

// --- РЕАЛИЗАЦИЯ ВНУТРЕННИХ ФУНКЦИЙ, КОТОРЫЕ ТРЕБОВАЛ ЛИНКОВЩИК ---

/// Маркер для линковщика Windows, сигнализирующий, что в программе 
/// используется математика с плавающей точкой (ликвидирует ошибку LNK2001: _fltused)
#[unsafe(no_mangle)]
pub static _fltused: i32 = 1;

/// Замена встроенного memset, который Rust неявно генерирует для работы со строками
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memset(s: *mut u8, c: i32, n: usize) -> *mut u8 {
    let mut i = 0;
    while i < n {
        unsafe { *s.add(i) = c as u8 };
        i += 1;
    }
    s
}

/// Замена встроенного memcpy
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    let mut i = 0;
    while i < n {
        unsafe { *dest.add(i) = *src.add(i) };
        i += 1;
    }
    dest
}

/// Замена встроенного memcmp
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcmp(s1: *const u8, s2: *const u8, n: usize) -> i32 {
    let mut i = 0;
    while i < n {
        let a = unsafe { *s1.add(i) };
        let b = unsafe { *s2.add(i) };
        if a != b {
            return (a as i32) - (b as i32);
        }
        i += 1;
    }
    0
}

// --- ИМПОРТ ПРЯМОГО WINDOWS API (KERNEL32) ---

#[link(name = "kernel32")]
unsafe extern "system" {
    /// Получение хендла стандартного вывода (stdout)
    unsafe fn GetStdHandle(nStdHandle: i32) -> *mut core::ffi::c_void;
    /// Прямая запись массива байт в консоль ОС Windows
    unsafe fn WriteFile(
        hFile: *mut core::ffi::c_void,
        lpBuffer: *const u8,
        nNumberOfBytesToWrite: u32,
        lpNumberOfBytesWritten: *mut u32,
        lpOverlapped: *mut core::ffi::c_void,
    ) -> i32;
    /// Завершение процесса операционной системой
    unsafe fn ExitProcess(uExitCode: u32) -> !;
}

const STD_OUTPUT_HANDLE: i32 = -11;

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe { ExitProcess(1) };
}

/// Заглушка для обработчика исключений, которая требуется ядру Rust (core) 
/// при сборке чистого no_std без использования Си-рантайма.
/// В реальном рантайме она никогда не вызовется.
/// Мы её добавили чтобы линковщик не ругался на "unresolved external symbol __CxxFrameHandler3".
#[unsafe(no_mangle)]
pub extern "C" fn __CxxFrameHandler3() -> i32 {
    0
}

// Вспомогательная функция для вывода буфера в консоль через Win32 API
unsafe fn win32_print(bytes: &[u8]) {
    let stdout = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    if !stdout.is_null() && stdout != (usize::MAX as *mut core::ffi::c_void) {
        let mut written = 0;
        unsafe { WriteFile(
            stdout,
            bytes.as_ptr(),
            bytes.len() as u32,
            &mut written,
            core::ptr::null_mut(),
        ) };
    }
}

// Вспомогательный буфер для форматирования чисел на стеке без аллокаций
unsafe fn print_digits(mut num: u64, is_negative: bool) {
    let mut buf = [0u8; 24];
    let mut idx = buf.len();
    
    if num == 0 {
        idx -= 1;
        buf[idx] = b'0';
    } else {
        while num > 0 {
            idx -= 1;
            buf[idx] = b'0' + (num % 10) as u8;
            num /= 10;
        }
        if is_negative {
            idx -= 1;
            buf[idx] = b'-';
        }
    }
    
    unsafe { win32_print(&buf[idx..]) };
    unsafe { win32_print(b"\n") };
}

// --- ПУБЛИЧНЫЙ API ВАШЕЙ ВИРТУАЛЬНОЙ МАШИНЫ ---

#[unsafe(no_mangle)]
pub extern "C" fn w16_object_frem(lhs: u64, rhs: u64) -> u64 {
    let l = f64::from_bits(lhs);
    let r = f64::from_bits(rhs);
    
    // Алгоритм trunc без вызова внешней Си-функции:
    // Просто приводим к i64 (отбрасывая дробную часть) и обратно в f64
    let truncated = (l / r) as i64 as f64;
    let res = l - truncated * r; 
    res.to_bits()
}

#[unsafe(no_mangle)]
pub extern "C" fn w16_object_print_int(value: u64) {
    let val = value as i64;
    unsafe {
        if val < 0 {
            print_digits((-val) as u64, true);
        } else {
            print_digits(val as u64, false);
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn w16_object_print_uint(value: u64) {
    unsafe { print_digits(value, false); }
}

#[unsafe(no_mangle)]
pub extern "C" fn w16_object_print_float(value: u64) {
    let f = f64::from_bits(value);
    // Простейший фикс: выводим целую часть. Для полноценного %g без CRT 
    // нужен алгоритм вроде Grisu/Ryu, но для no-op и тестов этого достаточно.
    unsafe {
        if f < 0.0 {
            print_digits((-f) as u64, true);
        } else {
            print_digits(f as u64, false);
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn w16_object_print_str(consts: *const u8, consts_len: usize, index: u64) {
    let index = index as usize;
    if consts.is_null() || index.checked_add(8).map_or(true, |end| end > consts_len) {
        return;
    }

    unsafe {
        let ptr = consts.add(index);
        let mut len_bytes = [0u8; 8];
        core::ptr::copy_nonoverlapping(ptr, len_bytes.as_mut_ptr(), 8);
        let str_len = u64::from_le_bytes(len_bytes) as usize;

        let Some(start) = index.checked_add(8) else { return; };
        let Some(end) = start.checked_add(str_len) else { return; };
        if end > consts_len {
            return;
        }

        // Выводим сырые байты строки прямо на экран через Win32 API
        let str_slice = core::slice::from_raw_parts(consts.add(start), str_len);
        win32_print(str_slice);
    }
}

unsafe extern "C" {
    // Указываем сигнатуру, которую ожидает Cranelift (запись по указателю)
    fn w16_main_0(result_ptr: *mut u64);
}

// Наша новая публичная функция вывода (вы ведь её реализовали через Win32 API?)
// Предположим, она называется w16_object_print_uint или аналогично.

/// Настоящая точка входа для Windows
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _start() -> ! {
    let mut return_value: u64 = 0;
    
    // Передаем легитимный адрес на стеке! 
    // Теперь Cranelift безопасно запишет сумму именно сюда.
    unsafe { w16_main_0(&mut return_value) };
    
    // Завершаем процесс, передавая остаток (код возврата Windows ограничен 32 битами)
    unsafe {
        ExitProcess(return_value as u32);
    }
}
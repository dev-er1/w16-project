pub mod dotobj;

pub use dotobj::{AOT, AOTError};

use std::path::{Path, PathBuf};
use std::process::Command;
use target_lexicon::Triple;
use w16_core::Bytecode;

pub const AOT_COMPILER_VERSION: &str = "0.1.0";

/// Компилирует W16 байткод в исполняемый бинарник.
pub fn compile_to_executable(
    bytecode: &Bytecode,
    output_name: &str,
    target: Triple,
) -> Result<String, AOTError> {
    let output_path = Path::new(output_name);
    let obj_file = with_extension(
        output_path,
        if cfg!(target_os = "windows") {
            "obj"
        } else {
            "o"
        },
    );
    let exe_file = if cfg!(target_os = "windows") {
        with_extension(output_path, "exe")
    } else {
        output_path.to_path_buf()
    };
    let obj_file = obj_file.to_string_lossy().to_string();
    let exe_file = exe_file.to_string_lossy().to_string();

    // Шаг 1: Генерируем объектный файл через Cranelift AOT
    let aot = AOT::new(target.clone());
    aot.try_compile(bytecode, &obj_file)?;

    // Шаг 2: Вызываем линковщик в зависимости от ОС
    let mut cmd = if cfg!(target_os = "windows") {
        // Получаем путь к домашней папке пользователя (C:\Users\<Имя>)
        let user_profile =
            std::env::var("USERPROFILE").unwrap_or_else(|_| r"C:\Users\default".to_string());

        // Формируем точный путь к вашей статической библиотеке
        let runtime_lib = format!(r"{user_profile}\w16\static-lib\w16-fns.lib");
        // Запрашиваем у `cc` правильную команду для линковщика MSVC под нужный таргет.
        // Он найдет оригинальный link.exe и пробросит все переменные (PATH, LIB, INCLUDE).
        if let Some(tool) = cc::windows_registry::find_tool(target.to_string().as_str(), "link.exe")
        {
            let mut c = tool.to_command();
            c.args([
                &obj_file,
                &runtime_lib,
                "kernel32.lib",
                &("/OUT:".to_string() + &exe_file),
                "/ENTRY:_start",      // <--- Указываем вашу точку входа из dumpbin
                "/SUBSYSTEM:CONSOLE", // <--- Говорим, что это консольное приложение
                "/NODEFAULTLIB",      // <--- Отключаем стандартную линковку MSVC CRT
            ]);
            c
        } else {
            // Фолбек на случай, если Visual Studio не установлена (например, стоит MinGW gcc)
            let mut c = Command::new("link.exe");
            c.args([
                &obj_file,
                &runtime_lib,
                "kernel32.lib",
                &("/OUT:".to_string() + &exe_file),
                "/ENTRY:_start",      // <--- Указываем вашу точку входа из dumpbin
                "/SUBSYSTEM:CONSOLE", // <--- Говорим, что это консольное приложение
                "/NODEFAULTLIB",      // <--- Отключаем стандартную линковку MSVC CRT
            ]);
            c
        }
    } else {
        // Для Linux / macOS оставляем стандартный cc
        let mut c = Command::new("cc");
        c.args([&obj_file, "-o", &exe_file]);
        c
    };

    // Запускаем процесс линковки
    let status = cmd.status();

    match status {
        Ok(status) if status.success() => {
            // Удаляем временный объектный файл
            let _ = std::fs::remove_file(&obj_file);
            Ok(exe_file)
        }
        Ok(status) => Err(AOTError::Cranelift(format!(
            "Linker failed with code {:?}",
            status.code()
        ))),
        Err(e) => Err(AOTError::Cranelift(format!("failed to run linker: {e}"))),
    }
}

fn with_extension(path: &Path, extension: &str) -> PathBuf {
    let mut output = path.to_path_buf();
    output.set_extension(extension);
    output
}

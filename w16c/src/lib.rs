// w16c\src\lib.rs

pub mod dotobj;

pub use dotobj::{AOT, AOTError, OptLevel};

use std::path::{Path, PathBuf};
use std::process::Command;
use target_lexicon::Triple;
use w16_core::Bytecode;

pub const AOT_COMPILER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Компилирует W16 байткод в исполняемый бинарник.
///
/// Шаг 1: генерирует объектный файл через Cranelift.
/// Шаг 2: вызывает системный линковщик.
pub fn compile_to_executable(
    bytecode: &Bytecode,
    output_name: &str,
    target: Triple,
) -> Result<String, AOTError> {
    compile_to_executable_with_opts(bytecode, output_name, target, OptLevel::Speed)
}

/// То же что [`compile_to_executable`], но с явным уровнем оптимизации.
pub fn compile_to_executable_with_opts(
    bytecode: &Bytecode,
    output_name: &str,
    target: Triple,
    opt_level: OptLevel,
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

    let obj_str = obj_file.to_string_lossy().to_string();
    let exe_str = exe_file.to_string_lossy().to_string();

    // Имя модуля — stem выходного файла (вместо захардкоженного "w16_program").
    let module_name = output_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("w16_program");

    // Шаг 1: объектный файл.
    let aot = AOT::new(target.clone(), opt_level, module_name);
    aot.try_compile(bytecode, &obj_str)?;

    // Шаг 2: линковка.
    let mut cmd = build_linker_command(&obj_str, &exe_str, &target)?;
    let status = cmd
        .status()
        .map_err(|e| AOTError::Cranelift(format!("failed to spawn linker: {e}")))?;

    if status.success() {
        let _ = std::fs::remove_file(&obj_str);
        Ok(exe_str)
    } else {
        Err(AOTError::Cranelift(format!(
            "linker exited with code {:?}",
            status.code()
        )))
    }
}

// ---------------------------------------------------------------------------
// Построение команды линковщика
// ---------------------------------------------------------------------------

fn build_linker_command(
    obj_file: &str,
    exe_file: &str,
    target: &Triple,
) -> Result<Command, AOTError> {
    if cfg!(target_os = "windows") {
        build_msvc_linker(obj_file, exe_file, target)
    } else {
        Ok(build_unix_linker(obj_file, exe_file))
    }
}

fn build_msvc_linker(obj_file: &str, exe_file: &str, target: &Triple) -> Result<Command, AOTError> {
    let user_profile =
        std::env::var("USERPROFILE").unwrap_or_else(|_| r"C:\Users\default".to_string());
    let runtime_lib = format!(r"{user_profile}\w16\static-lib\w16-fns.lib");
    let out_arg = format!("/OUT:{exe_file}");

    let common_args = [
        obj_file,
        &runtime_lib,
        "kernel32.lib",
        &out_arg,
        "/ENTRY:_start",
        "/SUBSYSTEM:CONSOLE",
        "/NODEFAULTLIB",
    ];

    // Предпочитаем MSVC link.exe найденный через cc крейт.
    if let Some(tool) = cc::windows_registry::find_tool(target.to_string().as_str(), "link.exe") {
        let mut cmd = tool.to_command();
        cmd.args(common_args);
        Ok(cmd)
    } else {
        // Фолбек — ищем link.exe в PATH.
        let mut cmd = Command::new("link.exe");
        cmd.args(common_args);
        Ok(cmd)
    }
}

fn build_unix_linker(obj_file: &str, exe_file: &str) -> Command {
    let mut cmd = Command::new("cc");
    cmd.args([obj_file, "-o", exe_file]);
    cmd
}

// ---------------------------------------------------------------------------
// Хелпер
// ---------------------------------------------------------------------------

fn with_extension(path: &Path, extension: &str) -> PathBuf {
    let mut out = path.to_path_buf();
    out.set_extension(extension);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use w16_core::{Bytecode, ConstantPool, Instruction, OpCode};

    #[test]
    fn aot_emits_object_file_with_entrypoint() {
        let bytecode = Bytecode::new(
            vec![
                Instruction {
                    opcode: OpCode::Load8,
                    a: 0,
                    b: 0,
                    c: 42,
                },
                Instruction {
                    opcode: OpCode::Halt,
                    a: 0,
                    b: 0,
                    c: 0,
                },
            ],
            ConstantPool::new(),
        );
        let out = std::env::temp_dir().join("w16c_aot_smoke.obj");
        let out_str = out.to_string_lossy().to_string();

        let generated = AOT::new(Triple::host(), OptLevel::None, "w16c_aot_smoke")
            .try_compile(&bytecode, &out_str)
            .unwrap();

        let metadata = std::fs::metadata(&generated).unwrap();
        assert!(metadata.len() > 0);
        let _ = std::fs::remove_file(generated);
    }
}

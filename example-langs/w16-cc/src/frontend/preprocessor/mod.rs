//! # Препроцессор C.
//!
//! Отдельный проход до лексера — работает на уровне текста.
//! Принимает исходный код, возвращает препроцессированную строку.
//!
//! ## Поддерживаемые директивы
//!
//! - `#include "file.h"` — локальный include (относительно текущего файла)
//! - `#include <file.h>` — системный include (ищет в `include_paths`)
//! - `#define NAME value` — константное определение
//! - `#define NAME` — определение без значения (для `#ifdef`)
//! - `#undef NAME` — удаление определения
//! - `#ifdef NAME` / `#ifndef NAME` / `#endif` — условная компиляция
//! - `#pragma once` — защита от повторного включения
//!
//! ## Ограничения текущей версии
//! - Function-like макросы (`#define MAX(a,b)`) не поддерживаются.
//! - `#if expr`, `#elif`, `#else` не поддерживаются.
//! - Конкатенация токенов (`##`) и stringification (`#`) не поддерживаются.

pub mod error;

pub use error::{PreprocError, PreprocErrorKind};

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Максимальная глубина вложенности `#ifdef`.
const MAX_NESTING: usize = 64;

/// Максимальная глубина рекурсии `#include`.
const MAX_INCLUDE_DEPTH: usize = 32;

// ---------------------------------------------------------------------------
// Публичный API
// ---------------------------------------------------------------------------

/// Настройки препроцессора.
#[derive(Debug, Clone, Default)]
pub struct PreprocOptions {
    /// Пути поиска системных заголовков (`#include <...>`).
    pub include_paths: Vec<PathBuf>,
    /// Предопределённые макросы (эквивалент `-D` флага компилятора).
    pub defines: HashMap<String, String>,
}

impl PreprocOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_include_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.include_paths.push(path.into());
        self
    }

    pub fn with_define(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.defines.insert(name.into(), value.into());
        self
    }
}

/// Запустить препроцессор.
///
/// - `source` — исходный текст
/// - `file_path` — путь к файлу (нужен для разрешения `#include "..."`)
/// - `opts` — настройки
pub fn preprocess(
    source: &str,
    file_path: &str,
    opts: &PreprocOptions,
) -> Result<String, PreprocError> {
    let mut pp = Preprocessor::new(opts.clone());
    pp.process_source(source, file_path, 0)
}

// ---------------------------------------------------------------------------
// Внутренняя реализация
// ---------------------------------------------------------------------------

struct Preprocessor {
    opts: PreprocOptions,
    /// Определённые макросы: имя -> подстановка (пустая строка если `#define NAME`).
    defines: HashMap<String, String>,
    /// Файлы с `#pragma once` — не включаем повторно.
    once_set: HashSet<String>,
    /// Файлы в текущем стеке включений — для обнаружения циклов.
    include_stack: Vec<String>,
}

impl Preprocessor {
    fn new(opts: PreprocOptions) -> Self {
        let defines = opts.defines.clone();
        Self {
            opts,
            defines,
            once_set: HashSet::new(),
            include_stack: Vec::new(),
        }
    }

    fn process_source(
        &mut self,
        source: &str,
        file_path: &str,
        depth: usize,
    ) -> Result<String, PreprocError> {
        if depth > MAX_INCLUDE_DEPTH {
            return Err(PreprocError::new(
                0,
                file_path,
                PreprocErrorKind::IncludeCycle(file_path.to_owned()),
            ));
        }

        // Стек состояний условной компиляции.
        // `true` = текущий блок активен (выводим текст).
        let mut cond_stack: Vec<bool> = Vec::new();

        self.include_stack.push(file_path.to_owned());
        let mut output = String::with_capacity(source.len());

        for (line_num_0, line) in source.lines().enumerate() {
            let line_num = line_num_0 + 1; // 1-based

            let trimmed = line.trim_start();

            if trimmed.starts_with('#') {
                self.handle_directive(
                    trimmed,
                    line_num,
                    file_path,
                    depth,
                    &mut cond_stack,
                    &mut output,
                )?;
                // Директива заменяется пустой строкой чтобы сохранить нумерацию строк.
                output.push('\n');
            } else {
                // Обычная строка — выводим если текущий блок активен.
                if self.is_active(&cond_stack) {
                    let expanded = self.expand_macros(line);
                    output.push_str(&expanded);
                    output.push('\n');
                } else {
                    output.push('\n');
                }
            }
        }

        self.include_stack.pop();
        Ok(output)
    }

    // -----------------------------------------------------------------------
    // Обработка директив
    // -----------------------------------------------------------------------

    fn handle_directive(
        &mut self,
        line: &str,
        line_num: usize,
        file_path: &str,
        depth: usize,
        cond_stack: &mut Vec<bool>,
        output: &mut String,
    ) -> Result<(), PreprocError> {
        // Убираем `#` и ведущие пробелы после него.
        let rest = line.trim_start_matches('#').trim_start();

        // Разбиваем на директиву и аргументы.
        let (directive, args) = split_directive(rest);

        // Условные директивы обрабатываем всегда (даже в неактивном блоке).
        match directive {
            "ifdef" => {
                if cond_stack.len() >= MAX_NESTING {
                    return Err(PreprocError::new(line_num, file_path, PreprocErrorKind::TooDeepNesting));
                }
                let name = args.trim();
                let active = self.is_active(cond_stack) && self.defines.contains_key(name);
                cond_stack.push(active);
                return Ok(());
            }
            "ifndef" => {
                if cond_stack.len() >= MAX_NESTING {
                    return Err(PreprocError::new(line_num, file_path, PreprocErrorKind::TooDeepNesting));
                }
                let name = args.trim();
                let active = self.is_active(cond_stack) && !self.defines.contains_key(name);
                cond_stack.push(active);
                return Ok(());
            }
            "endif" => {
                cond_stack.pop();
                return Ok(());
            }
            "else" => {
                if let Some(top) = cond_stack.clone().last_mut() {
                    // Инвертируем только если родительский блок активен.
                    let parent_active = cond_stack.len() < 2
                        || cond_stack[cond_stack.len() - 2];
                    *top = parent_active && !*top;
                }
                return Ok(());
            }
            _ => {}
        }

        // Остальные директивы — только в активном блоке.
        if !self.is_active(cond_stack) {
            return Ok(());
        }

        match directive {
            "include" => self.handle_include(args, line_num, file_path, depth, output)?,
            "define" => self.handle_define(args, line_num, file_path)?,
            "undef" => {
                let name = args.trim();
                self.defines.remove(name);
            }
            "pragma" => {
                if args.trim() == "once" {
                    self.once_set.insert(file_path.to_owned());
                }
                // Остальные pragma молча игнорируем.
            }
            // Неизвестные директивы — молча пропускаем (совместимость).
            _ => {}
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // #include
    // -----------------------------------------------------------------------

    fn handle_include(
        &mut self,
        args: &str,
        line_num: usize,
        file_path: &str,
        depth: usize,
        output: &mut String,
    ) -> Result<(), PreprocError> {
        let args = args.trim();

        let (include_path, is_system) = if args.starts_with('"') && args.ends_with('"') {
            // #include "file.h" — локальный
            let path = &args[1..args.len() - 1];
            (path.to_owned(), false)
        } else if args.starts_with('<') && args.ends_with('>') {
            // #include <file.h> — системный
            let path = &args[1..args.len() - 1];
            (path.to_owned(), true)
        } else {
            return Err(PreprocError::new(
                line_num,
                file_path,
                PreprocErrorKind::MalformedInclude(args.to_owned()),
            ));
        };

        // Разрешаем путь к файлу.
        let resolved = if is_system {
            self.resolve_system_include(&include_path)
        } else {
            self.resolve_local_include(&include_path, file_path)
        };

        let resolved = match resolved {
            Some(p) => p,
            None => {
                if is_system {
                    // Системный заголовок не найден — молча пропускаем.
                    // (stdio.h, stdlib.h и т.д. могут отсутствовать намеренно)
                    return Ok(());
                }
                return Err(PreprocError::new(
                    line_num,
                    file_path,
                    PreprocErrorKind::IncludeNotFound(include_path),
                ));
            }
        };

        let resolved_str = resolved.to_string_lossy().to_string();

        // #pragma once — пропускаем если уже включён.
        if self.once_set.contains(&resolved_str) {
            return Ok(());
        }

        // Защита от цикла.
        if self.include_stack.contains(&resolved_str) {
            return Err(PreprocError::new(
                line_num,
                file_path,
                PreprocErrorKind::IncludeCycle(resolved_str),
            ));
        }

        // Читаем и рекурсивно обрабатываем включаемый файл.
        let content = std::fs::read_to_string(&resolved)
            .map_err(|e| PreprocError::new(
                line_num,
                file_path,
                PreprocErrorKind::IncludeReadError {
                    path: resolved_str.clone(),
                    reason: e.to_string(),
                },
            ))?;

        let included = self.process_source(&content, &resolved_str, depth + 1)?;
        output.push_str(&included);

        Ok(())
    }

    fn resolve_local_include(&self, include_path: &str, current_file: &str) -> Option<PathBuf> {
        // Сначала ищем рядом с текущим файлом.
        let current_dir = Path::new(current_file).parent()?;
        let candidate = current_dir.join(include_path);
        if candidate.exists() {
            return Some(candidate);
        }
        // Потом в путях поиска.
        self.resolve_system_include(include_path)
    }

    fn resolve_system_include(&self, include_path: &str) -> Option<PathBuf> {
        for dir in &self.opts.include_paths {
            let candidate = dir.join(include_path);
            if candidate.exists() {
                return Some(candidate);
            }
        }
        None
    }

    // -----------------------------------------------------------------------
    // #define / #undef
    // -----------------------------------------------------------------------

    fn handle_define(
        &mut self,
        args: &str,
        line_num: usize,
        file_path: &str,
    ) -> Result<(), PreprocError> {
        let args = args.trim();

        if args.is_empty() {
            return Err(PreprocError::new(
                line_num,
                file_path,
                PreprocErrorKind::MalformedDefine(args.to_owned()),
            ));
        }

        // Function-like макросы пока не поддерживаем — детектируем и пропускаем.
        if let Some(paren_pos) = args.find('(') {
            let name_part = &args[..paren_pos];
            // Если между именем и `(` нет пробела — это function-like макрос.
            if !name_part.contains(' ') {
                // Молча пропускаем — будет поддержано позже.
                return Ok(());
            }
        }

        // Разбиваем на имя и значение.
        let (name, value) = if let Some(pos) = args.find(|c: char| c.is_whitespace()) {
            let name = args[..pos].trim();
            let value = args[pos..].trim();
            (name, value.to_owned())
        } else {
            // `#define NAME` без значения.
            (args, String::new())
        };

        self.defines.insert(name.to_owned(), value);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Подстановка макросов в строке
    // -----------------------------------------------------------------------

    /// Заменяет все вхождения определённых макросов в строке.
    /// Только object-like макросы (константы) — не трогает function-like.
    fn expand_macros(&self, line: &str) -> String {
        if self.defines.is_empty() {
            return line.to_owned();
        }

        let mut result = line.to_owned();

        // Итерируем несколько раз для поддержки транзитивных подстановок
        // (например `#define B A`, `#define A 42`).
        for _ in 0..8 {
            let prev = result.clone();
            for (name, value) in &self.defines {
                if value.is_empty() {
                    continue; // флаговый дефайн без значения — не подставляем
                }
                result = replace_whole_word(&result, name, value);
            }
            if result == prev {
                break; // стабилизировалось
            }
        }

        result
    }

    // -----------------------------------------------------------------------
    // Вспомогательные
    // -----------------------------------------------------------------------

    /// Текущий блок активен если все уровни условной компиляции истинны.
    fn is_active(&self, stack: &[bool]) -> bool {
        stack.iter().all(|&v| v)
    }
}

// ---------------------------------------------------------------------------
// Свободные функции
// ---------------------------------------------------------------------------

/// Разбивает строку директивы на имя и аргументы.
/// Например `"include <stdio.h>"` -> `("include", "<stdio.h>")`.
fn split_directive(rest: &str) -> (&str, &str) {
    let rest = rest.trim();
    if let Some(pos) = rest.find(|c: char| c.is_whitespace()) {
        (&rest[..pos], rest[pos..].trim_start())
    } else {
        (rest, "")
    }
}

/// Заменяет целые слова `word` на `replacement` в строке `s`.
/// "Целое слово" — не окружённое буквами, цифрами или `_`.
fn replace_whole_word(s: &str, word: &str, replacement: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.char_indices().peekable();

    while let Some((i, _)) = chars.peek().copied() {
        if s[i..].starts_with(word) {
            let before_ok = i == 0 || {
                let prev = s[..i].chars().next_back().unwrap();
                !prev.is_alphanumeric() && prev != '_'
            };
            let after_pos = i + word.len();
            let after_ok = after_pos >= s.len() || {
                let next = s[after_pos..].chars().next().unwrap();
                !next.is_alphanumeric() && next != '_'
            };

            if before_ok && after_ok {
                result.push_str(replacement);
                // Продвигаем итератор на длину слова.
                for _ in 0..word.chars().count() {
                    chars.next();
                }
                continue;
            }
        }
        let (_, c) = chars.next().unwrap();
        result.push(c);
    }

    result
}

// ---------------------------------------------------------------------------
// Тесты
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn pp(src: &str) -> String {
        preprocess(src, "test.c", &PreprocOptions::new()).unwrap()
    }

    fn pp_with(src: &str, opts: PreprocOptions) -> String {
        preprocess(src, "test.c", &opts).unwrap()
    }

    #[test]
    fn test_define_constant() {
        let out = pp("#define N 42\nint x = N;");
        assert!(out.contains("int x = 42;"), "got: {out}");
    }

    #[test]
    fn test_define_no_value() {
        // Флаговый дефайн — не подставляется, но работает для #ifdef.
        let out = pp("#define DEBUG\nint x = 1;");
        assert!(out.contains("int x = 1;"));
    }

    #[test]
    fn test_undef() {
        let out = pp("#define N 10\n#undef N\nint x = N;");
        // После undef N не подставляется.
        assert!(out.contains("int x = N;"), "got: {out}");
    }

    #[test]
    fn test_ifdef_defined() {
        let out = pp("#define FOO\n#ifdef FOO\nint x = 1;\n#endif\n");
        assert!(out.contains("int x = 1;"), "got: {out}");
    }

    #[test]
    fn test_ifdef_not_defined() {
        let out = pp("#ifdef BAR\nint x = 1;\n#endif\n");
        assert!(!out.contains("int x = 1;"), "got: {out}");
    }

    #[test]
    fn test_ifndef_not_defined() {
        let out = pp("#ifndef GUARD_H\nint x = 1;\n#endif\n");
        assert!(out.contains("int x = 1;"), "got: {out}");
    }

    #[test]
    fn test_ifndef_defined() {
        let out = pp("#define GUARD_H\n#ifndef GUARD_H\nint x = 1;\n#endif\n");
        assert!(!out.contains("int x = 1;"), "got: {out}");
    }

    #[test]
    fn test_else_branch() {
        let out = pp("#ifdef MISSING\nint x = 1;\n#else\nint x = 2;\n#endif\n");
        assert!(!out.contains("int x = 1;"), "got: {out}");
        assert!(out.contains("int x = 2;"), "got: {out}");
    }

    #[test]
    fn test_define_substitution_in_expr() {
        let out = pp("#define MAX 100\nif (x < MAX) {}");
        assert!(out.contains("if (x < 100) {}"), "got: {out}");
    }

    #[test]
    fn test_no_partial_word_substitution() {
        // MAXIMUM не должно быть заменено на 100IMUM
        let out = pp("#define MAX 100\nint MAXIMUM = MAX;");
        assert!(out.contains("int MAXIMUM = 100;"), "got: {out}");
        assert!(!out.contains("int 100IMUM"), "got: {out}");
    }

    #[test]
    fn test_predefined_defines() {
        let opts = PreprocOptions::new().with_define("VERSION", "1");
        let out = pp_with("int v = VERSION;", opts);
        assert!(out.contains("int v = 1;"), "got: {out}");
    }

    #[test]
    fn test_system_include_not_found_is_ok() {
        // Системный заголовок которого нет — молча пропускается.
        let result = preprocess("#include <nonexistent.h>\nint x;", "test.c", &PreprocOptions::new());
        assert!(result.is_ok());
    }

    #[test]
    fn test_local_include_not_found_is_error() {
        let result = preprocess(r#"#include "missing.h""#, "test.c", &PreprocOptions::new());
        assert!(result.is_err());
    }

    #[test]
    fn test_transitive_define() {
        let out = pp("#define A 42\n#define B A\nint x = B;");
        assert!(out.contains("int x = 42;"), "got: {out}");
    }

    #[test]
    fn test_replace_whole_word() {
        assert_eq!(replace_whole_word("MAX + MAXIMUM", "MAX", "100"), "100 + MAXIMUM");
        assert_eq!(replace_whole_word("x_MAX", "MAX", "100"), "x_MAX");
        assert_eq!(replace_whole_word("MAX", "MAX", "100"), "100");
    }
}
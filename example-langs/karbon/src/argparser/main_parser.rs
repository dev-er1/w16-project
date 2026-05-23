//! Да, это просто код того же https://github.com/devnumber11/JDU/blob/main/crates/sirius/src/lib.rs, я знаю
//! но я добавил это сюда, только потому, что мне нужно было почистить компьютер, и я удалил JDU проект с диска,
//! и мне нужно поддерживать CLI karbon-а.
use std::collections::HashMap;

pub struct Clapo {
    /// Главная команда (Subcommand)
    /// Это первое слово, которое идет сразу после названия программы (jdu).
    pub subcommand: Option<String>,

    /// Флажки (Flags)
    /// Это пары "ключ -- значения".
    /// Используются когда флажку нужно уточнение
    pub flags: HashMap<String, String>,

    /// Простые включатели (Switches)
    /// Это одиночные флаги, которые либо есть, либо их нет. У них нет значения после них.
    /// Используются для режимов работы (тихий режим, версия, помощь).
    pub switches: Vec<String>,

    /// Свободный текст (Free Args)
    /// Это любые слова без черточек, которые не являются подкомандой.
    /// Обычно это пути к папкам или названия файлов, которые просто "докинули" в команду.
    pub free_args: Vec<String>,
}

impl Clapo {
    /// Точка входа всего парсера аргументов
    /// Возвращаем Option<T> потому что если человек введёт "karbon.exe"
    /// Мы должны будем вернуть ничего,
    /// а с Option<T> мы можем вернуть None.
    /// Конечно можно возвращать Result<T, E>,
    /// но всё равно.
    pub fn parse() -> Option<Self> {
        // Используем оператор '?', чтобы мгновенно выйти из функции и вернуть None,
        // если get_raw_args() не нашёл полезных аргументов
        let raw = Self::get_raw_args()?;

        let subcommand = raw.get(1).cloned();

        // Создаем пустые хранилища для результатов
        let mut flags = HashMap::new();
        let mut switches = Vec::new();
        let mut free_args = Vec::new();

        let mut iter = raw.into_iter().skip(2).peekable();
        while let Some(arg) = iter.next() {
            // Подход с match:
            match arg.as_str().trim() {
                // Случай 1: Аргумент начинается с "--" (Флаги и Переключатели)
                v if v.starts_with("--") => {
                    // Проверяем, есть ли за ним значение
                    if let Some(next_arg) = iter.next_if(|next| !next.starts_with("-")) {
                        // Если условие выполнено, next_if "съедает" аргумент и возвращает его
                        flags.insert(arg, next_arg);
                    } else {
                        // Если следующего нет или он начинается с "-", это просто switch
                        switches.push(arg);
                    }
                }

                // Случай 2: Все остальные "свободные" аргументы
                _ => {
                    free_args.push(arg);
                }
            }
        }
        Some(Self {
            subcommand,
            flags,
            switches,
            free_args,
        })
    }
    /// Тот же 'parse()', только для REPL-а
    pub fn parse_from(args: Vec<String>) -> Option<Self> {
        if args.is_empty() {
            return None;
        }

        // Теперь subcommand это индекс 0, так как в REPL мы не пишем "karbon.exe"
        let subcommand = args.first().cloned();

        let mut flags = HashMap::new();
        let mut switches = Vec::new();
        let mut free_args = Vec::new();

        // Начинаем с 1, так как 0 это подкоманда
        let mut iter = args.into_iter().skip(1).peekable();
        while let Some(arg) = iter.next() {
            if arg.starts_with("--") {
                if let Some(next_arg) = iter.next_if(|next| !next.starts_with("-")) {
                    flags.insert(arg, next_arg);
                } else {
                    switches.push(arg);
                }
            } else {
                free_args.push(arg);
            }
        }

        Some(Self {
            subcommand,
            flags,
            switches,
            free_args,
        })
    }
    /// Функция которая возвращает сырые аргументы.
    /// Простой парсинг через ``env::args().collect();``
    pub fn get_raw_args() -> Option<Vec<String>> {
        // Собираем аргументы через std::env
        let raw_args: Vec<String> = std::env::args().collect();
        // Оставляем Some только если в векторе больше 1 элемента
        // (Имя программы + хотя бы один аргумент)
        // Индекс 0 в векторе всегда имя программы("karbon.exe")
        // Индекс 1 это уже что ввёл пользователь("todo")
        Some(raw_args).filter(|a| a.len() > 1)
    }
}

//! example-langs\karbon\src\string_table.rs
//!
//! # Интернированная таблица строк
//!
//! Здесь хранятся все строки в векторе и HashMap,
//! это делает язык оптимизированее, и лучше, так как:
//!
//! 1. **Экономия памяти**: Дубликаты идентификаторов (например, имя переменной `i` в цикле)
//!    хранятся в куче в единственном экземпляре.
//! 2. **Дешевое копирование**: Вместо передачи тяжелых `String` между лексером, парсером
//!    и семантическим анализатором, мы передаем `StringId`, который занимает всего 4 байта.
//! 3. **Быстрое сравнение**: Сравнение двух имен или строк сводится к сравнению
//!    целых чисел (`u32`), вместо посимвольного сопоставления массивов байтов.
//! 4. **Стабильность ссылок**: Мы можем работать с индексами, не беспокоясь о времени жизни
//!    (&str) в структурах данных AST.
use std::collections::HashMap;

/// Само "хранилище" всех строк,
#[derive(Debug, Default)]
pub struct StringTable {
    // Вектор позволяет быстро получить строку по ID (StringId)
    strings: Vec<String>,
    // HashMap позволяет быстро проверить, была ли такая строка раньше
    lookup: HashMap<String, u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StringId(u32);

impl StringTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Помещает строку в таблицу и возвращает её ID.
    /// Если строка уже есть, просто возвращает существующий ID.
    pub fn intern(&mut self, s: &str) -> StringId {
        if let Some(&id) = self.lookup.get(s) {
            return StringId(id);
        }

        let id = self.strings.len() as u32;
        self.strings.push(s.to_string());
        self.lookup.insert(s.to_string(), id);
        StringId(id)
    }

    /// Получает строку по её ID
    pub fn resolve(&self, id: StringId) -> Option<&str> {
        self.strings.get(id.0 as usize).map(|s| s.as_str())
    }
}

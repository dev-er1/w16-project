//! *Код был взят из [karbon](../../../karbon/string_table.rs).*
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
pub struct StringId(pub u32);

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

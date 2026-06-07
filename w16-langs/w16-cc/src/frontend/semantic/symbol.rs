// w16-langs\w16-cc\src\frontend\semantic\symbol.rs
//
//! # Таблица символов.
//!
//! Реализует стековую область видимости: при входе в блок `{}`
//! добавляется новый уровень, при выходе — снимается.
//! Поиск идёт от вершины стека вниз, что обеспечивает правильное
//! затенение (shadowing) имён.

use std::collections::HashMap;
use crate::frontend::string_pool::StringId;
use crate::types::Type;

/// Вид символа.
#[derive(Debug, Clone)]
pub enum Symbol {
    /// Переменная или параметр функции.
    Var(Type),

    /// Функция: (типы параметров, тип возврата).
    Function {
        params: Vec<Type>,
        return_ty: Type,
    },

    /// Тип, объявленный через `typedef`.
    TypeAlias(Type),
}

impl Symbol {
    /// Возвращает тип значения символа (для переменных и функций).
    pub fn ty(&self) -> &Type {
        match self {
            Symbol::Var(ty) => ty,
            Symbol::Function { return_ty, .. } => return_ty,
            Symbol::TypeAlias(ty) => ty,
        }
    }
}

/// Таблица символов со стековой областью видимости.
#[derive(Debug, Default)]
pub struct SymbolTable {
    /// Стек областей видимости. Последний элемент — текущий (самый внутренний) уровень.
    scopes: Vec<HashMap<StringId, Symbol>>,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self { scopes: Vec::new() }
    }

    /// Открыть новую область видимости (вход в блок `{}`).
    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Закрыть текущую область видимости (выход из блока `{}`).
    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// Объявить символ в текущей области видимости.
    /// Возвращает `false` если имя уже занято на этом же уровне.
    pub fn declare(&mut self, id: StringId, symbol: Symbol) -> bool {
        let scope = self.scopes.last_mut()
            .expect("declare вызван без открытой области видимости");

        if scope.contains_key(&id) {
            return false; // повторное объявление
        }

        scope.insert(id, symbol);
        true
    }

    /// Найти символ по имени, начиная с текущей области видимости.
    pub fn lookup(&self, id: StringId) -> Option<&Symbol> {
        self.scopes.iter().rev().find_map(|scope| scope.get(&id))
    }

    /// Проверить, объявлен ли символ именно в текущей (верхней) области.
    pub fn declared_in_current(&self, id: StringId) -> bool {
        self.scopes.last()
            .map_or(false, |scope| scope.contains_key(&id))
    }

    /// Количество открытых уровней.
    pub fn depth(&self) -> usize {
        self.scopes.len()
    }
}
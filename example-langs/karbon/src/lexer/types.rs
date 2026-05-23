//! example-langs\karbon\src\lexer\types.rs
//!
//! # Типы данных
//!
//! Тут перечисляются типы данных, значения,
//! и функции для удобного управления типами.
use crate::string_table::StringId;

// ======================================
// Типы данных                          |
// ======================================
/// # Перечисление типов,
///
/// Мы храним только типы, а подтипы определяются уже в других перечислениях.
///
/// Важно: Type отличается от Value, тем, что он не хранит само значение.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Signed(SignedInt),
    Unsigned(UnsignedInt),
    Float(FloatNum),
    Str,
    Bool,
    Void,
}

/// Знаковые целочисленные типы
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignedInt {
    I8,
    I16,
    I32,
    I64,
}

/// Беззнаковые целочисленные типы
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsignedInt {
    U8,
    U16,
    U32,
    U64,
}

/// Дробные типы
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatNum {
    F16,
    F32,
    F64,
}

// ======================================
// Значения                             |
// ======================================
/// # Перечисление значений
///
/// Храним значение.
///
/// Отличается от Type тем, что тут мы уже храним значение, а не перечисляем тип
#[derive(Debug, Clone, PartialEq, Copy)]
pub enum Value {
    SignedVal(SIV),
    UnsignedVal(UIV),
    FloatVal(FNV),
    /// StringId — индекс, строки в таблице строк(string table)
    Str(StringId),
    Bool(bool),
    Void,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SIV {
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
} // (S)igned (I)nt (V)al сокращённо

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UIV {
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
} // (U)nsigned (I)nt (V)al сокращённо

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FNV {
    F32(f32),
    F64(f64),
} // (F)loat (N)um (V)al сокращённо

/// # impl-ы и вспомогательные функции
impl Type {
    #[inline]
    pub fn compatible_types(&self, target: &Type) -> bool {
        self == target
    }
}
impl Value {
    #[inline]
    pub fn get_type(&self) -> Type {
        match self {
            // Используем синтаксис паттернов для извлечения типа напрямую
            Value::SignedVal(siv) => match siv {
                SIV::I8(_) => Type::Signed(SignedInt::I8),
                SIV::I16(_) => Type::Signed(SignedInt::I16),
                SIV::I32(_) => Type::Signed(SignedInt::I32),
                SIV::I64(_) => Type::Signed(SignedInt::I64),
            },
            Value::UnsignedVal(uiv) => match uiv {
                UIV::U8(_) => Type::Unsigned(UnsignedInt::U8),
                UIV::U16(_) => Type::Unsigned(UnsignedInt::U16),
                UIV::U32(_) => Type::Unsigned(UnsignedInt::U32),
                UIV::U64(_) => Type::Unsigned(UnsignedInt::U64),
            },
            Value::FloatVal(fnv) => match fnv {
                FNV::F32(_) => Type::Float(FloatNum::F32),
                FNV::F64(_) => Type::Float(FloatNum::F64),
            },
            Value::Str(_) => Type::Str,
            Value::Bool(_) => Type::Bool,
            Value::Void => Type::Void,
        }
    }
}

// example-langs\w16-cc\src\frontend\semantic\error.rs
//
//! Ошибки семантического анализа.
use std::fmt;
use crate::frontend::lexer::token::Span;
use crate::frontend::string_pool::StringId;
use crate::types::Type;

/// Вид семантической ошибки.
#[derive(Debug, Clone)]
pub enum SemanticErrorKind {
    // --- Имена ---

    /// Идентификатор не объявлен в текущей области видимости.
    UndeclaredIdent(StringId),

    /// Идентификатор объявлен повторно в той же области видимости.
    Redeclaration(StringId),

    /// Вызов необъявленной функции.
    UndeclaredFunction(StringId),

    /// Попытка вызвать не-функцию.
    NotCallable(StringId),

    // --- Типы ---

    /// Несовместимые типы в бинарной операции.
    TypeMismatch { expected: Type, got: Type },

    /// Несовместимый тип в присваивании.
    AssignTypeMismatch { lhs: Type, rhs: Type },

    /// Неверный тип аргумента при вызове функции.
    ArgTypeMismatch { param: Type, got: Type, arg_index: usize },

    /// Неверное количество аргументов при вызове.
    ArgCountMismatch { expected: usize, got: usize },

    /// Унарный оператор применён к несовместимому типу.
    InvalidUnaryOp { ty: Type },

    /// Тип не поддерживает индексирование (`arr[i]`).
    NotIndexable(Type),

    /// Обращение к полю не-структуры.
    NotAStruct(Type),

    /// Поле не найдено в структуре.
    NoSuchField(StringId),

    // --- Поток управления ---

    /// `return` с выражением в функции возвращающей `void`.
    ReturnValueInVoid,

    /// `return` без выражения в функции возвращающей не-`void`.
    MissingReturnValue { expected: Type },

    /// `break` вне цикла или `switch`.
    BreakOutsideLoop,

    /// `continue` вне цикла.
    ContinueOutsideLoop,

    /// `goto` на неизвестную метку.
    UndeclaredLabel(StringId),

    // --- Прочее ---

    /// Инициализатор несовместим с типом переменной.
    InitTypeMismatch { var_ty: Type, init_ty: Type },
}

impl fmt::Display for SemanticErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UndeclaredIdent(id) =>
                write!(f, "undeclared identifier (id={})", id.0),
            Self::Redeclaration(id) =>
                write!(f, "redeclaration of identifier (id={})", id.0),
            Self::UndeclaredFunction(id) =>
                write!(f, "call to undeclared function (id={})", id.0),
            Self::NotCallable(id) =>
                write!(f, "identifier (id={}) is not callable", id.0),
            Self::TypeMismatch { expected, got } =>
                write!(f, "type mismatch: expected `{expected:?}`, got `{got:?}`"),
            Self::AssignTypeMismatch { lhs, rhs } =>
                write!(f, "cannot assign `{rhs:?}` to `{lhs:?}`"),
            Self::ArgTypeMismatch { param, got, arg_index } =>
                write!(f, "argument {arg_index}: expected `{param:?}`, got `{got:?}`"),
            Self::ArgCountMismatch { expected, got } =>
                write!(f, "expected {expected} argument(s), got {got}"),
            Self::InvalidUnaryOp { ty } =>
                write!(f, "unary operator cannot be applied to `{ty:?}`"),
            Self::NotIndexable(ty) =>
                write!(f, "`{ty:?}` is not indexable"),
            Self::NotAStruct(ty) =>
                write!(f, "`{ty:?}` is not a struct or union"),
            Self::NoSuchField(id) =>
                write!(f, "no field with id={}", id.0),
            Self::ReturnValueInVoid =>
                write!(f, "cannot return a value from a void function"),
            Self::MissingReturnValue { expected } =>
                write!(f, "missing return value: expected `{expected:?}`"),
            Self::BreakOutsideLoop =>
                write!(f, "`break` outside of loop or switch"),
            Self::ContinueOutsideLoop =>
                write!(f, "`continue` outside of loop"),
            Self::UndeclaredLabel(id) =>
                write!(f, "goto to undeclared label (id={})", id.0),
            Self::InitTypeMismatch { var_ty, init_ty } =>
                write!(f, "initializer type `{init_ty:?}` does not match variable type `{var_ty:?}`"),
        }
    }
}

/// Семантическая ошибка с позицией в исходнике.
#[derive(Debug, Clone)]
pub struct SemanticError {
    pub span: Span,
    pub kind: SemanticErrorKind,
}

impl SemanticError {
    pub fn new(span: Span, kind: SemanticErrorKind) -> Self {
        Self { span, kind }
    }
}

impl fmt::Display for SemanticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (l, c) = self.span.start_line_and_col;
        write!(f, "[{l}:{c}] error: {}", self.kind)
    }
}
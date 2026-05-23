// example-langs\w16-cc\src\frontend\parser\node.rs
//
//! # AST-ноды C11
//!
//! Без указателей и препроцессора.
//! Цель — максимально точно отразить структуру исходника,
//! семантический анализ и трансляция в HIR — отдельные этапы.

use crate::frontend::string_pool::StringId;
use crate::frontend::lexer::token::Span;
use crate::types::Type;
use crate::value::Value;

// ---------------------------------------------------------------------------
// Корень дерева
// ---------------------------------------------------------------------------

/// Единица трансляции — весь файл целиком.
#[derive(Debug, Clone)]
pub struct TranslationUnit {
    pub items: Vec<ExternalDecl>,
}

/// Объявление верхнего уровня (вне функции).
#[derive(Debug, Clone)]
pub enum ExternalDecl {
    /// Определение функции: `int foo(int x) { ... }`
    FunctionDef(FunctionDef),

    /// Глобальное объявление переменной или типа: `int x = 0;`
    Decl(Decl),
}

// ---------------------------------------------------------------------------
// Функции
// ---------------------------------------------------------------------------

/// Полное определение функции.
#[derive(Debug, Clone)]
pub struct FunctionDef {
    pub span: Span,
    /// Возвращаемый тип.
    pub return_ty: Type,
    /// Имя функции.
    pub name: StringId,
    /// Список параметров.
    pub params: Vec<Param>,
    /// Тело функции.
    pub body: Block,
    /// Помечена ли функция как `inline`.
    pub is_inline: bool,
    /// Помечена ли функция как `static`.
    pub is_static: bool,
    /// Помечена ли функция как `_Noreturn`.
    pub is_noreturn: bool,
}

/// Параметр функции.
#[derive(Debug, Clone)]
pub struct Param {
    pub span: Span,
    pub ty: Type,
    /// `None` — безымянный параметр в прототипе: `void foo(int)`.
    pub name: Option<StringId>,
}

// ---------------------------------------------------------------------------
// Объявления (declarations)
// ---------------------------------------------------------------------------

/// Объявление переменной, typedef, struct, enum или union.
#[derive(Debug, Clone)]
pub struct Decl {
    pub span: Span,
    pub kind: DeclKind,
}

#[derive(Debug, Clone)]
pub enum DeclKind {
    /// Переменная: `int x = 0;`
    Var(VarDecl),

    /// Несколько переменных одного типа: `int a, b = 1, c;`
    MultiVar(Vec<VarDecl>),

    /// `typedef int MyInt;`
    Typedef { ty: Type, alias: StringId },

    /// `struct Point { int x; int y; };`
    StructDef(StructDef),

    /// `union Data { int i; float f; };`
    UnionDef(UnionDef),

    /// `enum Color { Red, Green, Blue };`
    EnumDef(EnumDef),
}

/// Объявление одной переменной.
#[derive(Debug, Clone)]
pub struct VarDecl {
    pub span: Span,
    pub ty: Type,
    pub name: StringId,
    /// Инициализатор: `= expr` или `= { ... }`.
    pub initializer: Option<Initializer>,
    /// Спецификаторы хранения.
    pub storage: StorageClass,
    pub qualifiers: TypeQualifiers,
}

/// Спецификатор класса хранения.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StorageClass {
    #[default]
    Auto,
    Static,
    Extern,
    Register,
}

/// Квалификаторы типа.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TypeQualifiers {
    pub is_const: bool,
    pub is_volatile: bool,
    pub is_restrict: bool,
    pub is_atomic: bool,
}

/// Инициализатор переменной.
#[derive(Debug, Clone)]
pub enum Initializer {
    /// `= expr`
    Expr(Expr),
    /// `= { expr, expr, ... }` — для массивов и структур.
    List(Vec<Initializer>),
}

// ---------------------------------------------------------------------------
// Структуры, объединения, перечисления
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct StructDef {
    pub span: Span,
    /// `None` — анонимная структура.
    pub name: Option<StringId>,
    pub fields: Vec<FieldDecl>,
}

#[derive(Debug, Clone)]
pub struct UnionDef {
    pub span: Span,
    pub name: Option<StringId>,
    pub fields: Vec<FieldDecl>,
}

/// Поле структуры или объединения.
#[derive(Debug, Clone)]
pub struct FieldDecl {
    pub span: Span,
    pub ty: Type,
    pub name: StringId,
    /// Битовое поле: `int x : 3;`
    pub bit_width: Option<Box<Expr>>,
}

#[derive(Debug, Clone)]
pub struct EnumDef {
    pub span: Span,
    pub name: Option<StringId>,
    pub variants: Vec<EnumVariant>,
}

#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub span: Span,
    pub name: StringId,
    /// Явное значение: `Red = 5`.
    pub value: Option<Box<Expr>>,
}

// ---------------------------------------------------------------------------
// Блок и операторы (statements)
// ---------------------------------------------------------------------------

/// Блок `{ stmt* }`.
#[derive(Debug, Clone)]
pub struct Block {
    pub span: Span,
    pub stmts: Vec<Stmt>,
}

/// Оператор.
#[derive(Debug, Clone)]
pub struct Stmt {
    pub span: Span,
    pub kind: StmtKind,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    /// Объявление внутри блока.
    Decl(Decl),

    /// Выражение-оператор: `x = 1;`, `foo();`
    Expr(Expr),

    /// Пустой оператор: `;`
    Empty,

    /// `{ ... }`
    Block(Block),

    /// `if (cond) then [else alt]`
    If {
        cond: Expr,
        then: Box<Stmt>,
        alt: Option<Box<Stmt>>,
    },

    /// `while (cond) body`
    While {
        cond: Expr,
        body: Box<Stmt>,
    },

    /// `do body while (cond);`
    DoWhile {
        body: Box<Stmt>,
        cond: Expr,
    },

    /// `for (init; cond; step) body`
    For {
        /// `int i = 0` или `i = 0` или пусто.
        init: Option<ForInit>,
        cond: Option<Expr>,
        step: Option<Expr>,
        body: Box<Stmt>,
    },

    /// `switch (expr) { case ...: ... }`
    Switch {
        expr: Expr,
        body: Box<Stmt>,
    },

    /// `case expr:`
    Case(Expr),

    /// `default:`
    Default,

    /// `return [expr];`
    Return(Option<Expr>),

    /// `break;`
    Break,

    /// `continue;`
    Continue,

    /// `goto label;`
    Goto(StringId),

    /// `label:`
    Label(StringId),

    /// `_Static_assert(expr, "msg");`
    StaticAssert {
        cond: Expr,
        msg: StringId,
    },
}

/// Инициализирующая часть `for`.
#[derive(Debug, Clone)]
pub enum ForInit {
    /// `for (int i = 0; ...)`
    Decl(Decl),
    /// `for (i = 0; ...)`
    Expr(Expr),
}

// ---------------------------------------------------------------------------
// Выражения (expressions)
// ---------------------------------------------------------------------------

/// Выражение.
#[derive(Debug, Clone)]
pub struct Expr {
    pub span: Span,
    pub kind: ExprKind,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    /// Литерал: `42`, `3.14`, `'a'`, `true`.
    Literal(Value),

    /// Строковый литерал: `"hello"`.
    StringLiteral(StringId),

    /// Идентификатор: переменная или функция.
    Ident(StringId),

    /// Унарный оператор: `-x`, `!x`, `~x`, `++x`, `--x`, `x++`, `x--`.
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },

    /// Бинарный оператор: `a + b`, `a && b`, `a = b`.
    Binary {
        op: BinaryOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },

    /// Тернарный оператор: `cond ? then : alt`.
    Ternary {
        cond: Box<Expr>,
        then: Box<Expr>,
        alt: Box<Expr>,
    },

    /// Вызов функции: `foo(a, b)`.
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },

    /// Индексирование массива: `arr[i]`.
    Index {
        array: Box<Expr>,
        index: Box<Expr>,
    },

    /// Доступ к полю структуры: `s.field`.
    Field {
        object: Box<Expr>,
        field: StringId,
    },

    /// Составное выражение: `(a, b, c)` — значение последнего.
    Comma(Vec<Expr>),

    /// Приведение типа: `(int)x`.
    Cast {
        ty: Type,
        expr: Box<Expr>,
    },

    /// `sizeof(type)` или `sizeof expr`.
    Sizeof(SizeofArg),

    /// `_Alignof(type)`.
    Alignof(Type),

    /// `_Generic(expr, type: expr, ..., default: expr)`.
    Generic {
        control: Box<Expr>,
        associations: Vec<GenericAssoc>,
    },
}

/// Аргумент `sizeof`.
#[derive(Debug, Clone)]
pub enum SizeofArg {
    Type(Type),
    Expr(Box<Expr>),
}

/// Одна ассоциация `_Generic`.
#[derive(Debug, Clone)]
pub struct GenericAssoc {
    /// `None` — ветвь `default`.
    pub ty: Option<Type>,
    pub expr: Expr,
}

// ---------------------------------------------------------------------------
// Операторы
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// `-x`
    Neg,
    /// `+x`
    Pos,
    /// `!x`
    Not,
    /// `~x`
    BitNot,
    /// `++x`
    PreInc,
    /// `--x`
    PreDec,
    /// `x++`
    PostInc,
    /// `x--`
    PostDec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    // Арифметика
    Add, Sub, Mul, Div, Rem,

    // Побитовые
    BitAnd, BitOr, BitXor, Shl, Shr,

    // Сравнение
    Eq, Ne, Lt, Le, Gt, Ge,

    // Логика
    And, Or,

    // Присваивание
    Assign,
    AddAssign, SubAssign, MulAssign, DivAssign, RemAssign,
    AndAssign, OrAssign,  XorAssign, ShlAssign, ShrAssign,
}

impl BinaryOp {
    /// Является ли оператор присваиванием.
    pub fn is_assign(self) -> bool {
        matches!(self,
            BinaryOp::Assign
            | BinaryOp::AddAssign | BinaryOp::SubAssign
            | BinaryOp::MulAssign | BinaryOp::DivAssign | BinaryOp::RemAssign
            | BinaryOp::AndAssign | BinaryOp::OrAssign  | BinaryOp::XorAssign
            | BinaryOp::ShlAssign | BinaryOp::ShrAssign
        )
    }
}
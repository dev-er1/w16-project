//! example-langs\karbon\src\lexer\token.rs
//!
//! # Структура токена
//!
//! Тут определяются виды токена, и сама структура токена

use crate::{
    lexer::types::{Type, Value},
    string_table::StringId,
};

/// Виды токена
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TokenKind {
    // --- Системные и управляющие ---
    Ident(StringId), // Идентификатор (имена переменных, функций)
    Type(Type),
    Literal(Value), // Литералы (числа, строки, bool)
    EOF,            // Конец файла

    // --- Ключевые слова ---
    Var,    // var
    Const,  // const
    Fn,     // fn
    Struct, // struct
    Class,  // class
    If,     // if
    Else,   // else
    Loop,   // loop
    While,  // while
    For,    // for
    Return, // return (неявный или явный)
    Print,  // print

    // --- Спец-символы и пунктуация ---
    Assign, // =
    Arrow,  // -> (аргументы и поля структур)
    Dot,    // . (вызов методов .as, .load)
    Comma,  // , (разделитель аргументов)
    Colon,  // : (в будущем для типов, если изменится синтаксис)
    Semi,   // ;
    At,     // @ (интринсики: @alloc, @asm, @hot)

    // --- Скобки ---
    LParen,   // (
    RParen,   // )
    LBrace,   // {
    RBrace,   // }
    LBracket, // [
    RBracket, // ]

    // --- Операторы и Сравнение ---
    Operator(Op), // Математика
    Eq,           // ==
    NotEq,        // !=
    Lt,           // <
    Gt,           // >
    Le,           // <=
    Ge,           // >=

    // --- Работа с памятью ---
    Amp,  // & (безопасная ссылка)
    Star, // * (сырой указатель)
}

/// Операторы
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    Add, // Сложение
    Sub, // Вычитание
    Mul, // Умножение
    Div, // Деление
    Rem, // Остаток деления
}

/// Позиция в исходном коде
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub line: u32,
    pub start: u32,
    pub end: u32,
}
impl Span {
    pub fn new(line: u32, start: u32, end: u32) -> Self {
        Self { line, start, end }
    }
}

/// Структура токена
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Token {
    pub kind: TokenKind, // Вид токена
    pub pos: Span,       // Позиция в исходном коде
}

impl Token {
    pub fn new(kind: TokenKind, pos: Span) -> Self {
        Self { kind, pos }
    }
    /// Функция для быстрого создания EOF
    #[inline(always)]
    pub fn eof(line: u32, col: u32) -> Self {
        Self::new(TokenKind::EOF, Span::new(line, col, col))
    }
}

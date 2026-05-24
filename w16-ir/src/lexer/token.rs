// w16-ir\src\lexer\token.rs
//
//! # Типы токенов для текстового IR.
//!
//! [TokenKind] описывает сам вид токена, [Span] хранит позицию в исходном тексте,
//! а `Token` связывает их вместе. Parser использует spans для понятных ошибок.

/// Полуоткрытый диапазон байтов в исходном тексте: `[start, end)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    /// Начальная позиция в байтах.
    pub start: usize,
    /// Конечная позиция в байтах.
    pub end: usize,
}

/// Один токен текстового IR.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// Вид токена.
    pub kind: TokenKind,
    /// Где токен встретился в исходном тексте.
    pub span: Span,
}

/// Все токены, которые понимает текущий lexer.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    /// `module`.
    Module,
    /// `const`.
    Const,
    /// `fn`.
    Fn,
    /// `let`.
    Let,
    /// `if`.
    If,
    /// `else`.
    Else,
    /// `while`.
    While,
    /// `return`.
    Return,
    /// `halt`.
    Halt,
    /// `print`.
    Print,
    /// `true`.
    True,
    /// `false`.
    False,

    /// Обычный identifier без префикса.
    Ident(String),
    /// Имя функции после `@`.
    Function(String),
    /// Имя basic block после `^`.
    Block(String),
    /// SSA value после `%`. Зарезервировано под MIR.
    Value(u32),
    /// Локальная переменная после `$`.
    Local(String),

    /// Integer literal.
    Int(u64),
    /// Floating-point literal.
    Float(f64),
    /// String literal.
    String(String),

    /// `{`.
    LBrace,
    /// `}`.
    RBrace,
    /// `(`.
    LParen,
    /// `)`.
    RParen,
    /// `,`.
    Comma,
    /// `:`.
    Colon,
    /// `;`.
    Semicolon,
    /// `.`.
    Dot,
    /// `->`.
    Arrow,

    /// `+`.
    Plus,
    /// `-`.
    Minus,
    /// `*`.
    Star,
    /// `/`.
    Slash,
    /// `%`.
    Percent,
    /// `&`.
    Amp,
    /// `|`.
    Pipe,
    /// `^` как оператор пока не используется, но токен оставлен для полноты.
    Caret,
    /// `!`.
    Bang,
    /// `=`.
    Equal,
    /// `==`.
    EqualEqual,
    /// `!=`.
    BangEqual,
    /// `<`.
    Less,
    /// `<=`.
    LessEqual,
    /// `>`.
    Greater,
    /// `>=`.
    GreaterEqual,

    Shl,
    Shr,

    /// `break`.
    Break,
    /// `continue`.
    Continue,

    /// Конец файла.
    Eof,
}

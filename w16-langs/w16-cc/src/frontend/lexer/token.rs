// w16-langs\w16-cc\src\frontend\lexer\token.rs
//!
//! # Перечисление видов токена, и самой структуры токена
//! *Весь синтаксис по стандарту C11*
use crate::{frontend::string_pool::StringId, types::Type, value::Value};

/// Префикс строкового литерала C11.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringLiteralPrefix {
    /// `"text"`
    None,
    /// `u8"text"`
    Utf8,
    /// `L"text"`
    Wide,
    /// `u"text"`
    Utf16,
    /// `U"text"`
    Utf32,
}

/// Префикс символьного литерала C11.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharLiteralPrefix {
    /// `'x'`
    None,
    /// `L'x'`
    Wide,
    /// `u'x'`
    Utf16,
    /// `U'x'`
    Utf32,
}

/// Строковый литерал с сохранённым префиксом.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StringLiteral {
    /// Какая форма строкового литерала была в исходнике.
    pub prefix: StringLiteralPrefix,
    /// Содержимое литерала в string pool.
    pub value: StringId,
}

/// Символьный литерал с сохранённым префиксом.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharLiteral {
    /// Какая форма символьного литерала была в исходнике.
    pub prefix: CharLiteralPrefix,
    /// Содержимое литерала в string pool после обработки escape-последовательностей.
    pub value: StringId,
}

/// Виды токена
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    /// # Идентификатор
    ///
    /// Всё что НЕ попадает в список видов токена — Ident.
    ///
    /// ### Пример
    /// ```c
    /// int x = 0;
    /// ```
    /// Здесь 'x' = Ident(0).
    Ident(StringId),

    /// # Тип
    ///
    /// Это тип из перечисления [Type].
    ///
    /// ### Пример
    /// ```c
    /// int x = 0;
    /// ```
    /// Здесь типом является 'int'.
    Typ(Type),

    /// Значение.
    ValueLiteral(Value),

    /// Строковый литерал: `"abc"`, `u8"abc"`, `L"abc"`, `u"abc"`, `U"abc"`.
    StringLiteral(StringLiteral),

    /// Символьный литерал: `'a'`, `L'a'`, `u'a'`, `U'a'`.
    CharLiteral(CharLiteral),

    // === синтаксис ===
    // *Весь синтаксис взят из стандарта C11*
    /// # Auto
    ///
    /// Ключевое слово auto — это спецификатор класса хранения (storage class specifier).
    /// Оно явно указывает компилятору, что переменная должна быть создана на стеке при входе в функцию (или локальный блок {})
    /// и автоматически уничтожена при выходе из него.
    ///
    /// ### Пример
    /// ```c
    ///
    /// void my_function() {
    ///     auto int x = 0;
    /// }
    /// ```
    /// Сейчас писать 'auto' — необязательно, так как все переменные объявленные в блоке, итак являются auto.
    Auto,

    /// # Break
    ///
    /// Break мгновенно прерывает выполнение текущего цикла (for, while, do-while) или блока switch,
    /// и передает управление на первую строчку кода после этого блока.
    Break,

    /// # Case
    ///
    /// Метка внутри блока switch. Указывает точку перехода, если вычисляемое
    /// значение switch совпадает с константой после case.
    ///
    /// ### Пример
    /// ```c
    /// switch(x) {
    ///     case 1: break;
    /// }
    /// ```
    Case,

    /// # Default
    ///
    /// Ветвь в блоке switch, которая выполняется, если ни один из операторов case не подошел.
    ///
    /// ### Пример
    /// ```c
    /// switch(x) {
    ///     case 1: break;
    ///     default: do_something();
    /// }
    /// ```
    Default,

    /// # Switch
    ///
    /// Управляющая конструкция многовариантного ветвления. Вычисляет выражение
    /// и передает управление на соответствующую метку case или default.
    Switch,

    /// # While
    ///
    /// Цикл с предусловием. Выполняет блок кода, пока проверяемое выражение не станет равным 0 (ложь).
    ///
    /// ### Пример
    /// ```c
    /// while (x > 0) { x--; }
    /// ```
    While,

    /// # Do
    ///
    /// Часть цикла с постусловием do-while. Гарантирует, что тело цикла выполнится как минимум один раз.
    ///
    /// ### Пример
    /// ```c
    /// do { x--; } while (x > 0);
    /// ```
    Do,

    /// # For
    ///
    /// Универсальный цикл, объединяющий инициализацию, условие и итерационное выражение.
    /// Начиная с C99, позволяет объявлять счетчик прямо внутри цикла.
    ///
    /// ### Пример
    /// ```c
    /// for (int i = 0; i < 10; i++) {}
    /// ```
    For,

    /// # Continue
    ///
    /// Мгновенно завершает текущую итерацию цикла и переходит к проверке условия (в while)
    /// или к шагу итерации (в for).
    Continue,

    /// # If
    ///
    /// Условный оператор. Выполняет следующий за ним блок, если выражение в скобках не равно 0.
    If,

    /// # Else
    ///
    /// Альтернативная ветка условного оператора if. Выполняется, если условие в if вернуло 0.
    Else,

    /// # Return
    ///
    /// Завершает выполнение текущей функции и возвращает значение (если функция не void)
    /// в вызывающий поток.
    Return,

    /// # Goto
    ///
    /// Оператор безусловного перехода на именованную метку (label) в пределах текущей функции.
    ///
    /// ### Пример
    /// ```c
    /// loop:
    ///     goto loop;
    /// ```
    Goto,

    /// # Const
    ///
    /// Квалификатор типа: объект нельзя изменять через это имя.
    Const,

    /// # Enum
    ///
    /// Объявление перечислимого типа.
    Enum,

    /// # Extern
    ///
    /// Спецификатор класса хранения: объект или функция объявлены в другом месте.
    Extern,

    /// # Inline
    ///
    /// Подсказка компилятору о встраивании функции.
    Inline,

    /// # Register
    ///
    /// Спецификатор класса хранения для локальных объектов с быстрым доступом.
    Register,

    /// # Restrict
    ///
    /// Квалификатор указателя: через него выполняется единственный основной доступ к объекту.
    Restrict,

    /// # Sizeof
    ///
    /// Оператор получения размера типа или выражения.
    Sizeof,

    /// # Static
    ///
    /// Спецификатор класса хранения или внутренней компоновки.
    Static,

    /// # Struct
    ///
    /// Объявление структурного типа.
    Struct,

    /// # Typedef
    ///
    /// Объявляет новое имя для существующего типа.
    Typedef,

    /// # Union
    ///
    /// Объявление объединения.
    Union,

    /// # Volatile
    ///
    /// Квалификатор типа: значение может измениться вне видимого потока выполнения.
    Volatile,

    /// # _Alignas
    ///
    /// Спецификатор выравнивания объекта.
    Alignas,

    /// # _Alignof
    ///
    /// Оператор получения требования выравнивания типа.
    Alignof,

    /// # _Atomic
    ///
    /// Спецификатор или квалификатор атомарного типа.
    Atomic,

    /// # _Generic
    ///
    /// Обобщенный выбор выражения по типу.
    Generic,

    /// # _Noreturn
    ///
    /// Спецификатор функции, которая не возвращает управление вызывающему коду.
    Noreturn,

    /// # _Static_assert
    ///
    /// Проверка условия во время компиляции.
    StaticAssert,

    /// # _Thread_local
    ///
    /// Спецификатор хранения с отдельным объектом для каждого потока.
    ThreadLocal,

    /// # include
    ///
    /// Preprocessor directive `#include`.
    Include,

    /// # define
    ///
    /// Preprocessor directive `#define`.
    Define,

    /// # ifdef
    ///
    /// Preprocessor directive `#ifdef`.
    Ifdef,

    /// # ifndef
    ///
    /// Preprocessor directive `#ifndef`.
    Ifndef,

    /// # endif
    ///
    /// Preprocessor directive `#endif`.
    Endif,

    /// # pragma
    ///
    /// Preprocessor directive `#pragma`.
    Pragma,

    /// # Подключение библиотек
    ///
    /// ### Пример
    /// ```c
    /// #include <stdio.h>
    ///
    /// int main() {
    ///     printf("Hello world!");
    ///     return 0;
    /// }
    /// ```
    HeaderName(StringId),

    // =========================================================================
    // Пунктуация и Операторы
    // =========================================================================
    LeftParen,    // (
    RightParen,   // )
    LeftBrace,    // {
    RightBrace,   // }
    LeftBracket,  // [
    RightBracket, // ]

    Semicolon, // ;
    Comma,     // ,
    Dot,       // .
    Arrow,     // ->
    Ellipsis,  // ...
    Question,  // ?
    Colon,     // :
    Hash,      // #
    HashHash,  // ##

    // Математика
    Plus,       // +
    Minus,      // -
    Star,       // *
    Slash,      // /
    Percent,    // %
    PlusPlus,   // ++
    MinusMinus, // --

    // Сравнение и логика
    Assign,       // =
    Equal,        // ==
    NotEqual,     // !=
    LessThan,     // <
    LessEqual,    // <=
    GreaterThan,  // >
    GreaterEqual, // >=
    AmpAmp,       // &&
    PipePipe,     // ||
    Bang,         // !

    // Побитовые
    Amp,        // &
    Pipe,       // |
    Caret,      // ^
    Tilde,      // ~
    LeftShift,  // <<
    RightShift, // >>

    // Составное присваивание
    PlusAssign,       // +=
    MinusAssign,      // -=
    StarAssign,       // *=
    SlashAssign,      // /=
    PercentAssign,    // %=
    AmpAssign,        // &=
    PipeAssign,       // |=
    CaretAssign,      // ^=
    LeftShiftAssign,  // <<=
    RightShiftAssign, // >>=

    // Системные
    /// Конец строки. Нужен preprocessor-у, потому что директивы живут до newline.
    NewLine,

    /// # Конец кода
    ///
    /// Не надо кидать помидоры в меня, за то, что я назвал этот токен
    /// не "Eof".
    EndOfCode,
}

/// # Позиция в исходном коде
///
/// Позиция в исходном коде. Хранится
/// - Начальная линия и колонка в линии,
/// - Заканчивающаяся линия и колонка в линии.
///
/// Формат:
/// (линия, колонка в линии)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start_line_and_col: (u32, u32),
    pub end_line_and_col: (u32, u32),
}

impl Span {
    pub fn new(start_line_and_col: (u32, u32), end_line_and_col: (Option<u32>, u32)) -> Self {
        let end_line_and_col = (
            end_line_and_col.0.unwrap_or(start_line_and_col.0),
            end_line_and_col.1,
        );
        Self {
            start_line_and_col,
            end_line_and_col,
        }
    }
}

/// # Токен
///
/// Хранит в себе:
/// - [Span]. Позиция в коде.
/// - [TokenKind]. Вид токена.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// Позиция в коде
    pub position: Span,

    /// Вид токена
    pub kind: TokenKind,
}

impl Token {
    pub fn new_token(position: Span, kind: TokenKind) -> Self {
        Self { position, kind }
    }
}

//! example-langs\karbon\src\lexer\mod.rs
//!
//! # Лексер
//!
//! Превращаем код в вектор токенов(Token из src\lexer\token.rs)
pub mod lexer_errors;
pub mod token;
pub mod types;

use crate::lexer::token::{Op, Span, Token, TokenKind};
use crate::lexer::types::{FNV, SIV, Type, Value};
use crate::{
    lexer::lexer_errors::{LexerError, LexerErrors},
    string_table::StringTable,
};
use std::iter::Peekable;
use std::str::Chars;

/// Структура лексера
pub struct Lexer<'a> {
    /// Таблица строк
    pub string_table: &'a mut StringTable,
    /// Исходный код для вывода ошибок и взятия срезов
    pub source: &'a str,
    /// Итератор для токенизации
    chars: Peekable<Chars<'a>>,
    pub errors: Vec<LexerError>,
    // Текущая позиция для Span
    line: u32,
    col: u32,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str, string_table: &'a mut StringTable) -> Self {
        Self {
            string_table,
            source,
            chars: source.chars().peekable(), // Создаем итератор из строки
            errors: Vec::new(),
            line: 1,
            col: 1,
        }
    }

    /// Точка входа всего лексера, токенизирует код и
    /// возвращает кортеж: (найденные токены, список ошибок)
    pub fn tokenize(&mut self) -> (Vec<Token>, Vec<LexerError>) {
        let mut tokens = Vec::new();

        while !self.is_eof() {
            self.skip_whitespace();
            if self.is_eof() {
                break;
            }

            match self.next_token() {
                Some(token) => tokens.push(token),
                None => {
                    // Ошибка уже добавлена в self.errors внутри next_token.
                    // Продвигаемся вперед, чтобы не зациклиться.
                    self.advance();
                }
            }
        }

        tokens.push(Token::eof(self.line, self.col));

        // Передаем владение векторами вызывающей стороне
        (tokens, std::mem::take(&mut self.errors))
    }

    /// Вспомогательный метод для продвижения вперед
    #[inline(always)]
    fn advance(&mut self) -> Option<char> {
        let ch = self.chars.next()?;
        if ch == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(ch)
    }

    /// Посмотреть следующий символ, не забирая его
    #[inline(always)]
    fn peek(&mut self) -> Option<char> {
        self.chars.peek().copied()
    }

    /// Проверка на конец кода
    #[inline(always)]
    fn is_eof(&mut self) -> bool {
        self.chars.peek().is_none()
    }

    /// Пропускает все незначащие символы: пробелы, таб-ы, переносы строк и комментарии
    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            match ch {
                // Обычные пробельные символы
                ' ' | '\t' | '\r' => {
                    self.advance();
                }
                // Перенос строки (обрабатывается внутри advance для инкремента self.line)
                '\n' => {
                    self.advance();
                }
                // Начало комментария
                '/' => {
                    // Нам нужно заглянуть на второй символ, не поглощая первый раньше времени
                    if self.peek_next() == Some('/') {
                        self.skip_line_comment();
                    } else if self.peek_next() == Some('*') {
                        self.skip_block_comment();
                    } else {
                        // Это не комментарий, а просто оператор деления. Выходим.
                        break;
                    }
                }
                _ => break, // Встретили значащий символ (букву, цифру и т.д.)
            }
        }
    }

    /// Пропуск однострочного комментария // ...
    fn skip_line_comment(&mut self) {
        // Поглощаем обе косые черты
        self.advance();
        self.advance();

        // Читаем до конца строки или конца файла
        while let Some(ch) = self.peek() {
            if ch == '\n' {
                break;
            }
            self.advance();
        }
    }

    /// Пропуск многострочного комментария /* ... */
    fn skip_block_comment(&mut self) {
        let start_span = Span::new(self.line, self.col, self.col + 2);
        self.advance(); // /
        self.advance(); // *

        while let Some(ch) = self.advance() {
            if ch == '*' && self.peek() == Some('/') {
                self.advance(); // Поглощаем /
                return;
            }
        }

        // Если дошли до конца файла, а комментарий не закрыт — это ошибка!
        self.errors.push(LexerError::new(
            start_span,
            LexerErrors::UnterminatedComment,
        ));
    }

    /// Вспомогательный метод: заглянуть на 2 символа вперед
    #[inline(always)]
    fn peek_next(&self) -> Option<char> {
        let mut iter = self.chars.clone();
        iter.next(); // Пропускаем текущий
        iter.next() // Смотрим следующий
    }

    /// Следующий токен
    fn next_token(&mut self) -> Option<Token> {
        let start_col = self.col;
        let ch = self.advance()?;

        let kind = match ch {
            // Односимвольные
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '[' => TokenKind::LBracket,
            ']' => TokenKind::RBracket,
            ',' => TokenKind::Comma,
            '.' => TokenKind::Dot,
            ';' => TokenKind::Semi,
            '@' => TokenKind::At,

            // Операторы с возможным вторым символом
            '=' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::Eq
                } else {
                    TokenKind::Assign
                }
            }
            '!' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::NotEq
                } else {
                    self.errors.push(LexerError::new(
                        Span::new(self.line, start_col, self.col),
                        LexerErrors::UnexpectedChar('!'),
                    ));
                    return None;
                }
            }
            '-' => {
                if self.peek() == Some('>') {
                    self.advance();
                    TokenKind::Arrow
                } else {
                    TokenKind::Operator(Op::Sub)
                }
            }
            '+' => TokenKind::Operator(Op::Add),
            '*' => TokenKind::Star,
            '/' => TokenKind::Operator(Op::Div),
            '%' => TokenKind::Operator(Op::Rem),
            '&' => TokenKind::Amp,

            // Сравнения
            '<' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::Le
                } else {
                    TokenKind::Lt
                }
            }
            '>' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::Ge
                } else {
                    TokenKind::Gt
                }
            }

            // Идентификаторы и Ключевые слова
            'a'..='z' | 'A'..='Z' | '_' => {
                return Some(self.read_identifier(ch, start_col));
            }

            // Числа
            '0'..='9' => {
                return Some(self.read_number(ch, start_col));
            }
            '"' => return Some(self.read_string(start_col)),
            _ => {
                self.errors.push(LexerError::new(
                    Span::new(self.line, start_col, self.col),
                    LexerErrors::UnexpectedChar(ch),
                ));
                return None;
            }
        };

        Some(Token::new(kind, Span::new(self.line, start_col, self.col)))
    }

    fn read_string(&mut self, start_col: u32) -> Token {
        let mut text = String::new();
        let start_line = self.line;

        while let Some(ch) = self.advance() {
            match ch {
                // Успешное завершение строки
                '"' => {
                    let end_col = self.col;
                    let id = self.string_table.intern(&text);
                    return Token::new(
                        TokenKind::Literal(Value::Str(id)),
                        Span::new(start_line, start_col, end_col),
                    );
                }
                // Обработка экранирования символов (например, \" или \n)
                '\\' => {
                    if let Some(escaped) = self.advance() {
                        match escaped {
                            'n' => text.push('\n'),
                            't' => text.push('\t'),
                            '\\' => text.push('\\'),
                            '"' => text.push('"'),
                            _ => text.push(escaped),
                        }
                    }
                }
                // Обычный символ
                _ => text.push(ch),
            }
        }

        // Если мы вышли из цикла по EOF, значит кавычка не найдена
        let end_col = self.col;
        self.errors.push(LexerError::new(
            Span::new(start_line, start_col, self.col),
            LexerErrors::UnterminatedString(Span {
                line: self.line,
                start: start_col,
                end: end_col,
            }),
        ));

        Token::new(TokenKind::EOF, Span::new(self.line, self.col, end_col))
    }
    fn read_identifier(&mut self, first_char: char, start_col: u32) -> Token {
        let mut text = String::new();
        text.push(first_char);

        // Пока следующий символ — буква, цифра или нижнее подчеркивание
        while let Some(ch) = self.peek() {
            if ch.is_alphanumeric() || ch == '_' {
                text.push(self.advance().unwrap());
            } else {
                break;
            }
        }

        // 1. Проверяем, не является ли это ключевым словом
        let kind = match text.as_str() {
            "var" => TokenKind::Var,
            "const" => TokenKind::Const,
            "fn" => TokenKind::Fn,
            "struct" => TokenKind::Struct,
            "class" => TokenKind::Class,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "loop" => TokenKind::Loop,
            "while" => TokenKind::While,
            "for" => TokenKind::For,
            "return" => TokenKind::Return,
            "true" => TokenKind::Literal(Value::Bool(true)),
            "false" => TokenKind::Literal(Value::Bool(false)),
            "print" => TokenKind::Print,
            "i8" => TokenKind::Type(Type::Signed(types::SignedInt::I8)),
            "i16" => TokenKind::Type(Type::Signed(types::SignedInt::I16)),
            "i32" => TokenKind::Type(Type::Signed(types::SignedInt::I32)),
            "i64" => TokenKind::Type(Type::Signed(types::SignedInt::I64)),
            "u8" => TokenKind::Type(Type::Unsigned(types::UnsignedInt::U8)),
            "u16" => TokenKind::Type(Type::Unsigned(types::UnsignedInt::U16)),
            "u32" => TokenKind::Type(Type::Unsigned(types::UnsignedInt::U32)),
            "u64" => TokenKind::Type(Type::Unsigned(types::UnsignedInt::U64)),
            "f32" => TokenKind::Type(Type::Float(types::FloatNum::F32)),
            "f64" => TokenKind::Type(Type::Float(types::FloatNum::F64)),
            "str" => TokenKind::Type(Type::Str),
            "void" => TokenKind::Type(Type::Void),

            // 2. Если не ключевое слово, интернируем как обычный идентификатор
            _ => TokenKind::Ident(self.string_table.intern(&text)),
        };

        Token::new(kind, Span::new(self.line, start_col, self.col))
    }

    /// Метод для чтения цифры
    fn read_number(&mut self, first_char: char, start_col: u32) -> Token {
        let mut number_str = String::new();
        number_str.push(first_char);

        let mut is_float = false;

        while let Some(ch) = self.peek() {
            // 1. Если цифра — просто поглощаем
            if ch.is_ascii_digit() {
                number_str.push(self.advance().unwrap());
                continue;
            }

            // 2. Если точка — проверяем, не начало ли это метода (например, 1.as(i32))
            if ch == '.' && !is_float && self.peek_next().is_some_and(|next| next.is_ascii_digit())
            {
                is_float = true;
                number_str.push(self.advance().unwrap());
                continue;
            }

            // 3. Во всех остальных случаях (пробел, буква, оператор) — число закончилось
            break;
        }

        // Превращаем накопленную строку в значение
        let kind = self.parse_number_literal(&number_str, is_float, start_col);

        Token::new(kind, Span::new(self.line, start_col, self.col))
    }

    /// Вынесено отдельно, чтобы не загромождать основной цикл лексера
    #[inline]
    fn parse_number_literal(&mut self, s: &str, is_float: bool, start_col: u32) -> TokenKind {
        if is_float {
            s.parse::<f64>()
                .map(|val| TokenKind::Literal(Value::FloatVal(FNV::F64(val))))
                .unwrap_or_else(|_| self.emit_number_error(s, start_col))
        } else {
            s.parse::<i64>()
                .map(|val| TokenKind::Literal(Value::SignedVal(SIV::I64(val))))
                .unwrap_or_else(|_| self.emit_number_error(s, start_col))
        }
    }

    fn emit_number_error(&mut self, s: &str, start_col: u32) -> TokenKind {
        self.errors.push(LexerError::new(
            Span::new(self.line, start_col, self.col),
            LexerErrors::InvalidNumberFormat(s.to_string()),
        ));
        TokenKind::EOF
    }
}

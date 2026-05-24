// w16-ir\src\lexer\scanner.rs
//
//! # Реализация lexer для текстового W16 IR.
//!
//! Scanner идёт по исходной строке слева направо и создаёт `Token`. Он работает
//! на байтах, потому что синтаксис IR сейчас ASCII-only. Строковые литералы тоже
//! временно ограничены ASCII: это упрощает spans и убирает раннюю сложность с
//! Unicode, пока мы строим frontend-скелет.

use std::fmt;

use super::token::{Span, Token, TokenKind};

/// Ошибка лексического анализа.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    /// Человеческое описание ошибки.
    pub message: String,
    /// Где ошибка произошла в исходном тексте.
    pub span: Span,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at {}..{}",
            self.message, self.span.start, self.span.end
        )
    }
}

impl std::error::Error for LexError {}

/// Stateful lexer для одной исходной строки.
pub struct Lexer<'a> {
    /// Исходный текст целиком. Нужен для нарезки lexeme по byte spans.
    source: &'a str,
    /// Байтовое представление того же текста для быстрого просмотра символов.
    bytes: &'a [u8],
    /// Текущая позиция в байтах.
    pos: usize,
}

impl<'a> Lexer<'a> {
    /// Создать lexer для исходной строки.
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            bytes: source.as_bytes(),
            pos: 0,
        }
    }

    /// Разбить весь исходный текст на токены.
    ///
    /// Возвращаемый список всегда заканчивается `TokenKind::Eof`, чтобы parser
    /// мог безопасно смотреть на текущий токен без постоянных проверок длины.
    pub fn tokenize(mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();

        loop {
            let token = self.next_token()?;
            let done = token.kind == TokenKind::Eof;
            tokens.push(token);
            if done {
                return Ok(tokens);
            }
        }
    }

    fn next_token(&mut self) -> Result<Token, LexError> {
        self.skip_ws_and_comments();
        let start = self.pos;
        let Some(ch) = self.bump() else {
            return Ok(Token {
                kind: TokenKind::Eof,
                span: Span { start, end: start },
            });
        };

        // Односимвольные токены обрабатываем сразу, а префиксные имена
        // (`@name`, `$name`, `^block`) дочитываем отдельным helper-ом.
        let kind = match ch {
            b'{' => TokenKind::LBrace,
            b'}' => TokenKind::RBrace,
            b'(' => TokenKind::LParen,
            b')' => TokenKind::RParen,
            b',' => TokenKind::Comma,
            b':' => TokenKind::Colon,
            b';' => TokenKind::Semicolon,
            b'.' => TokenKind::Dot,
            b'+' => TokenKind::Plus,
            b'*' => TokenKind::Star,
            b'/' => TokenKind::Slash,
            b'%' => TokenKind::Percent,
            b'&' => TokenKind::Amp,
            b'|' => TokenKind::Pipe,
            b'^' => {
                let name = self.read_identifier_tail(start)?;
                TokenKind::Block(name)
            }
            b'@' => {
                let name = self.read_identifier_tail(start)?;
                TokenKind::Function(name)
            }
            b'$' => {
                let name = self.read_identifier_tail(start)?;
                TokenKind::Local(name)
            }
            b'!' => {
                if self.eat(b'=') {
                    TokenKind::BangEqual
                } else {
                    TokenKind::Bang
                }
            }
            b'=' => {
                if self.eat(b'=') {
                    TokenKind::EqualEqual
                } else {
                    TokenKind::Equal
                }
            }
            b'<' => {
                if self.eat(b'=') {
                    TokenKind::LessEqual
                } else {
                    TokenKind::Less
                }
            }
            b'>' => {
                if self.eat(b'=') {
                    TokenKind::GreaterEqual
                } else if self.eat(b'>') {
                    TokenKind::Shr
                } else {
                    TokenKind::Greater
                }
            }
            b'-' => {
                if self.eat(b'>') {
                    TokenKind::Arrow
                } else if self.eat(b'<') {
                    TokenKind::Shl
                } else {
                    TokenKind::Minus
                }
            }
            b'"' => self.read_string(start)?,
            b'0'..=b'9' => self.read_number(start)?,
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.read_word(start),
            other => {
                return Err(self.error(
                    start,
                    self.pos,
                    format!("unexpected byte '{}'", other as char),
                ));
            }
        };

        Ok(Token {
            kind,
            span: Span {
                start,
                end: self.pos,
            },
        })
    }

    fn skip_ws_and_comments(&mut self) {
        loop {
            while matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
                self.pos += 1;
            }

            // В IR-документах и golden tests удобно писать обычные `//`
            // комментарии. Lexer полностью выбрасывает их из token stream.
            if self.peek() == Some(b'/') && self.peek_next() == Some(b'/') {
                self.pos += 2;
                while !matches!(self.peek(), None | Some(b'\n')) {
                    self.pos += 1;
                }
                continue;
            }

            break;
        }
    }

    fn read_identifier_tail(&mut self, start: usize) -> Result<String, LexError> {
        let name_start = self.pos;
        if !matches!(self.peek(), Some(b'a'..=b'z' | b'A'..=b'Z' | b'_')) {
            return Err(self.error(start, self.pos, "expected identifier after prefix"));
        }
        while matches!(
            self.peek(),
            Some(b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'0'..=b'9')
        ) {
            self.pos += 1;
        }
        Ok(self.source[name_start..self.pos].to_owned())
    }

    fn read_word(&mut self, start: usize) -> TokenKind {
        while matches!(
            self.peek(),
            Some(b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'0'..=b'9')
        ) {
            self.pos += 1;
        }
        // Ключевые слова распознаются здесь; всё остальное остаётся обычным
        // identifier и уже parser решает, является ли это типом/константой/etc.
        match &self.source[start..self.pos] {
            "module" => TokenKind::Module,
            "const" => TokenKind::Const,
            "fn" => TokenKind::Fn,
            "let" => TokenKind::Let,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "while" => TokenKind::While,
            "return" => TokenKind::Return,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            "halt" => TokenKind::Halt,
            "print" => TokenKind::Print,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            word => TokenKind::Ident(word.to_owned()),
        }
    }

    fn read_number(&mut self, start: usize) -> Result<TokenKind, LexError> {
        // `0x...` всегда integer. Десятичный literal становится float только
        // если после точки есть хотя бы одна цифра.
        if self.bytes[start] == b'0' && matches!(self.peek(), Some(b'x' | b'X')) {
            self.pos += 1;
            let digits_start = self.pos;
            while matches!(self.peek(), Some(b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F')) {
                self.pos += 1;
            }
            if digits_start == self.pos {
                return Err(self.error(start, self.pos, "expected hex digits"));
            }
            let value = u64::from_str_radix(&self.source[digits_start..self.pos], 16)
                .map_err(|_| self.error(start, self.pos, "integer literal is too large"))?;
            return Ok(TokenKind::Int(value));
        }

        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }

        if self.peek() == Some(b'.') && matches!(self.peek_next(), Some(b'0'..=b'9')) {
            self.pos += 1;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
            let value = self.source[start..self.pos]
                .parse::<f64>()
                .map_err(|_| self.error(start, self.pos, "invalid float literal"))?;
            Ok(TokenKind::Float(value))
        } else {
            let value = self.source[start..self.pos]
                .parse::<u64>()
                .map_err(|_| self.error(start, self.pos, "integer literal is too large"))?;
            Ok(TokenKind::Int(value))
        }
    }

    fn read_string(&mut self, start: usize) -> Result<TokenKind, LexError> {
        let mut value = String::new();

        loop {
            let Some(ch) = self.bump() else {
                return Err(self.error(start, self.pos, "unterminated string literal"));
            };

            match ch {
                b'"' => return Ok(TokenKind::String(value)),
                b'\\' => {
                    // Поддерживаем только минимальный набор escape-последовательностей.
                    // Расширение до Unicode лучше делать отдельным маленьким шагом.
                    let Some(escaped) = self.bump() else {
                        return Err(self.error(start, self.pos, "unterminated escape sequence"));
                    };
                    match escaped {
                        b'n' => value.push('\n'),
                        b'r' => value.push('\r'),
                        b't' => value.push('\t'),
                        b'"' => value.push('"'),
                        b'\\' => value.push('\\'),
                        _ => {
                            return Err(self.error(
                                self.pos - 1,
                                self.pos,
                                "unsupported escape sequence",
                            ));
                        }
                    }
                }
                byte if byte.is_ascii() => value.push(byte as char),
                _ => {
                    return Err(self.error(
                        self.pos - 1,
                        self.pos,
                        "strings must be ASCII for now",
                    ));
                }
            }
        }
    }

    fn bump(&mut self) -> Option<u8> {
        let ch = self.peek()?;
        self.pos += 1;
        Some(ch)
    }

    fn eat(&mut self, expected: u8) -> bool {
        if self.peek() == Some(expected) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn peek_next(&self) -> Option<u8> {
        self.bytes.get(self.pos + 1).copied()
    }

    fn error(&self, start: usize, end: usize, message: impl Into<String>) -> LexError {
        LexError {
            message: message.into(),
            span: Span { start, end },
        }
    }
}

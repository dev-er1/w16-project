// example-langs\w16-cc\src\frontend\lexer\mod.rs
//
//! # Лексер
//!
//! Превращает C11-ish исходный текст в поток токенов. Это пока не полноценный
//! preprocessor: lexer только сохраняет `#`, `##`, директивные ключевые слова
//! и `NewLine`, чтобы следующий этап мог разобрать `#include`, `#define` и
//! похожие конструкции.

pub mod lexer_error;
pub mod token;

use std::{iter::Peekable, str::Chars};

use crate::{
    frontend::{
        lexer::{
            lexer_error::{LexerError, LexerErrors},
            token::{
                CharLiteral, CharLiteralPrefix, Span, StringLiteral, StringLiteralPrefix, Token,
                TokenKind,
            },
        },
        string_pool::StringTable,
    },
    types::Type,
    value::{Value, F80},
};

/// Лексер для C-like исходника.
pub struct Lexer<'a> {
    src: Peekable<Chars<'a>>,
    line: u32,
    col: u32,
    pub errors: Vec<LexerError>,
    pub tokens: Vec<Token>,
    pub string_table: StringTable,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str, string_table: StringTable) -> Self {
        Self {
            src: source.chars().peekable(),
            line: 1,
            col: 0,
            errors: Vec::new(),
            tokens: Vec::new(),
            string_table,
        }
    }

    /// Запускает lexer и возвращает tokens + string table или накопленные errors.
    pub fn tokenize(mut self) -> Result<(Vec<Token>, StringTable), Vec<LexerError>> {
        while self.peek_ch().is_some() {
            self.skip_horizontal_whitespace();
            let Some(ch) = self.peek_ch() else {
                break;
            };

            let start = self.peek_span();
            match ch {
                '\n' => {
                    self.bump();
                    self.push(start, TokenKind::NewLine);
                }
                'a'..='z' | 'A'..='Z' | '_' => self.lex_word_or_prefixed_literal(),
                '0'..='9' => self.lex_number(),
                '"' => self.lex_string(StringLiteralPrefix::None),
                '\'' => self.lex_char(CharLiteralPrefix::None),
                '/' if self.peek_next_ch() == Some('/') => self.skip_line_comment(),
                '/' if self.peek_next_ch() == Some('*') => self.skip_block_comment(),
                _ => self.lex_punctuation_or_operator(),
            }
        }

        let eof_start = self.peek_span();
        self.push(eof_start, TokenKind::EndOfCode);

        if self.errors.is_empty() {
            Ok((self.tokens, self.string_table))
        } else {
            Err(self.errors)
        }
    }

    #[inline(always)]
    fn peek_ch(&mut self) -> Option<char> {
        self.src.peek().copied()
    }

    #[inline(always)]
    fn peek_next_ch(&mut self) -> Option<char> {
        let mut it = self.src.clone();
        it.next();
        it.next()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.src.next()?;
        if ch == '\n' {
            self.line += 1;
            self.col = 0;
        } else {
            self.col += 1;
        }
        Some(ch)
    }

    fn eat_if(&mut self, expected: char) -> bool {
        if self.peek_ch() == Some(expected) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn skip_horizontal_whitespace(&mut self) {
        while matches!(self.peek_ch(), Some(' ' | '\t' | '\r')) {
            self.bump();
        }
    }

    #[inline(always)]
    fn peek_span(&self) -> (u32, u32) {
        (self.line, self.col)
    }

    fn span_from(&self, start: (u32, u32)) -> Span {
        Span::new(start, (Some(self.line), self.col))
    }

    fn push(&mut self, start: (u32, u32), kind: TokenKind) {
        self.tokens
            .push(Token::new_token(self.span_from(start), kind));
    }

    fn error(&mut self, error: LexerErrors, start: (u32, u32)) {
        self.errors
            .push(LexerError::new(error, self.span_from(start)));
    }

    fn lex_word_or_prefixed_literal(&mut self) {
        let start = self.peek_span();
        let word = self.read_while(is_ident_continue);

        match (word.as_str(), self.peek_ch()) {
            ("u8", Some('"')) => return self.lex_string_from(start, StringLiteralPrefix::Utf8),
            ("L", Some('"')) => return self.lex_string_from(start, StringLiteralPrefix::Wide),
            ("u", Some('"')) => return self.lex_string_from(start, StringLiteralPrefix::Utf16),
            ("U", Some('"')) => return self.lex_string_from(start, StringLiteralPrefix::Utf32),
            ("L", Some('\'')) => return self.lex_char_from(start, CharLiteralPrefix::Wide),
            ("u", Some('\'')) => return self.lex_char_from(start, CharLiteralPrefix::Utf16),
            ("U", Some('\'')) => return self.lex_char_from(start, CharLiteralPrefix::Utf32),
            _ => {}
        }

        let kind = keyword_or_ident(&word, &mut self.string_table);
        self.push(start, kind);
    }

    fn lex_number(&mut self) {
        let start = self.peek_span();
        let mut raw = String::new();
        let mut has_dot = false;
        let mut has_exp = false;

        while let Some(ch) = self.peek_ch() {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                raw.push(ch);
                self.bump();
            } else if ch == '.' && !has_dot {
                has_dot = true;
                raw.push(ch);
                self.bump();
            } else if matches!(ch, '+' | '-')
                && matches!(raw.chars().last(), Some('e' | 'E' | 'p' | 'P'))
            {
                raw.push(ch);
                self.bump();
            } else {
                break;
            }

            if matches!(ch, 'e' | 'E' | 'p' | 'P') {
                has_exp = true;
            }
        }

        let lower = raw.to_ascii_lowercase();
        let is_hex_integer = lower.starts_with("0x") && !has_dot && !lower.contains(['p', 'P']);
        let is_float =
            !is_hex_integer && (has_dot || has_exp || lower.ends_with('f') || lower.ends_with('l'));
        let kind = if is_float {
            let trimmed = lower.trim_end_matches(['f', 'l']);
            let value = trimmed.parse::<f64>().unwrap_or(0.0);
            if lower.ends_with('f') {
                Value::Float(value as f32)
            } else if lower.ends_with('l') {
                Value::LongDouble(F80::from_f64(value))
            } else {
                Value::Double(value)
            }
        } else {
            let trimmed = lower.trim_end_matches(['u', 'l']);
            let value = parse_int_literal(trimmed);
            if lower.contains('u') {
                Value::ULongLong(value)
            } else {
                Value::LongLong(value as i64)
            }
        };

        self.push(start, TokenKind::ValueLiteral(kind));
    }

    fn lex_string(&mut self, prefix: StringLiteralPrefix) {
        let start = self.peek_span();
        self.lex_string_from(start, prefix);
    }

    fn lex_string_from(&mut self, start: (u32, u32), prefix: StringLiteralPrefix) {
        self.bump(); // opening quote
        let Some(value) = self.read_quoted('"', start, LexerErrors::UnterminatedString) else {
            return;
        };
        let id = self.string_table.intern(&value);
        self.push(
            start,
            TokenKind::StringLiteral(StringLiteral { prefix, value: id }),
        );
    }

    fn lex_char(&mut self, prefix: CharLiteralPrefix) {
        let start = self.peek_span();
        self.lex_char_from(start, prefix);
    }

    fn lex_char_from(&mut self, start: (u32, u32), prefix: CharLiteralPrefix) {
        self.bump(); // opening quote
        let Some(value) = self.read_quoted('\'', start, LexerErrors::UnterminatedChar) else {
            return;
        };
        let id = self.string_table.intern(&value);
        self.push(
            start,
            TokenKind::CharLiteral(CharLiteral { prefix, value: id }),
        );
    }

    fn read_quoted(
        &mut self,
        terminator: char,
        start: (u32, u32),
        err: LexerErrors,
    ) -> Option<String> {
        let mut value = String::new();
        while let Some(ch) = self.bump() {
            if ch == terminator {
                return Some(value);
            }
            if ch == '\n' {
                self.error(err, start);
                return None;
            }
            if ch == '\\' {
                if let Some(escaped) = self.bump() {
                    value.push(match escaped {
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        '0' => '\0',
                        '\\' => '\\',
                        '\'' => '\'',
                        '"' => '"',
                        other => other,
                    });
                } else {
                    self.error(err, start);
                    return None;
                }
            } else {
                value.push(ch);
            }
        }
        self.error(err, start);
        None
    }

    fn skip_line_comment(&mut self) {
        while let Some(ch) = self.peek_ch() {
            if ch == '\n' {
                break;
            }
            self.bump();
        }
    }

    fn skip_block_comment(&mut self) {
        let start = self.peek_span();
        self.bump();
        self.bump();
        while let Some(ch) = self.bump() {
            if ch == '*' && self.peek_ch() == Some('/') {
                self.bump();
                return;
            }
        }
        self.error(LexerErrors::UnterminatedBlockComment, start);
    }

    fn lex_punctuation_or_operator(&mut self) {
        let start = self.peek_span();
        let Some(ch) = self.bump() else {
            return;
        };

        let kind = match ch {
            '(' => TokenKind::LeftParen,
            ')' => TokenKind::RightParen,
            '{' => TokenKind::LeftBrace,
            '}' => TokenKind::RightBrace,
            '[' => TokenKind::LeftBracket,
            ']' => TokenKind::RightBracket,
            ';' => TokenKind::Semicolon,
            ',' => TokenKind::Comma,
            '?' => TokenKind::Question,
            ':' => TokenKind::Colon,
            '~' => TokenKind::Tilde,
            '.' if self.eat_if('.') && self.eat_if('.') => TokenKind::Ellipsis,
            '.' => TokenKind::Dot,
            '#' if self.eat_if('#') => TokenKind::HashHash,
            '#' => TokenKind::Hash,
            '+' if self.eat_if('+') => TokenKind::PlusPlus,
            '+' if self.eat_if('=') => TokenKind::PlusAssign,
            '+' => TokenKind::Plus,
            '-' if self.eat_if('-') => TokenKind::MinusMinus,
            '-' if self.eat_if('=') => TokenKind::MinusAssign,
            '-' if self.eat_if('>') => TokenKind::Arrow,
            '-' => TokenKind::Minus,
            '*' if self.eat_if('=') => TokenKind::StarAssign,
            '*' => TokenKind::Star,
            '/' if self.eat_if('=') => TokenKind::SlashAssign,
            '/' => TokenKind::Slash,
            '%' if self.eat_if('=') => TokenKind::PercentAssign,
            '%' => TokenKind::Percent,
            '=' if self.eat_if('=') => TokenKind::Equal,
            '=' => TokenKind::Assign,
            '!' if self.eat_if('=') => TokenKind::NotEqual,
            '!' => TokenKind::Bang,
            '<' => {
                if self.eat_if('<') {
                    if self.eat_if('=') {
                        TokenKind::LeftShiftAssign
                    } else {
                        TokenKind::LeftShift
                    }
                } else if self.eat_if('=') {
                    TokenKind::LessEqual
                } else {
                    TokenKind::LessThan
                }
            }
            '>' => {
                if self.eat_if('>') {
                    if self.eat_if('=') {
                        TokenKind::RightShiftAssign
                    } else {
                        TokenKind::RightShift
                    }
                } else if self.eat_if('=') {
                    TokenKind::GreaterEqual
                } else {
                    TokenKind::GreaterThan
                }
            }
            '&' if self.eat_if('&') => TokenKind::AmpAmp,
            '&' if self.eat_if('=') => TokenKind::AmpAssign,
            '&' => TokenKind::Amp,
            '|' if self.eat_if('|') => TokenKind::PipePipe,
            '|' if self.eat_if('=') => TokenKind::PipeAssign,
            '|' => TokenKind::Pipe,
            '^' if self.eat_if('=') => TokenKind::CaretAssign,
            '^' => TokenKind::Caret,
            other => {
                self.error(LexerErrors::UnexpectedChar(other), start);
                return;
            }
        };

        self.push(start, kind);
    }

    fn read_while(&mut self, mut pred: impl FnMut(char) -> bool) -> String {
        let mut value = String::new();
        while let Some(ch) = self.peek_ch() {
            if pred(ch) {
                value.push(ch);
                self.bump();
            } else {
                break;
            }
        }
        value
    }
}

fn keyword_or_ident(word: &str, strings: &mut StringTable) -> TokenKind {
    match word {
        "auto" => TokenKind::Auto,
        "break" => TokenKind::Break,
        "case" => TokenKind::Case,
        "default" => TokenKind::Default,
        "switch" => TokenKind::Switch,
        "while" => TokenKind::While,
        "do" => TokenKind::Do,
        "for" => TokenKind::For,
        "continue" => TokenKind::Continue,
        "if" => TokenKind::If,
        "else" => TokenKind::Else,
        "return" => TokenKind::Return,
        "goto" => TokenKind::Goto,
        "const" => TokenKind::Const,
        "enum" => TokenKind::Enum,
        "extern" => TokenKind::Extern,
        "inline" => TokenKind::Inline,
        "register" => TokenKind::Register,
        "restrict" => TokenKind::Restrict,
        "sizeof" => TokenKind::Sizeof,
        "static" => TokenKind::Static,
        "struct" => TokenKind::Struct,
        "typedef" => TokenKind::Typedef,
        "union" => TokenKind::Union,
        "volatile" => TokenKind::Volatile,
        "_Alignas" => TokenKind::Alignas,
        "_Alignof" => TokenKind::Alignof,
        "_Atomic" => TokenKind::Atomic,
        "_Generic" => TokenKind::Generic,
        "_Noreturn" => TokenKind::Noreturn,
        "_Static_assert" => TokenKind::StaticAssert,
        "_Thread_local" => TokenKind::ThreadLocal,
        "include" => TokenKind::Include,
        "define" => TokenKind::Define,
        "ifdef" => TokenKind::Ifdef,
        "ifndef" => TokenKind::Ifndef,
        "endif" => TokenKind::Endif,
        "pragma" => TokenKind::Pragma,
        "void" => TokenKind::Typ(Type::Void),
        "char" => TokenKind::Typ(Type::Char),
        "int" => TokenKind::Typ(Type::Int),
        "short" => TokenKind::Typ(Type::Short),
        "long" => TokenKind::Typ(Type::Long),
        "float" => TokenKind::Typ(Type::Float),
        "double" => TokenKind::Typ(Type::Double),
        "_Bool" => TokenKind::Typ(Type::Bool),
        "signed" => TokenKind::Typ(Type::Int),
        "unsigned" => TokenKind::Typ(Type::UnsignedInt),
        _ => TokenKind::Ident(strings.intern(word)),
    }
}

fn parse_int_literal(raw: &str) -> u64 {
    if let Some(rest) = raw.strip_prefix("0x") {
        u64::from_str_radix(rest, 16).unwrap_or(0)
    } else if let Some(rest) = raw.strip_prefix("0b") {
        u64::from_str_radix(rest, 2).unwrap_or(0)
    } else if raw.starts_with('0') && raw.len() > 1 {
        u64::from_str_radix(&raw[1..], 8).unwrap_or(0)
    } else {
        raw.parse::<u64>().unwrap_or(0)
    }
}

fn is_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

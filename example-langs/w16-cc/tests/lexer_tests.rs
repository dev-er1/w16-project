//! Тесты лексера w16-cc.

use w16_cc::frontend::{
    lexer::{
        token::{CharLiteralPrefix, StringLiteralPrefix, TokenKind},
        Lexer,
    },
    string_pool::StringTable,
};
use w16_cc::{types::Type, value::Value};

fn lex(src: &str) -> (Vec<TokenKind>, StringTable) {
    let (tokens, strings) = Lexer::new(src, StringTable::new())
        .tokenize()
        .expect("lexer should succeed");
    (
        tokens.into_iter().map(|token| token.kind).collect(),
        strings,
    )
}

#[test]
fn lexes_keywords_types_and_identifiers() {
    let (tokens, strings) = lex("int main return value");

    assert_eq!(tokens[0], TokenKind::Typ(Type::Int));
    assert!(matches!(tokens[1], TokenKind::Ident(_)));
    assert_eq!(tokens[2], TokenKind::Return);
    assert!(matches!(tokens[3], TokenKind::Ident(_)));
    assert_eq!(tokens.last(), Some(&TokenKind::EndOfCode));

    let TokenKind::Ident(main_id) = tokens[1] else {
        unreachable!();
    };
    assert_eq!(strings.resolve(main_id), Some("main"));
}

#[test]
fn lexes_common_operators_with_longest_match() {
    let (tokens, _) = lex("a++ b += 2 << 1 <<= 3 != 0 && ok || no -> field ...");

    assert!(tokens.contains(&TokenKind::PlusPlus));
    assert!(tokens.contains(&TokenKind::PlusAssign));
    assert!(tokens.contains(&TokenKind::LeftShift));
    assert!(tokens.contains(&TokenKind::LeftShiftAssign));
    assert!(tokens.contains(&TokenKind::NotEqual));
    assert!(tokens.contains(&TokenKind::AmpAmp));
    assert!(tokens.contains(&TokenKind::PipePipe));
    assert!(tokens.contains(&TokenKind::Arrow));
    assert!(tokens.contains(&TokenKind::Ellipsis));
}

#[test]
fn lexes_newline_and_preprocessor_directives() {
    let (tokens, _) = lex("#include <stdio.h>\n#define N 10\n#ifdef N\n#endif");

    assert_eq!(tokens[0], TokenKind::Hash);
    assert_eq!(tokens[1], TokenKind::Include);
    assert!(tokens.contains(&TokenKind::NewLine));
    assert!(tokens.contains(&TokenKind::Define));
    assert!(tokens.contains(&TokenKind::Ifdef));
    assert!(tokens.contains(&TokenKind::Endif));
}

#[test]
fn lexes_string_and_char_prefixes() {
    let (tokens, strings) = lex(r#""a" u8"b" L"c" u"d" U"e" 'x' L'y' u'z' U'w'"#);

    let TokenKind::StringLiteral(normal) = tokens[0] else {
        panic!("expected string literal");
    };
    let TokenKind::StringLiteral(utf8) = tokens[1] else {
        panic!("expected utf8 string literal");
    };
    let TokenKind::StringLiteral(wide) = tokens[2] else {
        panic!("expected wide string literal");
    };
    let TokenKind::StringLiteral(utf16) = tokens[3] else {
        panic!("expected utf16 string literal");
    };
    let TokenKind::StringLiteral(utf32) = tokens[4] else {
        panic!("expected utf32 string literal");
    };

    assert_eq!(normal.prefix, StringLiteralPrefix::None);
    assert_eq!(utf8.prefix, StringLiteralPrefix::Utf8);
    assert_eq!(wide.prefix, StringLiteralPrefix::Wide);
    assert_eq!(utf16.prefix, StringLiteralPrefix::Utf16);
    assert_eq!(utf32.prefix, StringLiteralPrefix::Utf32);
    assert_eq!(strings.resolve(normal.value), Some("a"));

    let TokenKind::CharLiteral(normal_ch) = tokens[5] else {
        panic!("expected char literal");
    };
    let TokenKind::CharLiteral(wide_ch) = tokens[6] else {
        panic!("expected wide char literal");
    };
    let TokenKind::CharLiteral(utf16_ch) = tokens[7] else {
        panic!("expected utf16 char literal");
    };
    let TokenKind::CharLiteral(utf32_ch) = tokens[8] else {
        panic!("expected utf32 char literal");
    };

    assert_eq!(normal_ch.prefix, CharLiteralPrefix::None);
    assert_eq!(wide_ch.prefix, CharLiteralPrefix::Wide);
    assert_eq!(utf16_ch.prefix, CharLiteralPrefix::Utf16);
    assert_eq!(utf32_ch.prefix, CharLiteralPrefix::Utf32);
    assert_eq!(strings.resolve(normal_ch.value), Some("x"));
}

#[test]
fn skips_line_and_block_comments() {
    let (tokens, _) = lex("int a; // comment\nint b; /* comment */ int c;");

    let int_count = tokens
        .iter()
        .filter(|kind| **kind == TokenKind::Typ(Type::Int))
        .count();
    assert_eq!(int_count, 3);
}

#[test]
fn lexes_basic_number_literals() {
    let (tokens, _) = lex("42 0xff 077 0b101 1.5 2.0f 3.0L");

    assert!(matches!(
        tokens[0],
        TokenKind::ValueLiteral(Value::LongLong(42))
    ));
    assert!(matches!(
        tokens[1],
        TokenKind::ValueLiteral(Value::LongLong(255))
    ));
    assert!(matches!(
        tokens[2],
        TokenKind::ValueLiteral(Value::LongLong(63))
    ));
    assert!(matches!(
        tokens[3],
        TokenKind::ValueLiteral(Value::LongLong(5))
    ));
    assert!(matches!(
        tokens[4],
        TokenKind::ValueLiteral(Value::Double(_))
    ));
    assert!(matches!(
        tokens[5],
        TokenKind::ValueLiteral(Value::Float(_))
    ));
    assert!(matches!(
        tokens[6],
        TokenKind::ValueLiteral(Value::LongDouble(_))
    ));
}

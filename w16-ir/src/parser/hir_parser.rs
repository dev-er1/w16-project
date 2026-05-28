// w16-ir\src\parser\hir_parser.rs
//
//! # Recursive-descent parser для W16-HIR.
//!
//! Этот parser читает token stream и строит `hir::Module`. Для выражений
//! используется precedence climbing: это компактный способ разобрать бинарные
//! операции с приоритетами без отдельной грамматики на каждый уровень.

use crate::hir::*;
use crate::lexer::{Lexer, Token, TokenKind};
use crate::parser::ParseError;

/// Полный frontend-вход для HIR: source text -> tokens -> HIR module.
pub fn parse_hir_module(source: &str) -> Result<Module, ParseError> {
    let tokens = Lexer::new(source)
        .tokenize()
        .map_err(|error| ParseError::new(error.span, error.message))?;
    Parser::new(tokens).parse_module()
}

/// Parser с текущей позицией в token stream.
pub struct Parser {
    /// Все токены, включая финальный `Eof`.
    tokens: Vec<Token>,
    /// Индекс текущего токена.
    pos: usize,
}

impl Parser {
    /// Создать parser из готового списка токенов.
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    /// Разобрать полный `module`.
    pub fn parse_module(&mut self) -> Result<Module, ParseError> {
        self.expect_simple(&TokenKind::Module, "expected `module`")?;
        let name = self.expect_ident("expected module name")?;
        self.expect_simple(&TokenKind::LBrace, "expected `{` after module name")?;

        let mut constants = Vec::new();
        let mut functions = Vec::new();

        while !self.at(&TokenKind::RBrace) && !self.at(&TokenKind::Eof) {
            if self.at(&TokenKind::Const) {
                constants.push(self.parse_const()?);
            } else if self.at(&TokenKind::Fn) {
                functions.push(self.parse_function()?);
            } else {
                return Err(self.error_here("expected `const` or `fn` item"));
            }
        }

        self.expect_simple(&TokenKind::RBrace, "expected `}` after module")?;
        self.expect_simple(&TokenKind::Eof, "expected end of file")?;

        Ok(Module {
            name,
            constants,
            functions,
        })
    }

    fn parse_const(&mut self) -> Result<ConstDecl, ParseError> {
        self.expect_simple(&TokenKind::Const, "expected `const`")?;
        let name = self.expect_ident("expected constant name")?;
        self.expect_simple(&TokenKind::Colon, "expected `:` after constant name")?;
        let ty = self.parse_type()?;
        self.expect_simple(&TokenKind::Equal, "expected `=` after constant type")?;
        let value = self.parse_literal()?;
        self.eat_optional_semicolon();
        Ok(ConstDecl { name, ty, value })
    }

    fn parse_function(&mut self) -> Result<Function, ParseError> {
        self.expect_simple(&TokenKind::Fn, "expected `fn`")?;
        let name = self.expect_function("expected function name like `@main`")?;
        self.expect_simple(&TokenKind::LParen, "expected `(` after function name")?;
        let params = self.parse_params()?;
        self.expect_simple(&TokenKind::RParen, "expected `)` after parameters")?;
        self.expect_simple(&TokenKind::Arrow, "expected `->` after parameters")?;
        let return_ty = self.parse_return_type()?;
        let body = self.parse_block_body()?;

        Ok(Function {
            name,
            params,
            return_ty,
            body,
        })
    }

    fn parse_params(&mut self) -> Result<Vec<Param>, ParseError> {
        let mut params = Vec::new();
        if self.at(&TokenKind::RParen) {
            return Ok(params);
        }

        loop {
            let name = self.expect_local("expected parameter name like `$x`")?;
            self.expect_simple(&TokenKind::Colon, "expected `:` after parameter name")?;
            let ty = self.parse_type()?;
            params.push(Param { name, ty });

            if !self.eat(&TokenKind::Comma) {
                return Ok(params);
            }
        }
    }

    fn parse_return_type(&mut self) -> Result<ReturnType, ParseError> {
        if self.eat(&TokenKind::LParen) {
            if self.eat(&TokenKind::RParen) {
                return Ok(ReturnType::Unit);
            }
            let mut types = Vec::new();
            loop {
                types.push(self.parse_type()?);
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect_simple(&TokenKind::RParen, "expected `)` after return tuple")?;
            Ok(ReturnType::Tuple(types))
        } else {
            let ty = self.parse_type()?;
            if ty == Type::Unit {
                Ok(ReturnType::Unit)
            } else {
                Ok(ReturnType::Single(ty))
            }
        }
    }

    fn parse_block_body(&mut self) -> Result<Vec<Stmt>, ParseError> {
        self.expect_simple(&TokenKind::LBrace, "expected `{`")?;
        let mut stmts = Vec::new();
        while !self.at(&TokenKind::RBrace) && !self.at(&TokenKind::Eof) {
            stmts.push(self.parse_stmt()?);
        }
        self.expect_simple(&TokenKind::RBrace, "expected `}`")?;
        Ok(stmts)
    }

    fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        // Операторы с ключевым словом проще распознать первыми. Assignment
        // проверяем отдельно по lookahead: `$name = ...`.
        if self.at(&TokenKind::Let) {
            self.bump();
            let name = self.expect_local("expected local name after `let`")?;
            self.expect_simple(&TokenKind::Colon, "expected `:` after local name")?;
            let ty = self.parse_type()?;
            self.expect_simple(&TokenKind::Equal, "expected `=` after local type")?;
            let value = self.parse_expr()?;
            self.eat_optional_semicolon();
            return Ok(Stmt::Let { name, ty, value });
        }

        if self.at(&TokenKind::If) {
            self.bump();
            self.expect_simple(&TokenKind::LParen, "expected `(` after `if`")?;
            let cond = self.parse_expr()?;
            self.expect_simple(&TokenKind::RParen, "expected `)` after if condition")?;
            let then_body = self.parse_block_body()?;
            let else_body = if self.eat(&TokenKind::Else) {
                self.parse_block_body()?
            } else {
                Vec::new()
            };
            return Ok(Stmt::If {
                cond,
                then_body,
                else_body,
            });
        }

        if self.at(&TokenKind::Do) {
            self.bump();
            let body = self.parse_block_body()?;
            self.expect_simple(&TokenKind::While, "expected `while` after `do` body")?;
            self.expect_simple(&TokenKind::LParen, "expected `(`")?;
            let cond = self.parse_expr()?;
            self.expect_simple(&TokenKind::RParen, "expected `)`")?;
            self.eat_optional_semicolon();
            return Ok(Stmt::DoWhile { body, cond });
        }

        if self.at(&TokenKind::While) {
            self.bump();
            self.expect_simple(&TokenKind::LParen, "expected `(` after `while`")?;
            let cond = self.parse_expr()?;
            self.expect_simple(&TokenKind::RParen, "expected `)` after while condition")?;
            let body = self.parse_block_body()?;
            return Ok(Stmt::While { cond, body });
        }

        if self.at(&TokenKind::Return) {
            self.bump();
            if self.eat(&TokenKind::LParen) {
                if self.eat(&TokenKind::RParen) {
                    self.eat_optional_semicolon();
                    return Ok(Stmt::Return(Vec::new()));
                }

                let mut values = Vec::new();
                loop {
                    values.push(self.parse_expr()?);
                    if !self.eat(&TokenKind::Comma) {
                        break;
                    }
                }
                self.expect_simple(&TokenKind::RParen, "expected `)` after return values")?;
                self.eat_optional_semicolon();
                return Ok(Stmt::Return(values));
            }

            let value = self.parse_expr()?;
            self.eat_optional_semicolon();
            return Ok(Stmt::Return(vec![value]));
        }

        if self.at(&TokenKind::Break) {
            self.bump();
            self.eat_optional_semicolon();
            return Ok(Stmt::Break);
        }
        if self.at(&TokenKind::Continue) {
            self.bump();
            self.eat_optional_semicolon();
            return Ok(Stmt::Continue);
        }

        if self.at(&TokenKind::Halt) {
            self.bump();
            self.eat_optional_semicolon();
            return Ok(Stmt::Halt);
        }

        if self.at(&TokenKind::Print) {
            self.bump();
            self.expect_simple(&TokenKind::LParen, "expected `(` after `print`")?;
            if self.eat(&TokenKind::RParen) {
                self.eat_optional_semicolon();
                return Ok(Stmt::Print(Vec::new()));
            }

            let mut args = Vec::new();
            loop {
                args.push(self.parse_expr()?);
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect_simple(&TokenKind::RParen, "expected `)` after print arguments")?;
            self.eat_optional_semicolon();
            return Ok(Stmt::Print(args));
        }

        if let TokenKind::Local(name) = self.peek().kind.clone()
            && self.peek_n(1).map(|t| &t.kind) == Some(&TokenKind::Equal)
        {
            self.bump();
            self.bump();
            let value = self.parse_expr()?;
            self.eat_optional_semicolon();
            return Ok(Stmt::Assign { name, value });
        }

        let expr = self.parse_expr()?;
        self.eat_optional_semicolon();
        Ok(Stmt::Expr(expr))
    }

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_binary_expr(0)
    }

    fn parse_binary_expr(&mut self, min_prec: u8) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_prefix_expr()?;

        loop {
            let Some((op, prec)) = self.current_binary_op() else {
                break;
            };
            if prec < min_prec {
                break;
            }
            self.bump();
            // Все текущие бинарные операции left-associative, поэтому RHS
            // парсим с `prec + 1`.
            let rhs = self.parse_binary_expr(prec + 1)?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }

        Ok(lhs)
    }

    fn parse_prefix_expr(&mut self) -> Result<Expr, ParseError> {
        if self.eat(&TokenKind::Bang) {
            return Ok(Expr::Unary {
                op: UnaryOp::Not,
                expr: Box::new(self.parse_prefix_expr()?),
            });
        }
        if self.eat(&TokenKind::Minus) {
            return Ok(Expr::Unary {
                op: UnaryOp::Neg,
                expr: Box::new(self.parse_prefix_expr()?),
            });
        }
        self.parse_primary_expr()
    }

    fn parse_primary_expr(&mut self) -> Result<Expr, ParseError> {
        match self.peek().kind.clone() {
            TokenKind::Int(value) => {
                self.bump();
                Ok(Expr::Literal(Literal::Int(value)))
            }
            TokenKind::Float(value) => {
                self.bump();
                Ok(Expr::Literal(Literal::Float(value)))
            }
            TokenKind::String(value) => {
                self.bump();
                Ok(Expr::Literal(Literal::String(value)))
            }
            TokenKind::True => {
                self.bump();
                Ok(Expr::Literal(Literal::Bool(true)))
            }
            TokenKind::False => {
                self.bump();
                Ok(Expr::Literal(Literal::Bool(false)))
            }
            TokenKind::Local(name) => {
                self.bump();
                Ok(Expr::Local(name))
            }
            TokenKind::Ident(name) => self.parse_ident_expr(name),
            TokenKind::Function(function) => {
                self.bump();
                self.expect_simple(&TokenKind::LParen, "expected `(` after function name")?;
                let args = self.parse_expr_list(TokenKind::RParen)?;
                self.expect_simple(&TokenKind::RParen, "expected `)` after call arguments")?;
                Ok(Expr::Call { function, args })
            }
            TokenKind::LParen => {
                self.bump();
                let expr = self.parse_expr()?;
                self.expect_simple(&TokenKind::RParen, "expected `)` after expression")?;
                Ok(expr)
            }
            _ => Err(self.error_here("expected expression")),
        }
    }

    fn parse_ident_expr(&mut self, name: String) -> Result<Expr, ParseError> {
        self.bump();
        // `select`, `cast.*`, `load.*`, `store.*` выглядят как обычные
        // identifier-выражения на уровне lexer, поэтому parser распознаёт их
        // здесь как специальные формы HIR.
        if name == "select" {
            self.expect_simple(&TokenKind::LParen, "expected `(` after `select`")?;
            let cond = self.parse_expr()?;
            self.expect_simple(&TokenKind::Comma, "expected `,` after select condition")?;
            let then_value = self.parse_expr()?;
            self.expect_simple(&TokenKind::Comma, "expected `,` after select then value")?;
            let else_value = self.parse_expr()?;
            self.expect_simple(&TokenKind::RParen, "expected `)` after select")?;
            return Ok(Expr::Select {
                cond: Box::new(cond),
                then_value: Box::new(then_value),
                else_value: Box::new(else_value),
            });
        }

        if matches!(name.as_str(), "cast" | "load" | "store") && self.eat(&TokenKind::Dot) {
            let suffix = self.expect_ident("expected suffix after `.`")?;
            return match name.as_str() {
                "cast" => {
                    let kind = parse_cast_kind(&suffix)
                        .ok_or_else(|| self.error_here(format!("unknown cast kind `{suffix}`")))?;
                    self.expect_simple(&TokenKind::LParen, "expected `(` after cast kind")?;
                    let expr = self.parse_expr()?;
                    self.expect_simple(&TokenKind::RParen, "expected `)` after cast")?;
                    Ok(Expr::Cast {
                        kind,
                        expr: Box::new(expr),
                    })
                }
                "load" => {
                    let ty = parse_type_name(&suffix)
                        .ok_or_else(|| self.error_here(format!("unknown load type `{suffix}`")))?;
                    self.expect_simple(&TokenKind::LParen, "expected `(` after load type")?;
                    let addr = self.parse_expr()?;
                    self.expect_simple(&TokenKind::RParen, "expected `)` after load")?;
                    Ok(Expr::Load {
                        ty,
                        addr: Box::new(addr),
                    })
                }
                "store" => {
                    let ty = parse_type_name(&suffix)
                        .ok_or_else(|| self.error_here(format!("unknown store type `{suffix}`")))?;
                    self.expect_simple(&TokenKind::LParen, "expected `(` after store type")?;
                    let addr = self.parse_expr()?;
                    self.expect_simple(&TokenKind::Comma, "expected `,` after store address")?;
                    let value = self.parse_expr()?;
                    self.expect_simple(&TokenKind::RParen, "expected `)` after store")?;
                    Ok(Expr::Store {
                        ty,
                        addr: Box::new(addr),
                        value: Box::new(value),
                    })
                }
                _ => unreachable!(),
            };
        }

        Ok(Expr::Const(name))
    }

    fn parse_expr_list(&mut self, end: TokenKind) -> Result<Vec<Expr>, ParseError> {
        let mut args = Vec::new();
        if self.at(&end) {
            return Ok(args);
        }
        loop {
            args.push(self.parse_expr()?);
            if !self.eat(&TokenKind::Comma) {
                return Ok(args);
            }
        }
    }

    fn parse_type(&mut self) -> Result<Type, ParseError> {
        let name = self.expect_ident("expected type")?;
        parse_type_name(&name).ok_or_else(|| self.error_here(format!("unknown type `{name}`")))
    }

    fn parse_literal(&mut self) -> Result<Literal, ParseError> {
        match self.peek().kind.clone() {
            TokenKind::Int(value) => {
                self.bump();
                Ok(Literal::Int(value))
            }
            TokenKind::Float(value) => {
                self.bump();
                Ok(Literal::Float(value))
            }
            TokenKind::String(value) => {
                self.bump();
                Ok(Literal::String(value))
            }
            TokenKind::True => {
                self.bump();
                Ok(Literal::Bool(true))
            }
            TokenKind::False => {
                self.bump();
                Ok(Literal::Bool(false))
            }
            _ => Err(self.error_here("expected literal")),
        }
    }

    fn current_binary_op(&self) -> Option<(BinaryOp, u8)> {
        let op = match self.peek().kind {
            TokenKind::Pipe => (BinaryOp::BitOr, 1),
            TokenKind::Caret => (BinaryOp::BitXor, 2),
            TokenKind::Amp => (BinaryOp::BitAnd, 3),
            TokenKind::EqualEqual => (BinaryOp::Eq, 4),
            TokenKind::BangEqual => (BinaryOp::Ne, 4),
            TokenKind::Less => (BinaryOp::Lt, 5),
            TokenKind::LessEqual => (BinaryOp::Le, 5),
            TokenKind::Greater => (BinaryOp::Gt, 5),
            TokenKind::GreaterEqual => (BinaryOp::Ge, 5),
            TokenKind::Plus => (BinaryOp::Add, 6),
            TokenKind::Minus => (BinaryOp::Sub, 6),
            TokenKind::Shl => (BinaryOp::Shl, 6),
            TokenKind::Shr => (BinaryOp::Shr, 6),
            TokenKind::Star => (BinaryOp::Mul, 7),
            TokenKind::Slash => (BinaryOp::Div, 7),
            TokenKind::Percent => (BinaryOp::Rem, 7),
            _ => return None,
        };
        Some(op)
    }

    fn expect_ident(&mut self, message: impl Into<String>) -> Result<String, ParseError> {
        match self.peek().kind.clone() {
            TokenKind::Ident(name) => {
                self.bump();
                Ok(name)
            }
            _ => Err(self.error_here(message)),
        }
    }

    fn expect_local(&mut self, message: impl Into<String>) -> Result<String, ParseError> {
        match self.peek().kind.clone() {
            TokenKind::Local(name) => {
                self.bump();
                Ok(name)
            }
            _ => Err(self.error_here(message)),
        }
    }

    fn expect_function(&mut self, message: impl Into<String>) -> Result<String, ParseError> {
        match self.peek().kind.clone() {
            TokenKind::Function(name) => {
                self.bump();
                Ok(name)
            }
            _ => Err(self.error_here(message)),
        }
    }

    fn expect_simple(
        &mut self,
        expected: &TokenKind,
        message: impl Into<String>,
    ) -> Result<(), ParseError> {
        if self.eat(expected) {
            Ok(())
        } else {
            Err(self.error_here(message))
        }
    }

    fn eat_optional_semicolon(&mut self) {
        self.eat(&TokenKind::Semicolon);
    }

    fn eat(&mut self, expected: &TokenKind) -> bool {
        if self.at(expected) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn at(&self, expected: &TokenKind) -> bool {
        // Для simple tokens достаточно сравнить discriminant. Токены с данными
        // (`Ident`, `Local`, literals) проверяются отдельными expect_* helper-ами.
        std::mem::discriminant(&self.peek().kind) == std::mem::discriminant(expected)
    }

    fn bump(&mut self) {
        if !self.at(&TokenKind::Eof) {
            self.pos += 1;
        }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn peek_n(&self, offset: usize) -> Option<&Token> {
        self.tokens.get(self.pos + offset)
    }

    fn error_here(&self, message: impl Into<String>) -> ParseError {
        ParseError::new(self.peek().span, message)
    }
}

fn parse_type_name(name: &str) -> Option<Type> {
    match name {
        "i64" => Some(Type::I64),
        "u64" => Some(Type::U64),
        "f64" => Some(Type::F64),
        "bool" => Some(Type::Bool),
        "ptr" => Some(Type::Ptr),
        "unit" => Some(Type::Unit),
        _ => None,
    }
}

fn parse_cast_kind(name: &str) -> Option<CastKind> {
    match name {
        "i2f" => Some(CastKind::I2F),
        "u2f" => Some(CastKind::U2F),
        "f2i" => Some(CastKind::F2I),
        "f2u" => Some(CastKind::F2U),
        "i2u" => Some(CastKind::I2U),
        "u2i" => Some(CastKind::U2I),
        "trunc_u64_to_u32" => Some(CastKind::TruncU64ToU32),
        "zext_u32_to_u64" => Some(CastKind::ZextU32ToU64),
        "sext_i32_to_i64" => Some(CastKind::SextI32ToI64),
        "bitcast" => Some(CastKind::Bitcast),
        _ => None,
    }
}

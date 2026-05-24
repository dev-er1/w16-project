//! # Парсер C11
//!
//! Рекурсивный нисходящий парсер для операторов и объявлений.
//! Алгоритм Пратта (Pratt parsing) для выражений — корректно
//! обрабатывает приоритеты и ассоциативность без бесконечной рекурсии.
//!
//! ## Ограничения текущей версии
//! - Указатели не поддерживаются.
//! - Препроцессор не обрабатывается (токены `Include`, `Define` и т.д. пропускаются).
//! - `_Generic` поддерживается на уровне AST, но семантика не проверяется.
pub mod node;

use node::*;
use std::fmt;

use crate::frontend::lexer::token::{Span, Token, TokenKind};
use crate::frontend::string_pool::StringId;
use crate::types::Type;

// ---------------------------------------------------------------------------
// Ошибки парсера
// ---------------------------------------------------------------------------

/// Виды ошибок
#[derive(Debug, Clone, Copy)]
pub enum ParseErrors {
    ExpectedExpressionButGot,
    ExpectedIdentButGot,
    ExpectedButGot,
    ExpectedTypeButGot,
    ExpectedStringLiteralButGot
}

#[derive(Debug, Clone)]
pub struct ParseError {
    pub pos: Span,
    pub message: ParseErrors,
    pub got: TokenKind,
    pub expected: Option<TokenKind>
}

impl ParseError {
    fn new(pos: Span, message: ParseErrors, got: TokenKind, expected: Option<TokenKind>) -> Self {
        Self { pos, message, got, expected }
    }
}

impl fmt::Display for ParseErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExpectedExpressionButGot => write!(f, "expected expression"),
            Self::ExpectedIdentButGot => write!(f, "expected identifier"),
            Self::ExpectedButGot => write!(f, "expected token missing"),
            Self::ExpectedTypeButGot => write!(f, "expected type specifier"),
            Self::ExpectedStringLiteralButGot => write!(f, "expected string literal"),
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(expected) = &self.expected {
            write!(f, " (expected `{expected:?}`, found `{:?}`)", self.got)
        } else {
            write!(f, " (found `{:?}`)", self.got)
        }
    }
}

pub type ParseResult<T> = Result<T, ParseError>;

// ---------------------------------------------------------------------------
// Парсер
// ---------------------------------------------------------------------------

pub struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        // Убираем NewLine-токены — они нужны только препроцессору.
        let tokens = tokens
            .into_iter()
            .filter(|t| !matches!(t.kind, TokenKind::NewLine))
            .collect();
        Self { tokens, cursor: 0 }
    }

    /// Точка входа — парсит весь файл в [`TranslationUnit`].
    pub fn parse(&mut self) -> ParseResult<TranslationUnit> {
        let mut items = Vec::new();

        while !self.at_end() {
            // Пропускаем препроцессорные токены верхнего уровня.
            if self.skip_preprocessor() {
                continue;
            }
            items.push(self.parse_external_decl()?);
        }

        Ok(TranslationUnit { items })
    }

    // -----------------------------------------------------------------------
    // Верхний уровень
    // -----------------------------------------------------------------------

    fn parse_external_decl(&mut self) -> ParseResult<ExternalDecl> {
        // Пробуем определить: это функция или объявление?
        // Смотрим вперёд: если после типа+имени идёт `(` — функция.
        if self.is_function_def() {
            Ok(ExternalDecl::FunctionDef(self.parse_function_def()?))
        } else {
            Ok(ExternalDecl::Decl(self.parse_decl()?))
        }
    }

    /// Эвристика: смотрим вперёд чтобы понять, является ли текущая позиция
    /// началом определения функции.
    fn is_function_def(&self) -> bool {
        let mut i = self.cursor;

        // Пропускаем спецификаторы (static, inline, extern, _Noreturn).
        while i < self.tokens.len() && Self::is_decl_specifier(&self.tokens[i].kind) {
            i += 1;
        }

        // Пропускаем тип.
        while i < self.tokens.len() && Self::is_type_token(&self.tokens[i].kind) {
            i += 1;
        }

        // Пропускаем имя функции.
        if i < self.tokens.len() && matches!(self.tokens[i].kind, TokenKind::Ident(_)) {
            i += 1;
        } else {
            return false;
        }

        // Если за именем идёт `(` — это функция.
        i < self.tokens.len() && matches!(self.tokens[i].kind, TokenKind::LeftParen)
    }

    // -----------------------------------------------------------------------
    // Функция
    // -----------------------------------------------------------------------

    fn parse_function_def(&mut self) -> ParseResult<FunctionDef> {
        let start = self.current_span();

        let (is_static, is_inline, is_noreturn) = self.parse_fn_specifiers();
        let return_ty = self.parse_type()?;
        let name = self.expect_ident()?;

        self.expect(TokenKind::LeftParen)?;
        let params = self.parse_param_list()?;
        self.expect(TokenKind::RightParen)?;

        let body = self.parse_block()?;

        Ok(FunctionDef {
            span: self.merge_span(start, body.span),
            return_ty,
            name,
            params,
            body,
            is_inline,
            is_static,
            is_noreturn,
        })
    }

    fn parse_fn_specifiers(&mut self) -> (bool, bool, bool) {
        let (mut is_static, mut is_inline, mut is_noreturn) = (false, false, false);
        loop {
            match self.peek_kind() {
                TokenKind::Static => { self.advance(); is_static = true; }
                TokenKind::Inline => { self.advance(); is_inline = true; }
                TokenKind::Noreturn => { self.advance(); is_noreturn = true; }
                TokenKind::Extern => { self.advance(); /* игнорируем в определениях */ }
                _ => break,
            }
        }
        (is_static, is_inline, is_noreturn)
    }

    fn parse_param_list(&mut self) -> ParseResult<Vec<Param>> {
        let mut params = Vec::new();

        // `void` как единственный параметр → пустой список.
        if matches!(self.peek_kind(), TokenKind::Typ(Type::Void))
            && self.peek_kind_at(1) == &TokenKind::RightParen
        {
            self.advance();
            return Ok(params);
        }

        if matches!(self.peek_kind(), TokenKind::RightParen) {
            return Ok(params);
        }

        loop {
            let span = self.current_span();
            let ty = self.parse_type()?;
            let name = if matches!(self.peek_kind(), TokenKind::Ident(_)) {
                Some(self.expect_ident()?)
            } else {
                None
            };
            params.push(Param { span, ty, name });

            if !self.eat(TokenKind::Comma) {
                break;
            }
            // Вариадик `...` — просто пропускаем для совместимости.
            if matches!(self.peek_kind(), TokenKind::Ellipsis) {
                self.advance();
                break;
            }
        }

        Ok(params)
    }

    // -----------------------------------------------------------------------
    // Объявления
    // -----------------------------------------------------------------------

    fn parse_decl(&mut self) -> ParseResult<Decl> {
        let start = self.current_span();

        // typedef
        if matches!(self.peek_kind(), TokenKind::Typedef) {
            self.advance();
            let ty = self.parse_type()?;
            let alias = self.expect_ident()?;
            self.expect(TokenKind::Semicolon)?;
            return Ok(Decl {
                span: start,
                kind: DeclKind::Typedef { ty, alias },
            });
        }

        // struct / union / enum на верхнем уровне (только определение без переменной)
        if let Some(kind) = self.try_parse_tag_def()? {
            self.expect(TokenKind::Semicolon)?;
            return Ok(Decl { span: start, kind });
        }

        // Обычная переменная или список переменных.
        let (storage, qualifiers) = self.parse_storage_and_qualifiers();
        let ty = self.parse_type()?;

        let mut vars = Vec::new();
        loop {
            vars.push(self.parse_var_declarator(start, ty.clone(), storage, qualifiers)?);
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::Semicolon)?;

        let kind = if vars.len() == 1 {
            DeclKind::Var(vars.remove(0))
        } else {
            DeclKind::MultiVar(vars)
        };

        Ok(Decl { span: start, kind })
    }

    fn parse_var_declarator(
        &mut self,
        span: Span,
        ty: Type,
        storage: StorageClass,
        qualifiers: TypeQualifiers,
    ) -> ParseResult<VarDecl> {
        let name = self.expect_ident()?;

        // Массив: `int a[10]`
        let ty = if self.eat(TokenKind::LeftBracket) {
            self.expect(TokenKind::RightBracket)?;
            // Размер массива должен быть константой — семантика проверит позже.
            Type::Array(Box::new(ty), 0 /* заполнит семантика */)
        } else {
            ty
        };

        let initializer = if self.eat(TokenKind::Assign) {
            Some(self.parse_initializer()?)
        } else {
            None
        };

        Ok(VarDecl { span, ty, name, initializer, storage, qualifiers })
    }

    fn parse_initializer(&mut self) -> ParseResult<Initializer> {
        if self.eat(TokenKind::LeftBrace) {
            // Агрегатный инициализатор: `{ 1, 2, 3 }`
            let mut list = Vec::new();
            while !matches!(self.peek_kind(), TokenKind::RightBrace | TokenKind::EndOfCode) {
                list.push(self.parse_initializer()?);
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::RightBrace)?;
            Ok(Initializer::List(list))
        } else {
            Ok(Initializer::Expr(self.parse_assign_expr()?))
        }
    }

    fn parse_storage_and_qualifiers(&mut self) -> (StorageClass, TypeQualifiers) {
        let mut storage = StorageClass::Auto;
        let mut qualifiers = TypeQualifiers::default();

        loop {
            match self.peek_kind() {
                TokenKind::Static => { self.advance(); storage = StorageClass::Static; }
                TokenKind::Extern => { self.advance(); storage = StorageClass::Extern; }
                TokenKind::Register => { self.advance(); storage = StorageClass::Register; }
                TokenKind::Auto => { self.advance(); /* Auto по умолчанию */ }
                TokenKind::Const => { self.advance(); qualifiers.is_const = true; }
                TokenKind::Volatile => { self.advance(); qualifiers.is_volatile = true; }
                TokenKind::Restrict => { self.advance(); qualifiers.is_restrict = true; }
                TokenKind::Atomic => { self.advance(); qualifiers.is_atomic = true; }
                _ => break,
            }
        }

        (storage, qualifiers)
    }

    /// Пробует разобрать определение struct/union/enum.
    /// Возвращает `None` если текущий токен не является тегом.
    fn try_parse_tag_def(&mut self) -> ParseResult<Option<DeclKind>> {
        match self.peek_kind() {
            TokenKind::Struct => {
                self.advance();
                Ok(Some(DeclKind::StructDef(self.parse_struct_body()?)))
            }
            TokenKind::Union => {
                self.advance();
                Ok(Some(DeclKind::UnionDef(self.parse_union_body()?)))
            }
            TokenKind::Enum => {
                self.advance();
                Ok(Some(DeclKind::EnumDef(self.parse_enum_body()?)))
            }
            _ => Ok(None),
        }
    }

    fn parse_struct_body(&mut self) -> ParseResult<StructDef> {
        let span = self.current_span();
        let name = if matches!(self.peek_kind(), TokenKind::Ident(_)) {
            Some(self.expect_ident()?)
        } else {
            None
        };

        // Если нет `{` — это просто использование типа, а не определение.
        if !self.eat(TokenKind::LeftBrace) {
            return Ok(StructDef { span, name, fields: Vec::new() });
        }

        let fields = self.parse_field_list()?;
        self.expect(TokenKind::RightBrace)?;

        Ok(StructDef { span, name, fields })
    }

    fn parse_union_body(&mut self) -> ParseResult<UnionDef> {
        let span = self.current_span();
        let name = if matches!(self.peek_kind(), TokenKind::Ident(_)) {
            Some(self.expect_ident()?)
        } else {
            None
        };

        if !self.eat(TokenKind::LeftBrace) {
            return Ok(UnionDef { span, name, fields: Vec::new() });
        }

        let fields = self.parse_field_list()?;
        self.expect(TokenKind::RightBrace)?;

        Ok(UnionDef { span, name, fields })
    }

    fn parse_field_list(&mut self) -> ParseResult<Vec<FieldDecl>> {
        let mut fields = Vec::new();

        while !matches!(self.peek_kind(), TokenKind::RightBrace | TokenKind::EndOfCode) {
            let span = self.current_span();
            let ty = self.parse_type()?;
            let name = self.expect_ident()?;

            // Битовое поле: `int x : 3;`
            let bit_width = if self.eat(TokenKind::Colon) {
                Some(Box::new(self.parse_expr()?))
            } else {
                None
            };

            fields.push(FieldDecl { span, ty, name, bit_width });
            self.expect(TokenKind::Semicolon)?;
        }

        Ok(fields)
    }

    fn parse_enum_body(&mut self) -> ParseResult<EnumDef> {
        let span = self.current_span();
        let name = if matches!(self.peek_kind(), TokenKind::Ident(_)) {
            Some(self.expect_ident()?)
        } else {
            None
        };

        self.expect(TokenKind::LeftBrace)?;
        let mut variants = Vec::new();

        while !matches!(self.peek_kind(), TokenKind::RightBrace | TokenKind::EndOfCode) {
            let vspan = self.current_span();
            let vname = self.expect_ident()?;
            let value = if self.eat(TokenKind::Assign) {
                Some(Box::new(self.parse_assign_expr()?))
            } else {
                None
            };
            variants.push(EnumVariant { span: vspan, name: vname, value });
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }

        self.expect(TokenKind::RightBrace)?;
        Ok(EnumDef { span, name, variants })
    }

    // -----------------------------------------------------------------------
    // Блок и операторы
    // -----------------------------------------------------------------------

    fn parse_block(&mut self) -> ParseResult<Block> {
        let start = self.current_span();
        self.expect(TokenKind::LeftBrace)?;
        let mut stmts = Vec::new();

        while !matches!(self.peek_kind(), TokenKind::RightBrace | TokenKind::EndOfCode) {
            stmts.push(self.parse_stmt()?);
        }

        let end = self.current_span();
        self.expect(TokenKind::RightBrace)?;

        Ok(Block { span: self.merge_span(start, end), stmts })
    }

    fn parse_stmt(&mut self) -> ParseResult<Stmt> {
        let span = self.current_span();

        // Препроцессорные токены внутри тела — пропускаем.
        if self.skip_preprocessor() {
            return self.parse_stmt();
        }

        match self.peek_kind() {
            // Блок
            TokenKind::LeftBrace => {
                let block = self.parse_block()?;
                Ok(Stmt { span, kind: StmtKind::Block(block) })
            }

            // Объявление переменной внутри блока (C99+)
            kind if Self::is_decl_start(kind) => {
                let decl = self.parse_decl()?;
                Ok(Stmt { span, kind: StmtKind::Decl(decl) })
            }

            // if
            TokenKind::If => {
                self.advance();
                self.expect(TokenKind::LeftParen)?;
                let cond = self.parse_expr()?;
                self.expect(TokenKind::RightParen)?;
                let then = Box::new(self.parse_stmt()?);
                let alt = if self.eat(TokenKind::Else) {
                    Some(Box::new(self.parse_stmt()?))
                } else {
                    None
                };
                Ok(Stmt { span, kind: StmtKind::If { cond, then, alt } })
            }

            // while
            TokenKind::While => {
                self.advance();
                self.expect(TokenKind::LeftParen)?;
                let cond = self.parse_expr()?;
                self.expect(TokenKind::RightParen)?;
                let body = Box::new(self.parse_stmt()?);
                Ok(Stmt { span, kind: StmtKind::While { cond, body } })
            }

            // do-while
            TokenKind::Do => {
                self.advance();
                let body = Box::new(self.parse_stmt()?);
                self.expect(TokenKind::While)?;
                self.expect(TokenKind::LeftParen)?;
                let cond = self.parse_expr()?;
                self.expect(TokenKind::RightParen)?;
                self.expect(TokenKind::Semicolon)?;
                Ok(Stmt { span, kind: StmtKind::DoWhile { body, cond } })
            }

            // for
            TokenKind::For => {
                self.advance();
                self.expect(TokenKind::LeftParen)?;
                let init = self.parse_for_init()?;
                let cond = if !matches!(self.peek_kind(), TokenKind::Semicolon) {
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                self.expect(TokenKind::Semicolon)?;
                let step = if !matches!(self.peek_kind(), TokenKind::RightParen) {
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                self.expect(TokenKind::RightParen)?;
                let body = Box::new(self.parse_stmt()?);
                Ok(Stmt { span, kind: StmtKind::For { init, cond, step, body } })
            }

            // switch
            TokenKind::Switch => {
                self.advance();
                self.expect(TokenKind::LeftParen)?;
                let expr = self.parse_expr()?;
                self.expect(TokenKind::RightParen)?;
                let body = Box::new(self.parse_stmt()?);
                Ok(Stmt { span, kind: StmtKind::Switch { expr, body } })
            }

            // case
            TokenKind::Case => {
                self.advance();
                let val = self.parse_expr()?;
                self.expect(TokenKind::Colon)?;
                Ok(Stmt { span, kind: StmtKind::Case(val) })
            }

            // default
            TokenKind::Default => {
                self.advance();
                self.expect(TokenKind::Colon)?;
                Ok(Stmt { span, kind: StmtKind::Default })
            }

            // return
            TokenKind::Return => {
                self.advance();
                let val = if !matches!(self.peek_kind(), TokenKind::Semicolon) {
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                self.expect(TokenKind::Semicolon)?;
                Ok(Stmt { span, kind: StmtKind::Return(val) })
            }

            // break
            TokenKind::Break => {
                self.advance();
                self.expect(TokenKind::Semicolon)?;
                Ok(Stmt { span, kind: StmtKind::Break })
            }

            // continue
            TokenKind::Continue => {
                self.advance();
                self.expect(TokenKind::Semicolon)?;
                Ok(Stmt { span, kind: StmtKind::Continue })
            }

            // goto
            TokenKind::Goto => {
                self.advance();
                let label = self.expect_ident()?;
                self.expect(TokenKind::Semicolon)?;
                Ok(Stmt { span, kind: StmtKind::Goto(label) })
            }

            // _Static_assert
            TokenKind::StaticAssert => {
                self.advance();
                self.expect(TokenKind::LeftParen)?;
                let cond = self.parse_expr()?;
                self.expect(TokenKind::Comma)?;
                let msg = self.expect_string_id()?;
                self.expect(TokenKind::RightParen)?;
                self.expect(TokenKind::Semicolon)?;
                Ok(Stmt { span, kind: StmtKind::StaticAssert { cond, msg } })
            }

            // Метка: `label:`
            TokenKind::Ident(_)
                if self.peek_kind_at(1) == &TokenKind::Colon =>
            {
                let label = self.expect_ident()?;
                self.advance(); // `:`
                Ok(Stmt { span, kind: StmtKind::Label(label) })
            }

            // Пустой оператор
            TokenKind::Semicolon => {
                self.advance();
                Ok(Stmt { span, kind: StmtKind::Empty })
            }

            // Выражение-оператор
            _ => {
                let expr = self.parse_expr()?;
                self.expect(TokenKind::Semicolon)?;
                Ok(Stmt { span, kind: StmtKind::Expr(expr) })
            }
        }
    }

    fn parse_for_init(&mut self) -> ParseResult<Option<ForInit>> {
        if matches!(self.peek_kind(), TokenKind::Semicolon) {
            self.advance();
            return Ok(None);
        }

        if Self::is_decl_start(self.peek_kind()) {
            let decl = self.parse_decl()?;
            // parse_decl уже съедает `;`
            return Ok(Some(ForInit::Decl(decl)));
        }

        let expr = self.parse_expr()?;
        self.expect(TokenKind::Semicolon)?;
        Ok(Some(ForInit::Expr(expr)))
    }

    // -----------------------------------------------------------------------
    // Выражения — алгоритм Пратта
    // -----------------------------------------------------------------------

    /// Верхний уровень — запятая как оператор.
    fn parse_expr(&mut self) -> ParseResult<Expr> {
        let span = self.current_span();
        let first = self.parse_assign_expr()?;

        if !matches!(self.peek_kind(), TokenKind::Comma) {
            return Ok(first);
        }

        let mut exprs = vec![first];
        while self.eat(TokenKind::Comma) {
            exprs.push(self.parse_assign_expr()?);
        }

        Ok(Expr { span, kind: ExprKind::Comma(exprs) })
    }

    /// Присваивание и тернарный оператор.
    fn parse_assign_expr(&mut self) -> ParseResult<Expr> {
        let span = self.current_span();
        let lhs = self.parse_pratt(0)?;

        // Тернарный: `cond ? then : alt`
        if self.eat(TokenKind::Question) {
            let then = self.parse_assign_expr()?;
            self.expect(TokenKind::Colon)?;
            let alt = self.parse_assign_expr()?;
            return Ok(Expr {
                span,
                kind: ExprKind::Ternary {
                    cond: Box::new(lhs),
                    then: Box::new(then),
                    alt: Box::new(alt),
                },
            });
        }

        // Присваивание (правоассоциативное).
        if let Some(op) = self.peek_assign_op() {
            self.advance();
            let rhs = self.parse_assign_expr()?;
            return Ok(Expr {
                span,
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
            });
        }

        Ok(lhs)
    }

    /// Ядро алгоритма Пратта: парсит выражение с минимальным приоритетом `min_bp`.
    fn parse_pratt(&mut self, min_bp: u8) -> ParseResult<Expr> {
        let span = self.current_span();
        let mut lhs = self.parse_unary()?;

        loop {
            let Some((op, (lbp, rbp))) = self.peek_binary_op() else { break };
            if lbp < min_bp { break; }

            self.advance();
            let rhs = self.parse_pratt(rbp)?;

            lhs = Expr {
                span,
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
            };
        }

        Ok(lhs)
    }

    /// Унарные операторы и постфиксные выражения.
    fn parse_unary(&mut self) -> ParseResult<Expr> {
        let span = self.current_span();

        match self.peek_kind() {
            // Префиксные
            TokenKind::Minus => {
                self.advance();
                let operand = self.parse_unary()?;
                return Ok(self.unary(span, UnaryOp::Neg, operand));
            }
            TokenKind::Plus => {
                self.advance();
                let operand = self.parse_unary()?;
                return Ok(self.unary(span, UnaryOp::Pos, operand));
            }
            TokenKind::Bang => {
                self.advance();
                let operand = self.parse_unary()?;
                return Ok(self.unary(span, UnaryOp::Not, operand));
            }
            TokenKind::Tilde => {
                self.advance();
                let operand = self.parse_unary()?;
                return Ok(self.unary(span, UnaryOp::BitNot, operand));
            }
            TokenKind::PlusPlus => {
                self.advance();
                let operand = self.parse_unary()?;
                return Ok(self.unary(span, UnaryOp::PreInc, operand));
            }
            TokenKind::MinusMinus => {
                self.advance();
                let operand = self.parse_unary()?;
                return Ok(self.unary(span, UnaryOp::PreDec, operand));
            }

            // sizeof
            TokenKind::Sizeof => {
                self.advance();
                // `sizeof(type)` vs `sizeof expr`
                if matches!(self.peek_kind(), TokenKind::LeftParen)
                    && Self::is_type_token(self.peek_kind_at(1))
                {
                    self.advance(); // `(`
                    let ty = self.parse_type()?;
                    self.expect(TokenKind::RightParen)?;
                    return Ok(Expr { span, kind: ExprKind::Sizeof(SizeofArg::Type(ty)) });
                }
                let expr = self.parse_unary()?;
                return Ok(Expr { span, kind: ExprKind::Sizeof(SizeofArg::Expr(Box::new(expr))) });
            }

            // _Alignof
            TokenKind::Alignof => {
                self.advance();
                self.expect(TokenKind::LeftParen)?;
                let ty = self.parse_type()?;
                self.expect(TokenKind::RightParen)?;
                return Ok(Expr { span, kind: ExprKind::Alignof(ty) });
            }

            // Приведение типа: `(type)expr`
            TokenKind::LeftParen if Self::is_type_token(self.peek_kind_at(1)) => {
                self.advance(); // `(`
                let ty = self.parse_type()?;
                self.expect(TokenKind::RightParen)?;
                let expr = self.parse_unary()?;
                return Ok(Expr {
                    span,
                    kind: ExprKind::Cast { ty, expr: Box::new(expr) },
                });
            }

            _ => {}
        }

        self.parse_postfix()
    }

    /// Постфиксные операторы: вызов, индексирование, поле, `++`, `--`.
    fn parse_postfix(&mut self) -> ParseResult<Expr> {
        let span = self.current_span();
        let mut expr = self.parse_primary()?;

        loop {
            match self.peek_kind() {
                // Вызов функции
                TokenKind::LeftParen => {
                    self.advance();
                    let args = self.parse_call_args()?;
                    self.expect(TokenKind::RightParen)?;
                    expr = Expr {
                        span,
                        kind: ExprKind::Call {
                            callee: Box::new(expr),
                            args,
                        },
                    };
                }

                // Индексирование
                TokenKind::LeftBracket => {
                    self.advance();
                    let index = self.parse_expr()?;
                    self.expect(TokenKind::RightBracket)?;
                    expr = Expr {
                        span,
                        kind: ExprKind::Index {
                            array: Box::new(expr),
                            index: Box::new(index),
                        },
                    };
                }

                // Поле через `.`
                TokenKind::Dot => {
                    self.advance();
                    let field = self.expect_ident()?;
                    expr = Expr {
                        span,
                        kind: ExprKind::Field {
                            object: Box::new(expr),
                            field,
                        },
                    };
                }

                // Постфиксные инкремент/декремент
                TokenKind::PlusPlus => {
                    self.advance();
                    expr = self.unary(span, UnaryOp::PostInc, expr);
                }
                TokenKind::MinusMinus => {
                    self.advance();
                    expr = self.unary(span, UnaryOp::PostDec, expr);
                }

                _ => break,
            }
        }

        Ok(expr)
    }

    fn parse_primary(&mut self) -> ParseResult<Expr> {
        let span = self.current_span();

        match self.peek_kind().clone() {
            // Литералы
            TokenKind::ValueLiteral(v) => {
                self.advance();
                Ok(Expr { span, kind: ExprKind::Literal(v) })
            }

            TokenKind::StringLiteral(lit) => {
                self.advance();
                Ok(Expr { span, kind: ExprKind::StringLiteral(lit.value) })
            }

            TokenKind::CharLiteral(lit) => {
                self.advance();
                // Символьный литерал — числовое значение; храним как StringId для единообразия.
                Ok(Expr { span, kind: ExprKind::StringLiteral(lit.value) })
            }

            // Идентификатор
            TokenKind::Ident(id) => {
                self.advance();
                Ok(Expr { span, kind: ExprKind::Ident(id) })
            }

            // Группировка: `(expr)`
            TokenKind::LeftParen => {
                self.advance();
                let inner = self.parse_expr()?;
                self.expect(TokenKind::RightParen)?;
                Ok(inner)
            }

            // _Generic
            TokenKind::Generic => {
                self.advance();
                self.expect(TokenKind::LeftParen)?;
                let control = Box::new(self.parse_assign_expr()?);
                self.expect(TokenKind::Comma)?;
                let mut associations = Vec::new();
                while !matches!(self.peek_kind(), TokenKind::RightParen | TokenKind::EndOfCode) {
                    let ty = if matches!(self.peek_kind(), TokenKind::Default) {
                        self.advance();
                        None
                    } else {
                        Some(self.parse_type()?)
                    };
                    self.expect(TokenKind::Colon)?;
                    let expr = self.parse_assign_expr()?;
                    associations.push(GenericAssoc { ty, expr });
                    if !self.eat(TokenKind::Comma) { break; }
                }
                self.expect(TokenKind::RightParen)?;
                Ok(Expr { span, kind: ExprKind::Generic { control, associations } })
            }

            other => Err(ParseError::new(
                span,
                ParseErrors::ExpectedExpressionButGot,
                other,
                None
            )),
        }
    }

    fn parse_call_args(&mut self) -> ParseResult<Vec<Expr>> {
        let mut args = Vec::new();
        if matches!(self.peek_kind(), TokenKind::RightParen) {
            return Ok(args);
        }
        loop {
            args.push(self.parse_assign_expr()?);
            if !self.eat(TokenKind::Comma) { break; }
        }
        Ok(args)
    }

    // -----------------------------------------------------------------------
    // Приоритеты операторов (Pratt)
    // -----------------------------------------------------------------------

    /// Возвращает бинарный оператор и его `(left_bp, right_bp)`, если текущий
    /// токен является инфиксным оператором. Присваивания здесь не обрабатываются.
    fn peek_binary_op(&self) -> Option<(BinaryOp, (u8, u8))> {
        let bp = |op: BinaryOp| -> (u8, u8) {
            match op {
                BinaryOp::Or => (1, 2),
                BinaryOp::And => (3, 4),
                BinaryOp::BitOr => (5, 6),
                BinaryOp::BitXor => (7, 8),
                BinaryOp::BitAnd => (9, 10),
                BinaryOp::Eq | BinaryOp::Ne => (11, 12),
                BinaryOp::Lt | BinaryOp::Le
                | BinaryOp::Gt | BinaryOp::Ge => (13, 14),
                BinaryOp::Shl | BinaryOp::Shr => (15, 16),
                BinaryOp::Add | BinaryOp::Sub => (17, 18),
                BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => (19, 20),
                _ => unreachable!(),
            }
        };

        let op = match self.peek_kind() {
            TokenKind::PipePipe => BinaryOp::Or,
            TokenKind::AmpAmp => BinaryOp::And,
            TokenKind::Pipe => BinaryOp::BitOr,
            TokenKind::Caret => BinaryOp::BitXor,
            TokenKind::Amp => BinaryOp::BitAnd,
            TokenKind::Equal => BinaryOp::Eq,
            TokenKind::NotEqual => BinaryOp::Ne,
            TokenKind::LessThan => BinaryOp::Lt,
            TokenKind::LessEqual => BinaryOp::Le,
            TokenKind::GreaterThan => BinaryOp::Gt,
            TokenKind::GreaterEqual=> BinaryOp::Ge,
            TokenKind::LeftShift => BinaryOp::Shl,
            TokenKind::RightShift => BinaryOp::Shr,
            TokenKind::Plus => BinaryOp::Add,
            TokenKind::Minus => BinaryOp::Sub,
            TokenKind::Star => BinaryOp::Mul,
            TokenKind::Slash => BinaryOp::Div,
            TokenKind::Percent => BinaryOp::Rem,
            _ => return None,
        };

        Some((op, bp(op)))
    }

    fn peek_assign_op(&self) -> Option<BinaryOp> {
        match self.peek_kind() {
            TokenKind::Assign => Some(BinaryOp::Assign),
            TokenKind::PlusAssign => Some(BinaryOp::AddAssign),
            TokenKind::MinusAssign => Some(BinaryOp::SubAssign),
            TokenKind::StarAssign => Some(BinaryOp::MulAssign),
            TokenKind::SlashAssign => Some(BinaryOp::DivAssign),
            TokenKind::PercentAssign => Some(BinaryOp::RemAssign),
            TokenKind::AmpAssign => Some(BinaryOp::AndAssign),
            TokenKind::PipeAssign => Some(BinaryOp::OrAssign),
            TokenKind::CaretAssign => Some(BinaryOp::XorAssign),
            TokenKind::LeftShiftAssign => Some(BinaryOp::ShlAssign),
            TokenKind::RightShiftAssign => Some(BinaryOp::ShrAssign),
            _ => None,
        }
    }

    // -----------------------------------------------------------------------
    // Типы
    // -----------------------------------------------------------------------

    fn parse_type(&mut self) -> ParseResult<Type> {
        // Квалификаторы перед типом.
        loop {
            match self.peek_kind() {
                TokenKind::Const | TokenKind::Volatile
                | TokenKind::Restrict | TokenKind::Atomic => { self.advance(); }
                _ => break,
            }
        }

        match self.peek_kind().clone() {
            TokenKind::Typ(ty) => {
                self.advance();
                Ok(ty)
            }

            // `struct Name` или анонимная структура как тип.
            TokenKind::Struct => {
                self.advance();
                // Для простого использования типа `struct Point` в объявлении
                // просто возвращаем заглушку — семантика разрешит имя.
                let _name = if matches!(self.peek_kind(), TokenKind::Ident(_)) {
                    Some(self.expect_ident()?)
                } else {
                    None
                };
                // TODO: вернуть Type::Struct когда семантика будет готова.
                Ok(Type::Int)
            }

            TokenKind::Enum => {
                self.advance();
                let _name = if matches!(self.peek_kind(), TokenKind::Ident(_)) {
                    Some(self.expect_ident()?)
                } else {
                    None
                };
                Ok(Type::Int)
            }

            TokenKind::Union => {
                self.advance();
                let _name = if matches!(self.peek_kind(), TokenKind::Ident(_)) {
                    Some(self.expect_ident()?)
                } else {
                    None
                };
                Ok(Type::Int)
            }

            other => Err(ParseError::new(
                self.current_span(),
                ParseErrors::ExpectedTypeButGot,
                other,
                None
            )),
        }
    }

    // -----------------------------------------------------------------------
    // Вспомогательные методы
    // -----------------------------------------------------------------------

    fn unary(&self, span: Span, op: UnaryOp, operand: Expr) -> Expr {
        Expr { span, kind: ExprKind::Unary { op, operand: Box::new(operand) } }
    }

    fn current_span(&self) -> Span {
        self.tokens
            .get(self.cursor)
            .map(|t| t.position)
            .unwrap_or_else(|| self.tokens.last().map(|t| t.position).unwrap_or(
                Span::new((0, 0), (None, 0))
            ))
    }

    fn merge_span(&self, start: Span, end: Span) -> Span {
        Span {
            start_line_and_col: start.start_line_and_col,
            end_line_and_col: end.end_line_and_col,
        }
    }

    fn peek_kind(&self) -> &TokenKind {
        self.tokens
            .get(self.cursor)
            .map(|t| &t.kind)
            .unwrap_or(&TokenKind::EndOfCode)
    }

    fn peek_kind_at(&self, offset: usize) -> &TokenKind {
        self.tokens
            .get(self.cursor + offset)
            .map(|t| &t.kind)
            .unwrap_or(&TokenKind::EndOfCode)
    }

    fn advance(&mut self) -> &Token {
        let tok = &self.tokens[self.cursor];
        if self.cursor < self.tokens.len() {
            self.cursor += 1;
        }
        tok
    }

    fn at_end(&self) -> bool {
        self.cursor >= self.tokens.len()
            || matches!(self.peek_kind(), TokenKind::EndOfCode)
    }

    /// Потребляет токен если он совпадает с `kind`. Возвращает `true` если совпал.
    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.peek_kind() == &kind {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Требует конкретный токен, иначе — ошибка.
    fn expect(&mut self, kind: TokenKind) -> ParseResult<()> {
        if self.peek_kind() == &kind {
            self.advance();
            Ok(())
        } else {
            Err(ParseError::new(
                self.current_span(),
                ParseErrors::ExpectedButGot,
                self.peek_kind().clone(),
                Some(kind)
            ))
        }
    }

    fn expect_ident(&mut self) -> ParseResult<StringId> {
        match self.peek_kind().clone() {
            TokenKind::Ident(id) => { self.advance(); Ok(id) }
            other => Err(ParseError::new(
                self.current_span(),
                ParseErrors::ExpectedIdentButGot,
                other,
                None
            )),
        }
    }

    fn expect_string_id(&mut self) -> ParseResult<StringId> {
        match self.peek_kind().clone() {
            TokenKind::StringLiteral(lit) => { self.advance(); Ok(lit.value) }
            other => Err(ParseError::new(
                self.current_span(),
                ParseErrors::ExpectedStringLiteralButGot,
                other,
                None
            )),
        }
    }

    /// Пропускает препроцессорные токены. Возвращает `true` если что-то пропустил.
    fn skip_preprocessor(&mut self) -> bool {
        match self.peek_kind() {
            TokenKind::Include | TokenKind::Define
            | TokenKind::Ifdef | TokenKind::Ifndef
            | TokenKind::Endif | TokenKind::Pragma
            | TokenKind::Hash  | TokenKind::HashHash
            | TokenKind::HeaderName(_) => {
                // Пропускаем до конца строки (NewLine уже отфильтрованы,
                // поэтому пропускаем токены до следующего «безопасного»).
                self.advance();
                true
            }
            _ => false,
        }
    }

    /// Проверяет, является ли токен началом объявления.
    fn is_decl_start(kind: &TokenKind) -> bool {
        Self::is_type_token(kind) || Self::is_decl_specifier(kind)
    }

    fn is_decl_specifier(kind: &TokenKind) -> bool {
        matches!(kind,
            TokenKind::Static | TokenKind::Extern | TokenKind::Auto
            | TokenKind::Register | TokenKind::Inline | TokenKind::Noreturn
            | TokenKind::Typedef | TokenKind::Const  | TokenKind::Volatile
            | TokenKind::Restrict | TokenKind::Atomic
        )
    }

    fn is_type_token(kind: &TokenKind) -> bool {
        matches!(kind,
            TokenKind::Typ(_)
            | TokenKind::Struct | TokenKind::Union | TokenKind::Enum
        )
    }
}
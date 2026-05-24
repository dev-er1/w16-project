// example-langs\w16-cc\src\semantic\checker.rs
//
//! Проход 2: проверка типов и потока управления.
//!
//! Получает уже построенную таблицу символов от [`super::resolver::Resolver`]
//! и проверяет:
//! - совместимость типов в операциях, присваиваниях, вызовах,
//! - корректность `return` (тип возвращаемого значения),
//! - `break`/`continue` только внутри циклов/switch,
//! - инициализаторы совместимы с типом переменной.
//!
//! Разрешение имён здесь не выполняется — предполагается что resolver
//! уже завершился без ошибок (или ошибки разрешения имён собраны отдельно).

use crate::frontend::lexer::token::Span;
use crate::frontend::parser::node::*;
use crate::types::Type;
use crate::value::Value;

use super::error::{SemanticError, SemanticErrorKind};
use super::symbol::{Symbol, SymbolTable};

pub struct Checker<'a> {
    /// Таблица символов, построенная resolver-ом.
    symbols: &'a SymbolTable,
    errors:  Vec<SemanticError>,

    /// Тип возврата текущей функции.
    current_return_ty: Option<Type>,

    /// Глубина вложенности циклов (для `break`/`continue`).
    loop_depth: usize,

    /// Глубина вложенности `switch` (для `break`).
    switch_depth: usize,
}

impl<'a> Checker<'a> {
    pub fn new(symbols: &'a SymbolTable) -> Self {
        Self {
            symbols,
            errors:            Vec::new(),
            current_return_ty: None,
            loop_depth:        0,
            switch_depth:      0,
        }
    }

    /// Точка входа. Возвращает список ошибок типизации.
    pub fn check(&mut self, tu: &TranslationUnit) -> Vec<SemanticError> {
        for item in &tu.items {
            match item {
                ExternalDecl::FunctionDef(f) => self.check_function(f),
                ExternalDecl::Decl(d) => self.check_global_decl(d),
            }
        }
        std::mem::take(&mut self.errors)
    }

    // -----------------------------------------------------------------------
    // Глобальные объявления
    // -----------------------------------------------------------------------

    fn check_global_decl(&mut self, decl: &Decl) {
        match &decl.kind {
            DeclKind::Var(v) => {
                if let Some(init) = &v.initializer {
                    self.check_initializer(init, &v.ty, v.span);
                }
            }
            DeclKind::MultiVar(vars) => {
                for v in vars {
                    if let Some(init) = &v.initializer {
                        self.check_initializer(init, &v.ty, v.span);
                    }
                }
            }
            // Typedef, struct, union, enum — типов выражений нет.
            _ => {}
        }
    }

    // -----------------------------------------------------------------------
    // Функции
    // -----------------------------------------------------------------------

    fn check_function(&mut self, f: &FunctionDef) {
        self.current_return_ty = Some(f.return_ty.clone());
        self.check_block(&f.body);
        self.current_return_ty = None;
    }

    fn check_block(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.check_stmt(stmt);
        }
    }

    // -----------------------------------------------------------------------
    // Операторы
    // -----------------------------------------------------------------------

    fn check_stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Empty | StmtKind::Default
            | StmtKind::Label(_) | StmtKind::Goto(_) => {}

            StmtKind::Block(b) => self.check_block(b),

            StmtKind::Decl(d) => self.check_local_decl(d),

            StmtKind::Expr(e) => { self.infer(e); }

            StmtKind::Return(val) => {
                let ret_ty = self.current_return_ty.clone().unwrap_or(Type::Void);
                match (val, &ret_ty) {
                    (None, Type::Void) => {}
                    (None, ty) => {
                        self.error(stmt.span, SemanticErrorKind::MissingReturnValue {
                            expected: ty.clone(),
                        });
                    }
                    (Some(_), Type::Void) => {
                        self.error(stmt.span, SemanticErrorKind::ReturnValueInVoid);
                    }
                    (Some(expr), expected) => {
                        let got = self.infer(expr);
                        if !self.compat(&got, expected) {
                            self.error(expr.span, SemanticErrorKind::TypeMismatch {
                                expected: expected.clone(),
                                got,
                            });
                        }
                    }
                }
            }

            StmtKind::If { cond, then, alt } => {
                self.infer(cond);
                self.check_stmt(then);
                if let Some(alt) = alt { self.check_stmt(alt); }
            }

            StmtKind::While { cond, body } => {
                self.infer(cond);
                self.loop_depth += 1;
                self.check_stmt(body);
                self.loop_depth -= 1;
            }

            StmtKind::DoWhile { body, cond } => {
                self.loop_depth += 1;
                self.check_stmt(body);
                self.loop_depth -= 1;
                self.infer(cond);
            }

            StmtKind::For { init, cond, step, body } => {
                if let Some(init) = init {
                    match init {
                        ForInit::Decl(d) => self.check_local_decl(d),
                        ForInit::Expr(e) => { self.infer(e); }
                    }
                }
                if let Some(cond) = cond { self.infer(cond); }
                if let Some(step) = step { self.infer(step); }
                self.loop_depth += 1;
                self.check_stmt(body);
                self.loop_depth -= 1;
            }

            StmtKind::Switch { expr, body } => {
                let ty = self.infer(expr);
                if !self.is_integer(&ty) {
                    self.error(expr.span, SemanticErrorKind::TypeMismatch {
                        expected: Type::Int,
                        got: ty,
                    });
                }
                self.switch_depth += 1;
                self.check_stmt(body);
                self.switch_depth -= 1;
            }

            StmtKind::Case(e) => { self.infer(e); }

            StmtKind::Break => {
                if self.loop_depth == 0 && self.switch_depth == 0 {
                    self.error(stmt.span, SemanticErrorKind::BreakOutsideLoop);
                }
            }

            StmtKind::Continue => {
                if self.loop_depth == 0 {
                    self.error(stmt.span, SemanticErrorKind::ContinueOutsideLoop);
                }
            }

            StmtKind::StaticAssert { cond, .. } => { self.infer(cond); }
        }
    }

    fn check_local_decl(&mut self, decl: &Decl) {
        match &decl.kind {
            DeclKind::Var(v) => {
                if let Some(init) = &v.initializer {
                    self.check_initializer(init, &v.ty, v.span);
                }
            }
            DeclKind::MultiVar(vars) => {
                for v in vars {
                    if let Some(init) = &v.initializer {
                        self.check_initializer(init, &v.ty, v.span);
                    }
                }
            }
            _ => {}
        }
    }

    // -----------------------------------------------------------------------
    // Вывод типов выражений
    // -----------------------------------------------------------------------

    /// Выводит тип выражения и попутно проверяет корректность операций.
    /// При ошибке добавляет её и возвращает `Type::Int` как заглушку.
    pub fn infer(&mut self, expr: &Expr) -> Type {
        match &expr.kind {
            ExprKind::Literal(v) => self.type_of_value(v),

            ExprKind::StringLiteral(_) => Type::Char,

            ExprKind::Ident(id) => {
                self.symbols.lookup(*id)
                    .map(|s| s.ty().clone())
                    .unwrap_or(Type::Int) // resolver уже поймал undeclared
            }

            ExprKind::Unary { op, operand } => {
                let ty = self.infer(operand);
                self.check_unary(op, &ty, expr.span);
                ty
            }

            ExprKind::Binary { op, lhs, rhs } => {
                let lty = self.infer(lhs);
                let rty = self.infer(rhs);
                self.check_binary(op, &lty, &rty, expr.span)
            }

            ExprKind::Ternary { cond, then, alt } => {
                self.infer(cond);
                let ty_then = self.infer(then);
                let ty_alt = self.infer(alt);
                if !self.compat(&ty_alt, &ty_then) {
                    self.error(expr.span, SemanticErrorKind::TypeMismatch {
                        expected: ty_then.clone(),
                        got:      ty_alt,
                    });
                }
                ty_then
            }

            ExprKind::Call { callee, args } => {
                if let ExprKind::Ident(id) = &callee.kind {
                    self.check_call(*id, args, expr.span)
                } else {
                    for arg in args { self.infer(arg); }
                    Type::Int
                }
            }

            ExprKind::Index { array, index } => {
                let arr_ty = self.infer(array);
                let idx_ty = self.infer(index);
                if !self.is_integer(&idx_ty) {
                    self.error(index.span, SemanticErrorKind::TypeMismatch {
                        expected: Type::Int,
                        got: idx_ty,
                    });
                }
                match arr_ty {
                    Type::Array(elem, _) => *elem,
                    other => {
                        self.error(expr.span, SemanticErrorKind::NotIndexable(other));
                        Type::Int
                    }
                }
            }

            ExprKind::Field { object, .. } => {
                let obj_ty = self.infer(object);
                // Полная проверка полей требует таблицы типов struct/union.
                // Сейчас проверяем только что объект не является скалярным типом.
                if self.is_scalar(&obj_ty) {
                    self.error(expr.span, SemanticErrorKind::NotAStruct(obj_ty));
                }
                Type::Int // заглушка до таблицы типов struct
            }

            ExprKind::Cast { ty, expr: inner } => {
                self.infer(inner);
                ty.clone()
            }

            ExprKind::Sizeof(_) | ExprKind::Alignof(_) => Type::UnsignedLong,

            ExprKind::Comma(exprs) => {
                exprs.iter()
                    .map(|e| self.infer(e))
                    .last()
                    .unwrap_or(Type::Void)
            }

            ExprKind::Generic { control, associations } => {
                self.infer(control);
                associations.first()
                    .map(|a| self.infer(&a.expr))
                    .unwrap_or(Type::Void)
            }
        }
    }

    // -----------------------------------------------------------------------
    // Проверка вызова
    // -----------------------------------------------------------------------

    fn check_call(&mut self, id: crate::frontend::string_pool::StringId, args: &[Expr], span: Span) -> Type {
        let sym = match self.symbols.lookup(id) {
            Some(s) => s.clone(),
            None => { // resolver уже поймал — молча продолжаем
                for arg in args { self.infer(arg); }
                return Type::Int;
            }
        };

        match sym {
            Symbol::Function { params, return_ty } => {
                if args.len() != params.len() {
                    self.error(span, SemanticErrorKind::ArgCountMismatch {
                        expected: params.len(),
                        got:      args.len(),
                    });
                }
                for (i, (arg, param_ty)) in args.iter().zip(params.iter()).enumerate() {
                    let got = self.infer(arg);
                    if !self.compat(&got, param_ty) {
                        self.error(arg.span, SemanticErrorKind::ArgTypeMismatch {
                            param:     param_ty.clone(),
                            got,
                            arg_index: i,
                        });
                    }
                }
                return_ty
            }
            _ => {
                for arg in args { self.infer(arg); }
                Type::Int
            }
        }
    }

    // -----------------------------------------------------------------------
    // Проверка операторов
    // -----------------------------------------------------------------------

    fn check_unary(&mut self, op: &UnaryOp, ty: &Type, span: Span) {
        match op {
            UnaryOp::BitNot => {
                if !self.is_integer(ty) {
                    self.error(span, SemanticErrorKind::InvalidUnaryOp { ty: ty.clone() });
                }
            }
            UnaryOp::Neg | UnaryOp::Pos => {
                if !self.is_arithmetic(ty) {
                    self.error(span, SemanticErrorKind::InvalidUnaryOp { ty: ty.clone() });
                }
            }
            // `!` и инкременты/декременты — любой скалярный тип, без ошибки.
            _ => {}
        }
    }

    fn check_binary(&mut self, op: &BinaryOp, lty: &Type, rty: &Type, span: Span) -> Type {
        match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul
            | BinaryOp::Div | BinaryOp::Rem => {
                if !self.is_arithmetic(lty) || !self.is_arithmetic(rty) {
                    self.error(span, SemanticErrorKind::TypeMismatch {
                        expected: Type::Int,
                        got:      rty.clone(),
                    });
                }
                self.usual_arith_conv(lty, rty)
            }

            BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor
            | BinaryOp::Shl  | BinaryOp::Shr => {
                if !self.is_integer(lty) || !self.is_integer(rty) {
                    self.error(span, SemanticErrorKind::TypeMismatch {
                        expected: Type::Int,
                        got:      rty.clone(),
                    });
                }
                lty.clone()
            }

            BinaryOp::Eq | BinaryOp::Ne
            | BinaryOp::Lt | BinaryOp::Le
            | BinaryOp::Gt | BinaryOp::Ge
            | BinaryOp::And | BinaryOp::Or => Type::Int,

            BinaryOp::Assign => {
                if !self.compat(rty, lty) {
                    self.error(span, SemanticErrorKind::AssignTypeMismatch {
                        lhs: lty.clone(),
                        rhs: rty.clone(),
                    });
                }
                lty.clone()
            }

            // Составное присваивание — возвращаем тип lhs.
            BinaryOp::AddAssign | BinaryOp::SubAssign | BinaryOp::MulAssign
            | BinaryOp::DivAssign | BinaryOp::RemAssign | BinaryOp::AndAssign
            | BinaryOp::OrAssign  | BinaryOp::XorAssign | BinaryOp::ShlAssign
            | BinaryOp::ShrAssign => lty.clone(),
        }
    }

    // -----------------------------------------------------------------------
    // Инициализаторы
    // -----------------------------------------------------------------------

    fn check_initializer(&mut self, init: &Initializer, expected: &Type, span: Span) {
        match init {
            Initializer::Expr(e) => {
                let got = self.infer(e);
                if !self.compat(&got, expected) {
                    self.error(span, SemanticErrorKind::InitTypeMismatch {
                        var_ty:  expected.clone(),
                        init_ty: got,
                    });
                }
            }
            Initializer::List(items) => {
                let elem_ty = match expected {
                    Type::Array(elem, _) => *elem.clone(),
                    other => other.clone(),
                };
                for item in items {
                    self.check_initializer(item, &elem_ty, span);
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Вспомогательные методы
    // -----------------------------------------------------------------------

    fn error(&mut self, span: Span, kind: SemanticErrorKind) {
        self.errors.push(SemanticError::new(span, kind));
    }

    /// Числовые типы совместимы друг с другом (C разрешает неявные преобразования).
    fn compat(&self, a: &Type, b: &Type) -> bool {
        a == b || (self.is_arithmetic(a) && self.is_arithmetic(b))
    }

    fn is_arithmetic(&self, ty: &Type) -> bool {
        self.is_integer(ty) || self.is_float(ty)
    }

    fn is_integer(&self, ty: &Type) -> bool {
        matches!(ty,
            Type::Char | Type::SignedChar | Type::UnsignedChar
            | Type::Short | Type::UnsignedShort
            | Type::Int   | Type::UnsignedInt
            | Type::Long  | Type::UnsignedLong
            | Type::LongLong | Type::UnsignedLongLong
            | Type::Bool
        )
    }

    fn is_float(&self, ty: &Type) -> bool {
        matches!(ty, Type::Float | Type::Double | Type::LongDouble)
    }

    /// Скалярный тип — не подходит для доступа к полю через `.`
    fn is_scalar(&self, ty: &Type) -> bool {
        self.is_arithmetic(ty) || matches!(ty, Type::Void)
    }

    fn usual_arith_conv(&self, a: &Type, b: &Type) -> Type {
        use Type::*;
        if matches!(a, LongDouble) || matches!(b, LongDouble) { return LongDouble; }
        if matches!(a, Double)     || matches!(b, Double)     { return Double; }
        if matches!(a, Float)      || matches!(b, Float)      { return Float; }
        if matches!(a, UnsignedLongLong) || matches!(b, UnsignedLongLong) { return UnsignedLongLong; }
        if matches!(a, LongLong)   || matches!(b, LongLong)   { return LongLong; }
        if matches!(a, UnsignedLong) || matches!(b, UnsignedLong) { return UnsignedLong; }
        if matches!(a, Long)       || matches!(b, Long)       { return Long; }
        Int
    }

    fn type_of_value(&self, v: &Value) -> Type {
        match v {
            Value::Void => Type::Void,
            Value::Bool(_) => Type::Bool,
            Value::Char(_) => Type::Char,
            Value::Int(_) => Type::Int,
            Value::UInt(_) => Type::UnsignedInt,
            Value::Short(_) => Type::Short,
            Value::UShort(_) => Type::UnsignedShort,
            Value::Long(_) => Type::Long,
            Value::ULong(_) => Type::UnsignedLong,
            Value::LongLong(_) => Type::LongLong,
            Value::ULongLong(_) => Type::UnsignedLongLong,
            Value::Float(_) => Type::Float,
            Value::Double(_) => Type::Double,
            Value::LongDouble(_) => Type::LongDouble,
            Value::FloatComplex(..) => Type::FloatComplex,
            Value::DoubleComplex(..) => Type::DoubleComplex,
            Value::LongDoubleComplex(..) => Type::LongDoubleComplex,
            Value::Atomic(inner) => Type::Atomic(Box::new(self.type_of_value(inner))),
            Value::Array(ty, _) => ty.as_ref().clone(),
        }
    }
}
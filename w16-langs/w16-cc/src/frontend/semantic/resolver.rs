// w16-langs\w16-cc\src\frontend\semantic\resolver.rs
//
//! # Проход 1: разрешение имён.
//!
//! Обходит AST и строит таблицу символов: регистрирует функции, глобальные
//! переменные, параметры и локальные переменные. Проверяет:
//! - повторные объявления в одной области видимости,
//! - использование необъявленных идентификаторов,
//! - переходы `goto` на необъявленные метки.
//!
//! Типы операций и корректность выражений — задача [`super::checker`].

use crate::frontend::lexer::token::Span;
use crate::frontend::parser::node::*;
use crate::frontend::string_pool::StringId;
use crate::types::Type;

use super::error::{SemanticError, SemanticErrorKind};
use super::symbol::{Symbol, SymbolTable};

pub struct Resolver {
    pub symbols: SymbolTable,
    errors: Vec<SemanticError>,

    /// Метки объявленные в текущей функции (`label:`).
    declared_labels: Vec<StringId>,

    /// Цели `goto` в текущей функции — проверяем после обхода всей функции.
    goto_targets: Vec<(StringId, Span)>,
}

impl Resolver {
    pub fn new() -> Self {
        Self {
            symbols: SymbolTable::new(),
            errors: Vec::new(),
            declared_labels: Vec::new(),
            goto_targets: Vec::new(),
        }
    }

    /// Точка входа. Возвращает список ошибок разрешения имён.
    pub fn resolve(&mut self, tu: &TranslationUnit) -> Vec<SemanticError> {
        // Глобальная область видимости.
        self.symbols.push_scope();

        // Сначала регистрируем все символы верхнего уровня —
        // чтобы функции могли вызывать друг друга независимо от порядка.
        self.collect_globals(tu);

        // Затем обходим тела функций.
        for item in &tu.items {
            if let ExternalDecl::FunctionDef(f) = item {
                self.resolve_function(f);
            }
        }

        self.symbols.pop_scope();

        std::mem::take(&mut self.errors)
    }

    // -----------------------------------------------------------------------
    // Сбор глобальных символов
    // -----------------------------------------------------------------------

    fn collect_globals(&mut self, tu: &TranslationUnit) {
        for item in &tu.items {
            match item {
                ExternalDecl::FunctionDef(f) => {
                    let params: Vec<Type> = f.params.iter().map(|p| p.ty.clone()).collect();
                    self.declare(f.name, Symbol::Function {
                        params,
                        return_ty: f.return_ty.clone(),
                    }, f.span);
                }
                ExternalDecl::Decl(d) => self.collect_decl(d),
            }
        }
    }

    fn collect_decl(&mut self, decl: &Decl) {
        match &decl.kind {
            DeclKind::Var(v) => {
                self.declare(v.name, Symbol::Var(v.ty.clone()), v.span);
            }
            DeclKind::MultiVar(vars) => {
                for v in vars {
                    self.declare(v.name, Symbol::Var(v.ty.clone()), v.span);
                }
            }
            DeclKind::Typedef { ty, alias } => {
                self.declare(*alias, Symbol::TypeAlias(ty.clone()), decl.span);
            }
            // Struct/union/enum — имена полей видны только внутри тела.
            DeclKind::StructDef(_) | DeclKind::UnionDef(_) | DeclKind::EnumDef(_) => {}
        }
    }

    // -----------------------------------------------------------------------
    // Функции
    // -----------------------------------------------------------------------

    fn resolve_function(&mut self, f: &FunctionDef) {
        self.declared_labels.clear();
        self.goto_targets.clear();

        // Область видимости для параметров и тела.
        self.symbols.push_scope();

        for param in &f.params {
            if let Some(name) = param.name {
                self.declare(name, Symbol::Var(param.ty.clone()), param.span);
            }
        }

        self.resolve_block(&f.body);

        // Проверяем что все goto-цели объявлены в этой функции.
        for (target, span) in self.goto_targets.clone() {
            if !self.declared_labels.contains(&target) {
                self.error(span, SemanticErrorKind::UndeclaredLabel(target));
            }
        }

        self.symbols.pop_scope();
    }

    fn resolve_block(&mut self, block: &Block) {
        self.symbols.push_scope();
        for stmt in &block.stmts {
            self.resolve_stmt(stmt);
        }
        self.symbols.pop_scope();
    }

    // -----------------------------------------------------------------------
    // Операторы
    // -----------------------------------------------------------------------

    fn resolve_stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Empty | StmtKind::Break | StmtKind::Continue
            | StmtKind::Default => {}

            StmtKind::Block(b) => self.resolve_block(b),

            StmtKind::Decl(d) => self.resolve_local_decl(d),

            StmtKind::Expr(e) => self.resolve_expr(e),
            StmtKind::Return(e) => { if let Some(e) = e { self.resolve_expr(e); } }
            StmtKind::Case(e) => self.resolve_expr(e),

            StmtKind::If { cond, then, alt } => {
                self.resolve_expr(cond);
                self.resolve_stmt(then);
                if let Some(alt) = alt { self.resolve_stmt(alt); }
            }

            StmtKind::While { cond, body } => {
                self.resolve_expr(cond);
                self.resolve_stmt(body);
            }

            StmtKind::DoWhile { body, cond } => {
                self.resolve_stmt(body);
                self.resolve_expr(cond);
            }

            StmtKind::For { init, cond, step, body } => {
                // `for` открывает собственную область для `int i = 0`.
                self.symbols.push_scope();
                if let Some(init) = init {
                    match init {
                        ForInit::Decl(d) => self.resolve_local_decl(d),
                        ForInit::Expr(e) => self.resolve_expr(e),
                    }
                }
                if let Some(cond) = cond { self.resolve_expr(cond); }
                if let Some(step) = step { self.resolve_expr(step); }
                self.resolve_stmt(body);
                self.symbols.pop_scope();
            }

            StmtKind::Switch { expr, body } => {
                self.resolve_expr(expr);
                self.resolve_stmt(body);
            }

            StmtKind::Goto(label) => {
                self.goto_targets.push((*label, stmt.span));
            }

            StmtKind::Label(label) => {
                self.declared_labels.push(*label);
            }

            StmtKind::StaticAssert { cond, .. } => self.resolve_expr(cond),
        }
    }

    fn resolve_local_decl(&mut self, decl: &Decl) {
        match &decl.kind {
            DeclKind::Var(v) => {
                // Сначала разрешаем инициализатор — он не видит саму переменную.
                if let Some(init) = &v.initializer {
                    self.resolve_initializer(init);
                }
                self.declare(v.name, Symbol::Var(v.ty.clone()), v.span);
            }
            DeclKind::MultiVar(vars) => {
                for v in vars {
                    if let Some(init) = &v.initializer {
                        self.resolve_initializer(init);
                    }
                    self.declare(v.name, Symbol::Var(v.ty.clone()), v.span);
                }
            }
            DeclKind::Typedef { ty, alias } => {
                self.declare(*alias, Symbol::TypeAlias(ty.clone()), decl.span);
            }
            DeclKind::StructDef(_) | DeclKind::UnionDef(_) | DeclKind::EnumDef(_) => {}
        }
    }

    fn resolve_initializer(&mut self, init: &Initializer) {
        match init {
            Initializer::Expr(e) => self.resolve_expr(e),
            Initializer::List(items) => {
                for item in items { self.resolve_initializer(item); }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Выражения
    // -----------------------------------------------------------------------

    fn resolve_expr(&mut self, expr: &Expr) {
        match &expr.kind {
            // Литералы не содержат имён.
            ExprKind::Literal(_) | ExprKind::StringLiteral(_) => {}

            ExprKind::Ident(id) => {
                if self.symbols.lookup(*id).is_none() {
                    self.error(expr.span, SemanticErrorKind::UndeclaredIdent(*id));
                }
            }

            ExprKind::Unary { operand, .. } => self.resolve_expr(operand),
            ExprKind::Cast { expr: e, .. } => self.resolve_expr(e),
            ExprKind::Sizeof(SizeofArg::Expr(e)) => self.resolve_expr(e),
            ExprKind::Sizeof(SizeofArg::Type(_)) => {}
            ExprKind::Alignof(_) => {}

            ExprKind::Binary { lhs, rhs, .. } => {
                self.resolve_expr(lhs);
                self.resolve_expr(rhs);
            }

            ExprKind::Ternary { cond, then, alt } => {
                self.resolve_expr(cond);
                self.resolve_expr(then);
                self.resolve_expr(alt);
            }

            ExprKind::Call { callee, args } => {
                // Для прямого вызова `foo(...)` проверяем что `foo` объявлена.
                if let ExprKind::Ident(id) = &callee.kind {
                    match self.symbols.lookup(*id) {
                        None => {
                            self.error(callee.span, SemanticErrorKind::UndeclaredFunction(*id));
                        }
                        Some(Symbol::Var(_) | Symbol::TypeAlias(_)) => {
                            self.error(callee.span, SemanticErrorKind::NotCallable(*id));
                        }
                        Some(Symbol::Function { .. }) => {}
                    }
                } else {
                    self.resolve_expr(callee);
                }
                for arg in args { self.resolve_expr(arg); }
            }

            ExprKind::Index { array, index } => {
                self.resolve_expr(array);
                self.resolve_expr(index);
            }

            ExprKind::Field { object, .. } => self.resolve_expr(object),

            ExprKind::Comma(exprs) => {
                for e in exprs { self.resolve_expr(e); }
            }

            ExprKind::Generic { control, associations } => {
                self.resolve_expr(control);
                for assoc in associations { self.resolve_expr(&assoc.expr); }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Хелперы
    // -----------------------------------------------------------------------

    fn declare(&mut self, id: StringId, sym: Symbol, span: Span) {
        if !self.symbols.declare(id, sym) {
            self.error(span, SemanticErrorKind::Redeclaration(id));
        }
    }

    #[inline(always)]
    fn error(&mut self, span: Span, kind: SemanticErrorKind) {
        self.errors.push(SemanticError::new(span, kind));
    }

    #[inline(always)]
    pub fn into_symbols(self) -> SymbolTable {
        self.symbols
    }
}

impl Default for Resolver {
    fn default() -> Self { Self::new() }
}
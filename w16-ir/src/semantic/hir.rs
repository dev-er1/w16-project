// w16-ir\src\semantic\hir.rs
//
//! Semantic verifier для W16-HIR.
//!
//! Этот проход не меняет HIR, а только проверяет его корректность. Он собирает
//! таблицы глобальных констант и функций, затем проверяет каждую функцию с
//! лексическим стеком scopes. Типы выражений выводятся локально через
//! `infer_expr`; если тип вывести нельзя, verifier продолжает работу и копит
//! несколько ошибок за один запуск.

use std::collections::HashMap;

use crate::hir::*;
use crate::semantic::SemanticError;

/// # Проверить полный HIR-модуль.
///
/// Возвращает список ошибок, чтобы CLI мог показать пользователю сразу
/// несколько проблем, а не останавливаться на первой.
pub fn verify_hir_module(module: &Module) -> Result<(), Vec<SemanticError>> {
    let mut verifier = HirVerifier::new(module);
    verifier.verify();
    if verifier.errors.is_empty() {
        Ok(())
    } else {
        Err(verifier.errors)
    }
}

/// Собранная сигнатура функции для проверки вызовов.
#[derive(Clone)]
struct FunctionSig {
    /// Типы параметров в порядке объявления.
    params: Vec<Type>,

    /// Тип результата функции.
    return_ty: ReturnType,
}

/// Stateful verifier для одного модуля.
struct HirVerifier<'a> {
    /// Проверяемый модуль.
    module: &'a Module,

    /// Глобальные константы: имя -> тип.
    constants: HashMap<&'a str, Type>,

    /// Функции: имя -> сигнатура.
    functions: HashMap<&'a str, FunctionSig>,

    /// Накопленные ошибки.
    errors: Vec<SemanticError>,

    /// Глубина циклов.
    loop_depth: usize,
}

impl<'a> HirVerifier<'a> {
    fn new(module: &'a Module) -> Self {
        Self {
            module,
            constants: HashMap::new(),
            functions: HashMap::new(),
            errors: Vec::new(),
            loop_depth: 0
        }
    }

    fn verify(&mut self) {
        // Сначала собираем глобальные имена, чтобы функции могли ссылаться друг
        // на друга независимо от порядка объявления.
        self.collect_constants();
        self.collect_functions();

        for constant in &self.module.constants {
            let value_ty = literal_type(&constant.value);
            if value_ty != constant.ty {
                self.error(format!(
                    "constant `{}` has type {:?}, but literal is {value_ty:?}",
                    constant.name, constant.ty
                ));
            }
        }

        for function in &self.module.functions {
            self.verify_function(function);
        }
    }

    fn collect_constants(&mut self) {
        for constant in &self.module.constants {
            if self.constants.insert(&constant.name, constant.ty).is_some() {
                self.error(format!("duplicate constant `{}`", constant.name));
            }
        }
    }

    fn collect_functions(&mut self) {
        for function in &self.module.functions {
            let params = function.params.iter().map(|param| param.ty).collect();
            let sig = FunctionSig {
                params,
                return_ty: function.return_ty.clone(),
            };
            if self.functions.insert(&function.name, sig).is_some() {
                self.error(format!("duplicate function `@{}`", function.name));
            }
        }
    }

    fn verify_function(&mut self, function: &Function) {
        let mut scopes = ScopeStack::default();
        scopes.push();

        // Параметры функции живут в корневом scope тела функции.
        for param in &function.params {
            if !scopes.declare(param.name.clone(), param.ty) {
                self.error(format!("duplicate parameter `${}`", param.name));
            }
        }

        self.verify_stmts(&function.body, &function.return_ty, &mut scopes);
    }

    fn verify_stmts(&mut self, stmts: &[Stmt], return_ty: &ReturnType, scopes: &mut ScopeStack) {
        for stmt in stmts {
            self.verify_stmt(stmt, return_ty, scopes);
        }
    }

    fn verify_stmt(&mut self, stmt: &Stmt, return_ty: &ReturnType, scopes: &mut ScopeStack) {
        match stmt {
            Stmt::Let { name, ty, value } => {
                let value_ty = self.infer_expr(value, scopes);
                if value_ty != Some(*ty) {
                    self.error(format!(
                        "cannot initialize `${name}` of type {ty:?} with {value_ty:?}"
                    ));
                }
                if !scopes.declare(name.clone(), *ty) {
                    self.error(format!("duplicate local `${name}`"));
                }
            }
            Stmt::Assign { name, value } => {
                let Some(local_ty) = scopes.lookup(name) else {
                    self.error(format!("assignment to undeclared local `${name}`"));
                    return;
                };
                let value_ty = self.infer_expr(value, scopes);
                if value_ty != Some(local_ty) {
                    self.error(format!(
                        "cannot assign {value_ty:?} to `${name}` of type {local_ty:?}"
                    ));
                }
            }
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                self.expect_expr_type(cond, Type::Bool, scopes, "`if` condition");
                // Каждая ветка получает собственный вложенный scope. Пока HIR
                // не поддерживает объявление переменной, видимой после if.
                scopes.push();
                self.verify_stmts(then_body, return_ty, scopes);
                scopes.pop();
                scopes.push();
                self.verify_stmts(else_body, return_ty, scopes);
                scopes.pop();
            }

            Stmt::While { cond, body } => {
                self.expect_expr_type(cond, Type::Bool, scopes, "`while` condition");
                scopes.push();
                self.loop_depth += 1;
                self.verify_stmts(body, return_ty, scopes);
                self.loop_depth -= 1;
                scopes.pop();
            }
            
            Stmt::Break => {
                if self.loop_depth == 0 {
                    self.error("`break` outside of loop");
                }
            }
            Stmt::Continue => {
                if self.loop_depth == 0 {
                    self.error("`continue` outside of loop");
                }
            }

            Stmt::Return(values) => {
                let actual: Vec<Type> = values
                    .iter()
                    .filter_map(|expr| self.infer_expr(expr, scopes))
                    .collect();
                let expected = return_types(return_ty);
                if actual != expected {
                    self.error(format!(
                        "return type mismatch: expected {expected:?}, got {actual:?}"));
                }
            }
            Stmt::Halt => {}
            Stmt::Print(exprs) => {
                for expr in exprs {
                    self.infer_expr(expr, scopes);
                }
            }
            Stmt::Expr(expr) => {
                self.infer_expr(expr, scopes);
            }
        }
    }

    fn infer_expr(&mut self, expr: &Expr, scopes: &ScopeStack) -> Option<Type> {
        match expr {
            Expr::Literal(value) => Some(literal_type(value)),
            Expr::Local(name) => scopes.lookup(name).or_else(|| {
                self.error(format!("use of undeclared local `${name}`"));
                None
            }),
            Expr::Const(name) => self.constants.get(name.as_str()).copied().or_else(|| {
                self.error(format!("unknown constant `{name}`"));
                None
            }),
            Expr::Call { function, args } => {
                let Some(sig) = self.functions.get(function.as_str()).cloned() else {
                    self.error(format!("unknown function `@{function}`"));
                    return None;
                };

                if sig.params.len() != args.len() {
                    self.error(format!(
                        "function `@{function}` expects {} args, got {}",
                        sig.params.len(),
                        args.len()
                    ));
                    return return_type_as_expr(&sig.return_ty);
                }

                for (index, (arg, expected)) in args.iter().zip(sig.params.iter()).enumerate() {
                    let actual = self.infer_expr(arg, scopes);
                    if actual != Some(*expected) {
                        self.error(format!(
                            "argument {index} for `@{function}` must be {expected:?}, got {actual:?}"));
                    }
                }

                return_type_as_expr(&sig.return_ty)
            }
            Expr::Unary { op, expr } => {
                let ty = self.infer_expr(expr, scopes)?;
                match op {
                    UnaryOp::Not if ty == Type::Bool => Some(Type::Bool),
                    UnaryOp::Neg if is_numeric(ty) && ty != Type::Bool => Some(ty),
                    _ => {
                        self.error(format!("invalid unary {op:?} for {ty:?}"));
                        None
                    }
                }
            }
            Expr::Binary { op, lhs, rhs } => {
                let lhs_ty = self.infer_expr(lhs, scopes)?;
                let rhs_ty = self.infer_expr(rhs, scopes)?;
                if lhs_ty != rhs_ty {
                    self.error(format!(
                        "binary {op:?} needs matching types, got {lhs_ty:?} and {rhs_ty:?}"
                    ));
                    return None;
                }

                // Здесь deliberately простая модель: бинарные операции требуют
                // одинаковые типы операндов. Автоматические numeric promotions
                // лучше не добавлять, пока IR и lowering не стабилизировались.
                match op {
                    BinaryOp::Add
                    | BinaryOp::Sub
                    | BinaryOp::Mul
                    | BinaryOp::Div
                    | BinaryOp::Rem
                        if is_numeric(lhs_ty) =>
                    {
                        Some(lhs_ty)
                    }
                    BinaryOp::BitAnd | BinaryOp::Shl | BinaryOp::Shr | BinaryOp::BitOr | BinaryOp::BitXor
                        if is_integer(lhs_ty) || lhs_ty == Type::Bool =>
                    {
                        Some(lhs_ty)
                    }
                    BinaryOp::Eq | BinaryOp::Ne if lhs_ty != Type::Unit => Some(Type::Bool),
                    BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge
                        if is_numeric(lhs_ty) => {
                        Some(Type::Bool)
                    }
                    _ => {
                        self.error(format!("invalid binary {op:?} for {lhs_ty:?}"));
                        None
                    }
                }
            }
            Expr::Select {
                cond,
                then_value,
                else_value,
            } => {
                self.expect_expr_type(cond, Type::Bool, scopes, "select condition");
                let then_ty = self.infer_expr(then_value, scopes)?;
                let else_ty = self.infer_expr(else_value, scopes)?;
                if then_ty == else_ty {
                    Some(then_ty)
                } else {
                    self.error(format!(
                        "select branches must match, got {then_ty:?} and {else_ty:?}"
                    ));
                    None
                }
            }
            Expr::Cast { kind, expr } => {
                let ty = self.infer_expr(expr, scopes)?;
                match (kind, ty) {
                    (CastKind::I2F, Type::I64) => Some(Type::F64),
                    (CastKind::U2F, Type::U64) => Some(Type::F64),
                    (CastKind::F2I, Type::F64) => Some(Type::I64),
                    (CastKind::F2U, Type::F64) => Some(Type::U64),
                    (CastKind::I2U, Type::I64) => Some(Type::U64),
                    (CastKind::U2I, Type::U64) => Some(Type::I64),
                    (CastKind::TruncU64ToU32, Type::U64) => Some(Type::U64), // результат всё равно u64, но младшие 32
                    (CastKind::ZextU32ToU64, Type::U64) => Some(Type::U64),  // уже u64
                    (CastKind::SextI32ToI64, Type::I64) => Some(Type::I64),  // уже i64
                    (CastKind::Bitcast, _) => Some(ty), // биткаст сохраняет тип
                    _ => {
                        self.error(format!("invalid cast {kind:?} from {ty:?}"));
                        None
                    }
                }
            }
            Expr::Load { ty, addr } => {
                let addr_ty = self.infer_expr(addr, scopes);
                if !matches!(addr_ty, Some(Type::Ptr | Type::U64)) {
                    self.error(format!("load address must be ptr or u64, got {addr_ty:?}"));
                }
                Some(*ty)
            }
            Expr::Store { ty, addr, value } => {
                let addr_ty = self.infer_expr(addr, scopes);
                if !matches!(addr_ty, Some(Type::Ptr | Type::U64)) {
                    self.error(format!("store address must be ptr or u64, got {addr_ty:?}"));
                }
                let value_ty = self.infer_expr(value, scopes);
                if value_ty != Some(*ty) {
                    self.error(format!(
                        "store.{ty:?} expects value {ty:?}, got {value_ty:?}"
                    ));
                }
                // Store является выражением только для удобства parser/HIR. В
                // semantic model он возвращает unit и позже станет statement-ish
                // side-effect в MIR.
                Some(Type::Unit)
            }
        }
    }

    fn expect_expr_type(
        &mut self,
        expr: &Expr,
        expected: Type,
        scopes: &ScopeStack,
        context: &str,
    ) {
        let actual = self.infer_expr(expr, scopes);
        if actual != Some(expected) {
            self.error(format!("{context} must be {context:?}, got {actual:?}"));
        }
    }

    fn error(&mut self, message: impl Into<String>) {
        self.errors.push(SemanticError::new(message));
    }
}

/// Стек лексических областей видимости.
#[derive(Default)]
struct ScopeStack {
    /// Каждый HashMap — одна область видимости: имя local -> тип.
    scopes: Vec<HashMap<String, Type>>,
}

impl ScopeStack {
    fn push(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop(&mut self) {
        self.scopes.pop();
    }

    fn declare(&mut self, name: String, ty: Type) -> bool {
        let scope = self.scopes.last_mut().expect("scope stack cannot be empty");
        if scope.contains_key(&name) {
            false
        } else {
            scope.insert(name, ty);
            true
        }
    }

    fn lookup(&self, name: &str) -> Option<Type> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }
}

fn literal_type(value: &Literal) -> Type {
    match value {
        Literal::Int(_) => Type::U64,
        Literal::Float(_) => Type::F64,
        Literal::Bool(_) => Type::Bool,
        Literal::String(_) => Type::Ptr,
    }
}

fn return_types(return_ty: &ReturnType) -> Vec<Type> {
    match return_ty {
        ReturnType::Unit => Vec::new(),
        ReturnType::Single(ty) => vec![*ty],
        ReturnType::Tuple(types) => types.clone(),
    }
}

fn return_type_as_expr(return_ty: &ReturnType) -> Option<Type> {
    match return_ty {
        ReturnType::Single(ty) => Some(*ty),
        ReturnType::Unit | ReturnType::Tuple(_) => None,
    }
}

fn is_integer(ty: Type) -> bool {
    matches!(ty, Type::I64 | Type::U64 | Type::Ptr)
}

fn is_numeric(ty: Type) -> bool {
    matches!(ty, Type::I64 | Type::U64 | Type::F64)
}

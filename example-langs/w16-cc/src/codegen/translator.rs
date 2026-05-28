// example-langs\w16-cc\src\codegen\translator.rs
//
//! Трансляция W16-CC AST -> `hir::Module`.
//!
//! Строит HIR AST напрямую без текстового промежуточного представления.
//! Готовый `hir::Module` можно передать в `w16_lib::W16::run_hir_ast`.

use std::collections::HashMap;

use w16_ir::hir::{
    BinaryOp as HirBinOp, CastKind, Expr as HirExpr, Function as HirFn,
    Literal as HirLit, Module as HirModule, Param as HirParam, ReturnType,
    Stmt as HirStmt, Type as HirType, UnaryOp as HirUnOp,
};

use crate::frontend::parser::node::*;
use crate::frontend::string_pool::StringTable;
use crate::value::Value as CValue;

use super::{map_type, TranslationError, TranslationResult};

// ---------------------------------------------------------------------------
// Контекст функции
// ---------------------------------------------------------------------------

/// Локальная область видимости внутри функции.
/// Отображает StringId -> имя HIR-переменной (`String`).
struct FnCtx {
    /// Стек скоупов: имя C-переменной (через StringPool) -> имя HIR-локала.
    scopes: Vec<HashMap<u32, String>>,
    /// Счётчик для генерации уникальных имён (`_t0`, `_t1`, ...).
    temp_counter: usize,
    /// Имя текущей функции (для сообщений об ошибках).
    _fn_name: String,
}

impl FnCtx {
    fn new(_fn_name: String) -> Self {
        Self { scopes: vec![HashMap::new()], temp_counter: 0, _fn_name }
    }

    fn push(&mut self) { self.scopes.push(HashMap::new()); }

    fn pop(&mut self) { self.scopes.pop(); }

    /// Объявить переменную и вернуть её HIR-имя.
    fn declare(&mut self, id: u32, hint: &str) -> String {
        let name = format!("{}_{}", hint, self.temp_counter);
        self.temp_counter += 1;
        self.scopes.last_mut().unwrap().insert(id, name.clone());
        name
    }

    /// Найти HIR-имя переменной по StringId.
    fn lookup(&self, id: u32) -> Option<&str> {
        self.scopes.iter().rev()
            .find_map(|s| s.get(&id))
            .map(String::as_str)
    }

    fn fresh(&mut self, hint: &str) -> String {
        let name = format!("{}_{}", hint, self.temp_counter);
        self.temp_counter += 1;
        name
    }
}

// ---------------------------------------------------------------------------
// Транслятор
// ---------------------------------------------------------------------------

pub struct AstTranslator<'a> {
    strings: &'a StringTable,
    errors: Vec<TranslationError>,
}

impl<'a> AstTranslator<'a> {
    pub fn new(strings: &'a StringTable) -> Self {
        Self { strings, errors: Vec::new() }
    }

    /// Транслировать единицу трансляции в HIR-модуль.
    ///
    /// Имя модуля берётся из первой функции; если функций нет — `"module"`.
    pub fn translate(&mut self, tu: &TranslationUnit, module_name: &str) -> TranslationResult<HirModule> {
        let mut functions = Vec::new();

        for item in &tu.items {
            match item {
                ExternalDecl::FunctionDef(f) => {
                    match self.translate_function(f) {
                        Ok(hf) => functions.push(hf),
                        Err(e) => self.errors.push(e),
                    }
                }
                // Глобальные переменные в HIR нет — пропускаем.
                // Константы можно добавить позже через hir::ConstDecl.
                ExternalDecl::Decl(_) => {}
            }
        }

        if !self.errors.is_empty() {
            return Err(self.errors[0].clone());
        }

        Ok(HirModule {
            name: module_name.to_owned(),
            constants: Vec::new(),
            functions,
        })
    }

    /// Вернуть все накопленные ошибки.
    pub fn errors(&self) -> &[TranslationError] {
        &self.errors
    }

    // -----------------------------------------------------------------------
    // Функция
    // -----------------------------------------------------------------------

    fn translate_function(&mut self, f: &FunctionDef) -> TranslationResult<HirFn> {
        let name = self.resolve(f.name);
        let mut ctx = FnCtx::new(name.clone());

        // Параметры
        let mut params = Vec::new();
        for p in &f.params {
            if let Some(pid) = p.name {
                let hir_name = self.resolve(pid);
                ctx.scopes[0].insert(pid.0, hir_name.clone());
                params.push(HirParam { name: hir_name, ty: map_type(&p.ty) });
            }
        }

        // Возвращаемый тип
        let return_ty = match map_type(&f.return_ty) {
            HirType::Unit => ReturnType::Unit,
            ty => ReturnType::Single(ty),
        };

        // Тело
        let mut body = Vec::new();
        self.translate_block(&f.body, &mut ctx, &mut body)?;

        Ok(HirFn { name, params, return_ty, body })
    }

    // -----------------------------------------------------------------------
    // Блок и операторы
    // -----------------------------------------------------------------------

    fn translate_block(
        &mut self,
        block: &Block,
        ctx: &mut FnCtx,
        out: &mut Vec<HirStmt>,
    ) -> TranslationResult<()> {
        ctx.push();
        for stmt in &block.stmts {
            self.translate_stmt(stmt, ctx, out)?;
        }
        ctx.pop();
        Ok(())
    }

    fn translate_stmt(
        &mut self,
        stmt: &Stmt,
        ctx: &mut FnCtx,
        out: &mut Vec<HirStmt>,
    ) -> TranslationResult<()> {
        match &stmt.kind {
            StmtKind::Empty => {}

            StmtKind::Block(b) => self.translate_block(b, ctx, out)?,

            StmtKind::Decl(d) => self.translate_local_decl(d, ctx, out)?,

            StmtKind::Expr(e) => {
                let hir_e = self.translate_expr(e, ctx, out)?;
                out.push(HirStmt::Expr(hir_e));
            }

            StmtKind::Return(None) => {
                out.push(HirStmt::Return(vec![]));
            }

            StmtKind::Return(Some(e)) => {
                let val = self.translate_expr(e, ctx, out)?;
                out.push(HirStmt::Return(vec![val]));
            }

            StmtKind::If { cond, then, alt } => {
                let cond_expr = self.translate_expr_as_bool(cond, ctx, out)?;
                let mut then_body = Vec::new();
                self.translate_stmt(then, ctx, &mut then_body)?;
                let mut else_body = Vec::new();
                if let Some(alt) = alt {
                    self.translate_stmt(alt, ctx, &mut else_body)?;
                }
                out.push(HirStmt::If { cond: cond_expr, then_body, else_body });
            }

            StmtKind::While { cond, body } => {
                let cond_expr = self.translate_expr_as_bool(cond, ctx, out)?;
                let mut loop_body = Vec::new();
                self.translate_stmt(body, ctx, &mut loop_body)?;
                out.push(HirStmt::While { cond: cond_expr, body: loop_body });
            }

            StmtKind::DoWhile { body, cond } => {
                let mut loop_body = Vec::new();
                self.translate_stmt(body, ctx, &mut loop_body)?;
                let cond_expr = self.translate_expr_as_bool(cond, ctx, out)?;
                out.push(HirStmt::DoWhile { body: loop_body, cond: cond_expr });
            }

            // for (init; cond; step) body
            // Раскрываем в: init; while (cond) { body; step; }
            StmtKind::For { init, cond, step, body } => {
                ctx.push();

                // init
                if let Some(init) = init {
                    match init {
                        ForInit::Decl(d) => self.translate_local_decl(d, ctx, out)?,
                        ForInit::Expr(e) => {
                            let he = self.translate_expr(e, ctx, out)?;
                            out.push(HirStmt::Expr(he));
                        }
                    }
                }

                // cond: если нет -> `while true`
                let hir_cond = if let Some(cond) = cond {
                    self.translate_expr_as_bool(cond, ctx, out)?
                } else {
                    HirExpr::Literal(HirLit::Bool(true))
                };

                let mut loop_body = Vec::new();
                self.translate_stmt(body, ctx, &mut loop_body)?;

                // step — в конце тела
                if let Some(step) = step {
                    let hs = self.translate_expr(step, ctx, &mut loop_body)?;
                    loop_body.push(HirStmt::Expr(hs));
                }

                out.push(HirStmt::While { cond: hir_cond, body: loop_body });
                ctx.pop();
            }

            // switch раскрывается в if/else if/else
            StmtKind::Switch { expr, body } => {
                self.translate_switch(expr, body, ctx, out)?;
            }

            StmtKind::Break => out.push(HirStmt::Break),
            StmtKind::Continue => out.push(HirStmt::Continue),

            // goto/метки — HIR не поддерживает unstructured jumps
            StmtKind::Goto(id) => {
                return Err(TranslationError::new(
                    stmt.span,
                    format!("goto is not supported in W16-HIR (label id={})", id.0),
                ));
            }
            StmtKind::Label(_) => {
                // Метки без goto — просто игнорируем
            }

            StmtKind::Case(_) | StmtKind::Default => {
                // Обрабатываются в translate_switch, не должны встречаться здесь
            }

            StmtKind::StaticAssert { .. } => {
                // Compile-time assert — ничего не генерируем
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Switch -> if/else if/else
    // -----------------------------------------------------------------------

    fn translate_switch(
        &mut self,
        expr: &Expr,
        body: &Stmt,
        ctx: &mut FnCtx,
        out: &mut Vec<HirStmt>,
    ) -> TranslationResult<()> {
        // Вычисляем выражение switch в temp-переменную
        let switch_val = self.translate_expr(expr, ctx, out)?;
        let tmp = ctx.fresh("sw");
        out.push(HirStmt::Let {
            name: tmp.clone(),
            ty: HirType::I64,
            value: switch_val,
        });

        // Собираем кейсы из тела switch
        let stmts = match &body.kind {
            StmtKind::Block(b) => &b.stmts,
            _ => return Err(TranslationError::new(body.span, "switch body must be a block")),
        };

        // Группируем: Vec<(условия, тело)> + опциональный default
        struct Case {
            values: Vec<HirExpr>,   // пустой = default
            stmts: Vec<HirStmt>,
        }

        let mut cases: Vec<Case> = Vec::new();
        let mut current_values: Vec<HirExpr> = Vec::new();
        let mut current_stmts: Vec<HirStmt> = Vec::new();
        let mut in_case = false;

        for s in stmts {
            match &s.kind {
                StmtKind::Case(val_expr) => {
                    if in_case && !current_stmts.is_empty() {
                        cases.push(Case { values: current_values.clone(), stmts: current_stmts.drain(..).collect() });
                        current_values.clear();
                    }
                    let hv = self.translate_expr(val_expr, ctx, out)?;
                    current_values.push(hv);
                    in_case = true;
                }
                StmtKind::Default => {
                    if in_case && !current_stmts.is_empty() {
                        cases.push(Case { values: current_values.clone(), stmts: current_stmts.drain(..).collect() });
                        current_values.clear();
                    }
                    // default — пустой список значений
                    in_case = true;
                }
                StmtKind::Break => {
                    if in_case {
                        cases.push(Case { values: current_values.clone(), stmts: current_stmts.drain(..).collect() });
                        current_values.clear();
                        in_case = false;
                    }
                }
                _ => {
                    if in_case {
                        let mut tmp_stmts = Vec::new();
                        self.translate_stmt(s, ctx, &mut tmp_stmts)?;
                        current_stmts.extend(tmp_stmts);
                    }
                }
            }
        }
        if in_case && (!current_values.is_empty() || !current_stmts.is_empty()) {
            cases.push(Case { values: current_values, stmts: current_stmts });
        }

        // Строим if/else if/else
        // Идём с конца чтобы вложить if-else рекурсивно
        let mut result: Option<Vec<HirStmt>> = None;

        for case in cases.into_iter().rev() {
            if case.values.is_empty() {
                // default
                result = Some(case.stmts);
            } else {
                // case v1, v2, ...: cond = (sw == v1 || sw == v2 || ...)
                let mut cond = HirExpr::Binary {
                    op: HirBinOp::Eq,
                    lhs: Box::new(HirExpr::Local(tmp.clone())),
                    rhs: Box::new(case.values[0].clone()),
                };
                for extra in case.values.into_iter().skip(1) {
                    cond = HirExpr::Binary {
                        op: HirBinOp::BitOr, // логическое ИЛИ через BitOr (семантика Bool)
                        lhs: Box::new(cond),
                        rhs: Box::new(HirExpr::Binary {
                            op: HirBinOp::Eq,
                            lhs: Box::new(HirExpr::Local(tmp.clone())),
                            rhs: Box::new(extra),
                        }),
                    };
                }

                let else_body = result.unwrap_or_default();
                result = Some(vec![HirStmt::If {
                    cond,
                    then_body: case.stmts,
                    else_body,
                }]);
            }
        }

        if let Some(stmts) = result {
            out.extend(stmts);
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Локальные объявления
    // -----------------------------------------------------------------------

    fn translate_local_decl(
        &mut self,
        decl: &Decl,
        ctx: &mut FnCtx,
        out: &mut Vec<HirStmt>,
    ) -> TranslationResult<()> {
        match &decl.kind {
            DeclKind::Var(v) => self.translate_var_decl(v, ctx, out)?,
            DeclKind::MultiVar(vars) => {
                for v in vars { self.translate_var_decl(v, ctx, out)?; }
            }
            // typedef/struct/union/enum — пропускаем
            _ => {}
        }
        Ok(())
    }

    fn translate_var_decl(
        &mut self,
        v: &VarDecl,
        ctx: &mut FnCtx,
        out: &mut Vec<HirStmt>,
    ) -> TranslationResult<()> {
        let hir_ty = map_type(&v.ty);
        let hint = self.resolve(v.name);
        let hname = ctx.declare(v.name.0, &hint);

        let value = if let Some(init) = &v.initializer {
            self.translate_initializer(init, hir_ty, ctx, out)?
        } else {
            // Нет инициализатора -> 0
            default_value(hir_ty)
        };

        out.push(HirStmt::Let { name: hname, ty: hir_ty, value });
        Ok(())
    }

    fn translate_initializer(
        &mut self,
        init: &Initializer,
        _expected_ty: HirType,
        ctx: &mut FnCtx,
        out: &mut Vec<HirStmt>,
    ) -> TranslationResult<HirExpr> {
        match init {
            Initializer::Expr(e) => self.translate_expr(e, ctx, out),
            Initializer::List(items) => {
                // Агрегатный инициализатор без struct — берём первый элемент
                if let Some(first) = items.first() {
                    self.translate_initializer(first, _expected_ty, ctx, out)
                } else {
                    Ok(default_value(_expected_ty))
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Выражения
    // -----------------------------------------------------------------------

    fn translate_expr(
        &mut self,
        expr: &Expr,
        ctx: &mut FnCtx,
        out: &mut Vec<HirStmt>,
    ) -> TranslationResult<HirExpr> {
        match &expr.kind {
            ExprKind::Literal(v) => Ok(translate_literal(v)),

            ExprKind::StringLiteral(id) => {
                let s = self.strings.resolve(*id).unwrap_or("").to_owned();
                Ok(HirExpr::Literal(HirLit::String(s)))
            }

            ExprKind::Ident(id) => {
                let name = ctx.lookup(id.0)
                    .map(str::to_owned)
                    .unwrap_or_else(|| self.resolve(*id));
                Ok(HirExpr::Local(name))
            }

            ExprKind::Unary { op, operand } => {
                let val = self.translate_expr(operand, ctx, out)?;
                let hir_op = match op {
                    UnaryOp::Neg | UnaryOp::Pos => HirUnOp::Neg,
                    UnaryOp::Not | UnaryOp::BitNot => HirUnOp::Not,
                    // Пре/пост инкремент/декремент раскрываем в assign
                    UnaryOp::PreInc => return self.translate_incdec(operand, true,  true,  ctx, out),
                    UnaryOp::PreDec => return self.translate_incdec(operand, false, true,  ctx, out),
                    UnaryOp::PostInc => return self.translate_incdec(operand, true,  false, ctx, out),
                    UnaryOp::PostDec => return self.translate_incdec(operand, false, false, ctx, out),
                };
                Ok(HirExpr::Unary { op: hir_op, expr: Box::new(val) })
            }

            ExprKind::Binary { op, lhs, rhs } => {
                self.translate_binary(op, lhs, rhs, expr, ctx, out)
            }

            ExprKind::Ternary { cond, then, alt } => {
                let c = self.translate_expr_as_bool(cond, ctx, out)?;
                let t = self.translate_expr(then, ctx, out)?;
                let e = self.translate_expr(alt, ctx, out)?;
                Ok(HirExpr::Select {
                    cond: Box::new(c),
                    then_value: Box::new(t),
                    else_value: Box::new(e),
                })
            }

            ExprKind::Call { callee, args } => {
                // Поддерживаем только прямой вызов по имени
                let fname = match &callee.kind {
                    ExprKind::Ident(id) => self.resolve(*id),
                    _ => return Err(TranslationError::new(
                        expr.span,
                        "indirect function calls are not supported",
                    )),
                };
                let mut hir_args = Vec::new();
                for a in args {
                    hir_args.push(self.translate_expr(a, ctx, out)?);
                }
                Ok(HirExpr::Call { function: fname, args: hir_args })
            }

            ExprKind::Index { array, index } => {
                // arr[i] -> load.u64(arr + i * 8)  — упрощённо
                let arr = self.translate_expr(array, ctx, out)?;
                let idx = self.translate_expr(index, ctx, out)?;
                let scaled = HirExpr::Binary {
                    op: HirBinOp::Mul,
                    lhs: Box::new(idx),
                    rhs: Box::new(HirExpr::Literal(HirLit::Int(8))),
                };
                let addr = HirExpr::Binary {
                    op: HirBinOp::Add,
                    lhs: Box::new(arr),
                    rhs: Box::new(scaled),
                };
                Ok(HirExpr::Load { ty: HirType::U64, addr: Box::new(addr) })
            }

            ExprKind::Field { .. } => Err(TranslationError::new(
                expr.span,
                "struct field access is not supported without pointer types",
            )),

            ExprKind::Cast { ty, expr: inner } => {
                let val = self.translate_expr(inner, ctx, out)?;
                let src_ty = HirType::I64; // приближение
                let dst_ty = map_type(ty);
                let cast_kind = cast_kind(src_ty, dst_ty);
                if let Some(kind) = cast_kind {
                    Ok(HirExpr::Cast { kind, expr: Box::new(val) })
                } else {
                    Ok(val) // no-op если типы совпадают
                }
            }

            ExprKind::Sizeof(_) | ExprKind::Alignof(_) => {
                // Возвращаем 8 как заглушку (64-битная платформа)
                Ok(HirExpr::Literal(HirLit::Int(8)))
            }

            ExprKind::Comma(exprs) => {
                // Вычисляем все кроме последнего как side-effect
                let mut last = HirExpr::Literal(HirLit::Int(0));
                for e in exprs {
                    let he = self.translate_expr(e, ctx, out)?;
                    last = he;
                }
                Ok(last)
            }

            ExprKind::Generic { associations, .. } => {
                // Берём первую ветку как приближение
                if let Some(assoc) = associations.first() {
                    self.translate_expr(&assoc.expr, ctx, out)
                } else {
                    Ok(HirExpr::Literal(HirLit::Int(0)))
                }
            }
        }
    }

    /// Транслирует выражение и приводит его к `Bool` если нужно.
    fn translate_expr_as_bool(
        &mut self,
        expr: &Expr,
        ctx: &mut FnCtx,
        out: &mut Vec<HirStmt>,
    ) -> TranslationResult<HirExpr> {
        let val = self.translate_expr(expr, ctx, out)?;

        // Если выражение уже возвращает Bool (сравнение, логика, bool-литерал)
        // — возвращаем напрямую без обёртки `!= 0`.
        let is_bool = matches!(&val,
            HirExpr::Binary { op, .. } if matches!(op,
                HirBinOp::Eq | HirBinOp::Ne | HirBinOp::Lt |
                HirBinOp::Le | HirBinOp::Gt | HirBinOp::Ge
            )
        ) || matches!(&val, HirExpr::Literal(HirLit::Bool(_)));

        if is_bool {
            return Ok(val);
        }

        // Для знаковых локалей и литералов используем SignedInt(0) (-> I64),
        // для беззнаковых — Int(0) (-> U64), чтобы типы совпадали в Ne.
        let zero = match &val {
            HirExpr::Literal(HirLit::Int(_)) => HirExpr::Literal(HirLit::Int(0)),
            HirExpr::Literal(HirLit::SignedInt(_)) => HirExpr::Literal(HirLit::SignedInt(0)),
            // Local — в большинстве случаев I64 (C int знаковый по умолчанию).
            _ => HirExpr::Literal(HirLit::SignedInt(0)),
        };

        Ok(HirExpr::Binary {
            op: HirBinOp::Ne,
            lhs: Box::new(val),
            rhs: Box::new(zero),
        })
    }

    // -----------------------------------------------------------------------
    // Бинарные операции
    // -----------------------------------------------------------------------

    fn translate_binary(
        &mut self,
        op: &BinaryOp,
        lhs: &Expr,
        rhs: &Expr,
        _span_expr: &Expr,
        ctx: &mut FnCtx,
        out: &mut Vec<HirStmt>,
    ) -> TranslationResult<HirExpr> {
        // Присваивания — отдельная ветка
        if op.is_assign() {
            return self.translate_assign(op, lhs, rhs, ctx, out);
        }

        let l = self.translate_expr(lhs, ctx, out)?;
        let r = self.translate_expr(rhs, ctx, out)?;

        let hir_op = match op {
            BinaryOp::Add => HirBinOp::Add,
            BinaryOp::Sub => HirBinOp::Sub,
            BinaryOp::Mul => HirBinOp::Mul,
            BinaryOp::Div => HirBinOp::Div,
            BinaryOp::Rem => HirBinOp::Rem,
            BinaryOp::Eq => HirBinOp::Eq,
            BinaryOp::Ne => HirBinOp::Ne,
            BinaryOp::Lt => HirBinOp::Lt,
            BinaryOp::Le => HirBinOp::Le,
            BinaryOp::Gt => HirBinOp::Gt,
            BinaryOp::Ge => HirBinOp::Ge,
            BinaryOp::And => HirBinOp::BitAnd,
            BinaryOp::Or => HirBinOp::BitOr,
            BinaryOp::BitAnd => HirBinOp::BitAnd,
            BinaryOp::BitOr => HirBinOp::BitOr,
            BinaryOp::BitXor => HirBinOp::BitXor,
            BinaryOp::Shl => HirBinOp::Shl,
            BinaryOp::Shr => HirBinOp::Shr,
            _ => unreachable!("assign handled above"),
        };

        Ok(HirExpr::Binary { op: hir_op, lhs: Box::new(l), rhs: Box::new(r) })
    }

    // -----------------------------------------------------------------------
    // Присваивания
    // -----------------------------------------------------------------------

    fn translate_assign(
        &mut self,
        op: &BinaryOp,
        lhs: &Expr,
        rhs: &Expr,
        ctx: &mut FnCtx,
        out: &mut Vec<HirStmt>,
    ) -> TranslationResult<HirExpr> {
        let rhs_val = self.translate_expr(rhs, ctx, out)?;

        // Имя lhs-переменной (только простые идентификаторы)
        let var_name = match &lhs.kind {
            ExprKind::Ident(id) => {
                ctx.lookup(id.0).map(str::to_owned)
                    .unwrap_or_else(|| self.resolve(*id))
            }
            _ => return Err(TranslationError::new(
                lhs.span,
                "complex lvalue assignment is not supported",
            )),
        };

        // Составное присваивание: x += rhs -> x = x + rhs
        let final_val = if *op == BinaryOp::Assign {
            rhs_val.clone()
        } else {
            let hir_op = compound_assign_op(op);
            HirExpr::Binary {
                op: hir_op,
                lhs: Box::new(HirExpr::Local(var_name.clone())),
                rhs: Box::new(rhs_val.clone()),
            }
        };

        out.push(HirStmt::Assign { name: var_name.clone(), value: final_val.clone() });
        Ok(HirExpr::Local(var_name))
    }

    // -----------------------------------------------------------------------
    // Инкремент/декремент
    // -----------------------------------------------------------------------

    fn translate_incdec(
        &mut self,
        operand: &Expr,
        is_inc: bool,
        is_pre: bool,
        ctx: &mut FnCtx,
        out: &mut Vec<HirStmt>,
    ) -> TranslationResult<HirExpr> {
        let var_name = match &operand.kind {
            ExprKind::Ident(id) => ctx.lookup(id.0).map(str::to_owned)
                .unwrap_or_else(|| self.resolve(*id)),
            _ => return Err(TranslationError::new(
                operand.span,
                "increment/decrement on complex expression is not supported",
            )),
        };

        let old = HirExpr::Local(var_name.clone());
        // SignedInt(1) чтобы тип совпадал с I64-переменной при Add/Sub.
        let one = HirExpr::Literal(HirLit::SignedInt(1));
        let new_val = HirExpr::Binary {
            op: if is_inc { HirBinOp::Add } else { HirBinOp::Sub },
            lhs: Box::new(old.clone()),
            rhs: Box::new(one),
        };

        if is_pre {
            out.push(HirStmt::Assign { name: var_name.clone(), value: new_val });
            Ok(HirExpr::Local(var_name))
        } else {
            // Постфикс: вернуть старое значение
            let tmp = ctx.fresh("old");
            out.push(HirStmt::Let { name: tmp.clone(), ty: HirType::I64, value: old });
            out.push(HirStmt::Assign { name: var_name, value: new_val });
            Ok(HirExpr::Local(tmp))
        }
    }

    // -----------------------------------------------------------------------
    // Вспомогательные
    // -----------------------------------------------------------------------

    /// Резолвит StringId в строку через StringTable.
    fn resolve(&self, id: crate::frontend::string_pool::StringId) -> String {
        self.strings.resolve(id).unwrap_or("_unknown").to_owned()
    }
}

// ---------------------------------------------------------------------------
// Свободные функции-хелперы
// ---------------------------------------------------------------------------

fn translate_literal(v: &CValue) -> HirExpr {
    match v {
        CValue::Bool(b) => HirExpr::Literal(HirLit::Bool(*b)),

        // Беззнаковые — U64, HIR видит правильный тип.
        CValue::UInt(u) => HirExpr::Literal(HirLit::Int(*u as u64)),
        CValue::UShort(u) => HirExpr::Literal(HirLit::Int(*u as u64)),
        CValue::ULong(u) => HirExpr::Literal(HirLit::Int(*u)),
        CValue::ULongLong(u) => HirExpr::Literal(HirLit::Int(*u)),
        CValue::Char(c) => HirExpr::Literal(HirLit::Int(*c as u64)),

        // Знаковые — оборачиваем в cast.u2i чтобы HIR видел I64, а не U64.
        // Биты те же, тип разный.
        CValue::Int(i) => signed_lit(*i as i64),
        CValue::Short(s) => signed_lit(*s as i64),
        CValue::Long(l) => signed_lit(*l),
        CValue::LongLong(l) => signed_lit(*l),

        CValue::Float(f) => HirExpr::Literal(HirLit::Float(*f as f64)),
        CValue::Double(d) => HirExpr::Literal(HirLit::Float(*d)),
        _ => HirExpr::Literal(HirLit::Int(0)),
    }
}

/// Создаёт знаковый литерал как `cast.u2i(<bits>)`.
/// HIR хранит Int как u64 битово, но cast меняет семантический тип на I64.
fn signed_lit(value: i64) -> HirExpr {
    HirExpr::Cast {
        kind: CastKind::U2I,
        expr: Box::new(HirExpr::Literal(HirLit::Int(value as u64))),
    }
}

fn default_value(ty: HirType) -> HirExpr {
    match ty {
        HirType::F64 => HirExpr::Literal(HirLit::Float(0.0)),
        HirType::Bool => HirExpr::Literal(HirLit::Bool(false)),
        _ => HirExpr::Literal(HirLit::Int(0)),
    }
}

fn cast_kind(src: HirType, dst: HirType) -> Option<CastKind> {
    use HirType::*;
    match (src, dst) {
        (I64, F64) => Some(CastKind::I2F),
        (U64, F64) => Some(CastKind::U2F),
        (F64, I64) => Some(CastKind::F2I),
        (F64, U64) => Some(CastKind::F2U),
        (I64, U64) => Some(CastKind::I2U),
        (U64, I64) => Some(CastKind::U2I),
        _ => None,
    }
}

fn compound_assign_op(op: &BinaryOp) -> HirBinOp {
    match op {
        BinaryOp::AddAssign => HirBinOp::Add,
        BinaryOp::SubAssign => HirBinOp::Sub,
        BinaryOp::MulAssign => HirBinOp::Mul,
        BinaryOp::DivAssign => HirBinOp::Div,
        BinaryOp::RemAssign => HirBinOp::Rem,
        BinaryOp::AndAssign => HirBinOp::BitAnd,
        BinaryOp::OrAssign => HirBinOp::BitOr,
        BinaryOp::XorAssign => HirBinOp::BitXor,
        BinaryOp::ShlAssign => HirBinOp::Shl,
        BinaryOp::ShrAssign => HirBinOp::Shr,
        _ => unreachable!(),
    }
}
// example-langs\w16-cc\src\codegen\emitter.rs
//
//! # Генерация текстового W16-HIR из `hir::Module`.
//!
//! Принимает уже построенный `hir::Module` от [`super::translator::AstTranslator`]
//! и выводит его в текстовый формат `.w16h` совместимый с `w16-ir` парсером.
//!
//! Использование:
//! ```rust
//! let hir_module = translator.translate(&tu, "my_module")?;
//! let text = TextEmitter::emit(&hir_module);
//! // text можно передать в w16_lib::run_hir_text_as(&text, mode)
//! ```

use w16_ir::hir::{
    BinaryOp, CastKind, Expr, Function, Literal, Module,
    ReturnType, Stmt, Type, UnaryOp,
};

/// Генератор текстового HIR.
pub struct TextEmitter {
    out: String,
    indent: usize,
}

impl TextEmitter {
    /// Сгенерировать текстовый HIR из модуля.
    pub fn emit(module: &Module) -> String {
        let mut e = Self { out: String::new(), indent: 0 };
        e.emit_module(module);
        e.out
    }

    // -----------------------------------------------------------------------
    // Модуль
    // -----------------------------------------------------------------------

    fn emit_module(&mut self, m: &Module) {
        self.push_str(&format!("module {} {{\n", m.name));
        self.indent += 1;

        for c in &m.constants {
            self.push_indent();
            self.push_str(&format!(
                "const {}: {} = {}\n",
                c.name,
                type_str(c.ty),
                lit_str(&c.value),
            ));
        }

        if !m.constants.is_empty() { self.push_str("\n"); }

        for (i, f) in m.functions.iter().enumerate() {
            self.emit_function(f);
            if i + 1 < m.functions.len() { self.push_str("\n"); }
        }

        self.indent -= 1;
        self.push_str("}\n");
    }

    // -----------------------------------------------------------------------
    // Функция
    // -----------------------------------------------------------------------

    fn emit_function(&mut self, f: &Function) {
        self.push_indent();
        let params = f.params.iter()
            .map(|p| format!("${}: {}", p.name, type_str(p.ty)))
            .collect::<Vec<_>>()
            .join(", ");
        let ret = return_type_str(&f.return_ty);
        self.push_str(&format!("fn @{}({}) -> {} {{\n", f.name, params, ret));
        self.indent += 1;

        for stmt in &f.body {
            self.emit_stmt(stmt);
        }

        self.indent -= 1;
        self.push_indent();
        self.push_str("}\n");
    }

    // -----------------------------------------------------------------------
    // Операторы
    // -----------------------------------------------------------------------

    fn emit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let { name, ty, value } => {
                self.push_indent();
                self.push_str(&format!(
                    "let ${}: {} = {};\n",
                    name, type_str(*ty), self.expr_str(value)
                ));
            }

            Stmt::Assign { name, value } => {
                self.push_indent();
                self.push_str(&format!("${} = {};\n", name, self.expr_str(value)));
            }

            Stmt::If { cond, then_body, else_body } => {
                self.push_indent();
                self.push_str(&format!("if ({}) {{\n", self.expr_str(cond)));
                self.indent += 1;
                for s in then_body { self.emit_stmt(s); }
                self.indent -= 1;
                if else_body.is_empty() {
                    self.push_indent();
                    self.push_str("}\n");
                } else {
                    self.push_indent();
                    self.push_str("} else {\n");
                    self.indent += 1;
                    for s in else_body { self.emit_stmt(s); }
                    self.indent -= 1;
                    self.push_indent();
                    self.push_str("}\n");
                }
            }

            Stmt::While { cond, body } => {
                self.push_indent();
                self.push_str(&format!("while ({}) {{\n", self.expr_str(cond)));
                self.indent += 1;
                for s in body { self.emit_stmt(s); }
                self.indent -= 1;
                self.push_indent();
                self.push_str("}\n");
            }

            Stmt::DoWhile { body, cond } => {
                // do-while в текстовом HIR: раскрываем как тело + while
                // (текстовый парсер w16-ir понимает do-while если ты его добавил,
                //  иначе раскрываем вручную)
                self.push_indent();
                self.push_str("do {\n");
                self.indent += 1;
                for s in body { self.emit_stmt(s); }
                self.indent -= 1;
                self.push_indent();
                self.push_str(&format!("}} while ({});\n", self.expr_str(cond)));
            }

            Stmt::Return(vals) => {
                self.push_indent();
                if vals.is_empty() {
                    self.push_str("return ();\n");
                } else if vals.len() == 1 {
                    self.push_str(&format!("return {};\n", self.expr_str(&vals[0])));
                } else {
                    let vs = vals.iter().map(|e| self.expr_str(e)).collect::<Vec<_>>().join(", ");
                    self.push_str(&format!("return ({});\n", vs));
                }
            }

            Stmt::Break => { self.push_indent(); self.push_str("break;\n"); }
            Stmt::Continue => { self.push_indent(); self.push_str("continue;\n"); }
            Stmt::Halt => { self.push_indent(); self.push_str("halt;\n"); }

            Stmt::Print(exprs) => {
                self.push_indent();
                let args = exprs.iter().map(|e| self.expr_str(e)).collect::<Vec<_>>().join(", ");
                self.push_str(&format!("print({});\n", args));
            }

            Stmt::Expr(e) => {
                self.push_indent();
                self.push_str(&format!("{};\n", self.expr_str(e)));
            }
        }
    }

    // -----------------------------------------------------------------------
    // Выражения
    // -----------------------------------------------------------------------

    fn expr_str(&self, expr: &Expr) -> String {
        match expr {
            Expr::Literal(lit) => lit_str(lit),
            Expr::Local(name) => format!("${name}"),
            Expr::Const(name) => name.clone(),

            Expr::Call { function, args } => {
                let a = args.iter().map(|e| self.expr_str(e)).collect::<Vec<_>>().join(", ");
                format!("@{function}({a})")
            }

            Expr::Unary { op, expr } => {
                let op_str = match op {
                    UnaryOp::Not => "!",
                    UnaryOp::Neg => "-",
                };
                format!("{}{}", op_str, self.expr_str(expr))
            }

            Expr::Binary { op, lhs, rhs } => {
                format!(
                    "({} {} {})",
                    self.expr_str(lhs),
                    binop_str(*op),
                    self.expr_str(rhs),
                )
            }

            Expr::Select { cond, then_value, else_value } => {
                format!(
                    "select({}, {}, {})",
                    self.expr_str(cond),
                    self.expr_str(then_value),
                    self.expr_str(else_value),
                )
            }

            Expr::Cast { kind, expr } => {
                format!("cast.{}({})", cast_kind_str(*kind), self.expr_str(expr))
            }

            Expr::Load { ty, addr } => {
                format!("load.{}({})", type_str(*ty), self.expr_str(addr))
            }

            Expr::Store { ty, addr, value } => {
                format!(
                    "store.{}({}, {})",
                    type_str(*ty),
                    self.expr_str(addr),
                    self.expr_str(value),
                )
            }
        }
    }

    // -----------------------------------------------------------------------
    // Вспомогательные
    // -----------------------------------------------------------------------

    fn push_indent(&mut self) {
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
    }

    fn push_str(&mut self, s: &str) {
        self.out.push_str(s);
    }
}

// ---------------------------------------------------------------------------
// Свободные функции-хелперы
// ---------------------------------------------------------------------------

fn type_str(ty: Type) -> &'static str {
    match ty {
        Type::I64 => "i64",
        Type::U64 => "u64",
        Type::F64 => "f64",
        Type::Bool => "bool",
        Type::Ptr => "ptr",
        Type::Unit => "unit",
    }
}

fn return_type_str(rt: &ReturnType) -> String {
    match rt {
        ReturnType::Unit => "()".to_owned(),
        ReturnType::Single(ty) => type_str(*ty).to_owned(),
        ReturnType::Tuple(tys) => {
            let inner = tys.iter().map(|t| type_str(*t)).collect::<Vec<_>>().join(", ");
            format!("({inner})")
        }
    }
}

fn lit_str(lit: &Literal) -> String {
    match lit {
        Literal::Int(v) => v.to_string(),
        Literal::Float(v) => format!("{v:?}"),   // сохраняем точность
        Literal::Bool(b) => b.to_string(),
        Literal::String(s) => format!("{s:?}"),   // экранируем
    }
}

fn binop_str(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Rem => "%",
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Le => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::Ge => ">=",
        BinaryOp::BitAnd => "&",
        BinaryOp::BitOr => "|",
        BinaryOp::BitXor => "^",
        BinaryOp::Shl => "<<",
        BinaryOp::Shr => ">>",
    }
}

fn cast_kind_str(kind: CastKind) -> &'static str {
    match kind {
        CastKind::I2F => "i2f",
        CastKind::U2F => "u2f",
        CastKind::F2I => "f2i",
        CastKind::F2U => "f2u",
        CastKind::I2U => "i2u",
        CastKind::U2I => "u2i",
        CastKind::TruncU64ToU32 => "trunc_u64_to_u32",
        CastKind::ZextU32ToU64 => "zext_u32_to_u64",
        CastKind::SextI32ToI64 => "sext_i32_to_i64",
        CastKind::Bitcast => "bitcast",
    }
}
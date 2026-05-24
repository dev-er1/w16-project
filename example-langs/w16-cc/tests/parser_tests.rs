use w16_cc::W16CFrontend;
use w16_cc::frontend::parser::node::*;
use w16_cc::types::Type;

// ---------------------------------------------------------------------------
// Хелперы
// ---------------------------------------------------------------------------

/// Парсит строку и возвращает `TranslationUnit`. Паникует при ошибке.
fn parse(src: &str) -> TranslationUnit {
    W16CFrontend::new(src)
        .get_ast()
        .unwrap_or_else(|errs| {
            panic!("parse error(s):\n{errs:#?}");
        })
}

/// Парсит строку и ожидает ошибку парсера.
fn parse_err(src: &str) {
    let result = W16CFrontend::new(src).get_ast();
    assert!(result.is_err(), "expected parse error, but got Ok");
}

/// Извлекает единственный `ExternalDecl::FunctionDef` из единицы трансляции.
fn single_fn(src: &str) -> FunctionDef {
    let tu = parse(src);
    assert_eq!(tu.items.len(), 1);
    match tu.items.into_iter().next().unwrap() {
        ExternalDecl::FunctionDef(f) => f,
        other => panic!("expected FunctionDef, got {other:?}"),
    }
}

/// Извлекает единственный `ExternalDecl::Decl` из единицы трансляции.
fn single_decl(src: &str) -> Decl {
    let tu = parse(src);
    assert_eq!(tu.items.len(), 1);
    match tu.items.into_iter().next().unwrap() {
        ExternalDecl::Decl(d) => d,
        other => panic!("expected Decl, got {other:?}"),
    }
}

/// Извлекает единственный оператор из тела функции.
fn single_stmt(src: &str) -> Stmt {
    let f = single_fn(src);
    assert_eq!(f.body.stmts.len(), 1, "expected exactly 1 statement in body");
    f.body.stmts.into_iter().next().unwrap()
}

/// Извлекает единственное выражение-оператор.
fn single_expr_stmt(src: &str) -> Expr {
    match single_stmt(src).kind {
        StmtKind::Expr(e) => e,
        other => panic!("expected Expr stmt, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Глобальные объявления переменных
// ---------------------------------------------------------------------------

#[test]
fn test_global_int() {
    let decl = single_decl("int x;");
    match decl.kind {
        DeclKind::Var(v) => assert!(matches!(v.ty, Type::Int)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_global_var_with_initializer() {
    let decl = single_decl("int x = 42;");
    match decl.kind {
        DeclKind::Var(v) => {
            assert!(matches!(v.ty, Type::Int));
            assert!(v.initializer.is_some());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_global_multi_var() {
    let decl = single_decl("int a, b, c;");
    assert!(matches!(decl.kind, DeclKind::MultiVar(_)));
    match decl.kind {
        DeclKind::MultiVar(vars) => assert_eq!(vars.len(), 3),
        _ => unreachable!(),
    }
}

#[test]
fn test_global_const_var() {
    let decl = single_decl("const int MAX = 100;");
    match decl.kind {
        DeclKind::Var(v) => {
            assert!(v.qualifiers.is_const);
            assert!(v.initializer.is_some());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_global_static_var() {
    let decl = single_decl("static int counter = 0;");
    match decl.kind {
        DeclKind::Var(v) => assert!(matches!(v.storage, StorageClass::Static)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_array_decl() {
    let decl = single_decl("int arr[];");
    match decl.kind {
        DeclKind::Var(v) => assert!(matches!(v.ty, Type::Array(..))),
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_typedef() {
    let decl = single_decl("typedef int MyInt;");
    assert!(matches!(decl.kind, DeclKind::Typedef { .. }));
}

// ---------------------------------------------------------------------------
// Определения функций
// ---------------------------------------------------------------------------

#[test]
fn test_empty_fn() {
    let f = single_fn("void foo(void) {}");
    assert!(matches!(f.return_ty, Type::Void));
    assert!(f.params.is_empty());
    assert!(f.body.stmts.is_empty());
}

#[test]
fn test_fn_with_params() {
    let f = single_fn("int add(int a, int b) { return a; }");
    assert_eq!(f.params.len(), 2);
    assert!(matches!(f.params[0].ty, Type::Int));
    assert!(matches!(f.params[1].ty, Type::Int));
}

#[test]
fn test_fn_no_params() {
    let f = single_fn("int get() { return 0; }");
    assert!(f.params.is_empty());
}

#[test]
fn test_inline_fn() {
    let f = single_fn("inline int square(int x) { return x; }");
    assert!(f.is_inline);
}

#[test]
fn test_static_fn() {
    let f = single_fn("static void helper() {}");
    assert!(f.is_static);
}

#[test]
fn test_fn_multiple_stmts() {
    let f = single_fn("int foo() { int x = 1; int y = 2; return x; }");
    assert_eq!(f.body.stmts.len(), 3);
}

// ---------------------------------------------------------------------------
// Операторы
// ---------------------------------------------------------------------------

#[test]
fn test_return_void() {
    let stmt = single_stmt("void f() { return; }");
    assert!(matches!(stmt.kind, StmtKind::Return(None)));
}

#[test]
fn test_return_expr() {
    let stmt = single_stmt("int f() { return 42; }");
    assert!(matches!(stmt.kind, StmtKind::Return(Some(_))));
}

#[test]
fn test_empty_stmt() {
    let stmt = single_stmt("void f() { ; }");
    assert!(matches!(stmt.kind, StmtKind::Empty));
}

#[test]
fn test_block_stmt() {
    let stmt = single_stmt("void f() { { ; } }");
    assert!(matches!(stmt.kind, StmtKind::Block(_)));
}

#[test]
fn test_if_no_else() {
    let stmt = single_stmt("void f() { if (1) return; }");
    match stmt.kind {
        StmtKind::If { alt, .. } => assert!(alt.is_none()),
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_if_else() {
    let stmt = single_stmt("void f() { if (1) return; else return; }");
    match stmt.kind {
        StmtKind::If { alt, .. } => assert!(alt.is_some()),
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_while_loop() {
    let stmt = single_stmt("void f() { while (1) ; }");
    assert!(matches!(stmt.kind, StmtKind::While { .. }));
}

#[test]
fn test_do_while() {
    let stmt = single_stmt("void f() { do ; while (0); }");
    assert!(matches!(stmt.kind, StmtKind::DoWhile { .. }));
}

#[test]
fn test_for_full() {
    let stmt = single_stmt("void f() { for (int i = 0; i; i) ; }");
    match stmt.kind {
        StmtKind::For { init, cond, step, .. } => {
            assert!(init.is_some());
            assert!(cond.is_some());
            assert!(step.is_some());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_for_empty() {
    // `for (;;)` — бесконечный цикл
    let stmt = single_stmt("void f() { for (;;) ; }");
    match stmt.kind {
        StmtKind::For { init, cond, step, .. } => {
            assert!(init.is_none());
            assert!(cond.is_none());
            assert!(step.is_none());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_break() {
    let stmt = single_stmt("void f() { while (1) break; }");
    // break вложен в while
    match stmt.kind {
        StmtKind::While { body, .. } => {
            assert!(matches!(body.kind, StmtKind::Break));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_continue() {
    let stmt = single_stmt("void f() { while (1) continue; }");
    match stmt.kind {
        StmtKind::While { body, .. } => {
            assert!(matches!(body.kind, StmtKind::Continue));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_goto_and_label() {
    let f = single_fn("void f() { loop: goto loop; }");
    assert_eq!(f.body.stmts.len(), 2);
    assert!(matches!(f.body.stmts[0].kind, StmtKind::Label(_)));
    assert!(matches!(f.body.stmts[1].kind, StmtKind::Goto(_)));
}

#[test]
fn test_switch() {
    let stmt = single_stmt(
        "void f() { switch (x) { case 1: break; default: break; } }"
    );
    assert!(matches!(stmt.kind, StmtKind::Switch { .. }));
}

#[test]
fn test_local_var_decl() {
    let stmt = single_stmt("void f() { int x = 0; }");
    assert!(matches!(stmt.kind, StmtKind::Decl(_)));
}

// ---------------------------------------------------------------------------
// Выражения
// ---------------------------------------------------------------------------

#[test]
fn test_int_literal() {
    let expr = single_expr_stmt("void f() { 42; }");
    assert!(matches!(expr.kind, ExprKind::Literal(_)));
}

#[test]
fn test_ident_expr() {
    let expr = single_expr_stmt("void f() { x; }");
    assert!(matches!(expr.kind, ExprKind::Ident(_)));
}

#[test]
fn test_binary_add() {
    let expr = single_expr_stmt("void f() { 1 + 2; }");
    match expr.kind {
        ExprKind::Binary { op, .. } => assert!(matches!(op, BinaryOp::Add)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_binary_precedence() {
    // `1 + 2 * 3` должно стать `1 + (2 * 3)`, то есть
    // корень — Add, правый ребёнок — Mul.
    let expr = single_expr_stmt("void f() { 1 + 2 * 3; }");
    match expr.kind {
        ExprKind::Binary { op: BinaryOp::Add, rhs, .. } => {
            assert!(matches!(rhs.kind, ExprKind::Binary { op: BinaryOp::Mul, .. }));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_binary_left_assoc() {
    // `1 - 2 - 3` → `(1 - 2) - 3`: корень — Sub, lhs тоже Sub.
    let expr = single_expr_stmt("void f() { 1 - 2 - 3; }");
    match expr.kind {
        ExprKind::Binary { op: BinaryOp::Sub, lhs, .. } => {
            assert!(matches!(lhs.kind, ExprKind::Binary { op: BinaryOp::Sub, .. }));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_assign_right_assoc() {
    // `a = b = 1` → `a = (b = 1)`: корень — Assign, rhs тоже Assign.
    let expr = single_expr_stmt("void f() { a = b = 1; }");
    match expr.kind {
        ExprKind::Binary { op: BinaryOp::Assign, rhs, .. } => {
            assert!(matches!(rhs.kind, ExprKind::Binary { op: BinaryOp::Assign, .. }));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_compound_assign() {
    let expr = single_expr_stmt("void f() { x += 1; }");
    match expr.kind {
        ExprKind::Binary { op, .. } => assert!(matches!(op, BinaryOp::AddAssign)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_unary_neg() {
    let expr = single_expr_stmt("void f() { -x; }");
    match expr.kind {
        ExprKind::Unary { op, .. } => assert!(matches!(op, UnaryOp::Neg)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_unary_not() {
    let expr = single_expr_stmt("void f() { !x; }");
    match expr.kind {
        ExprKind::Unary { op, .. } => assert!(matches!(op, UnaryOp::Not)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_pre_inc() {
    let expr = single_expr_stmt("void f() { ++x; }");
    match expr.kind {
        ExprKind::Unary { op, .. } => assert!(matches!(op, UnaryOp::PreInc)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_post_inc() {
    let expr = single_expr_stmt("void f() { x++; }");
    match expr.kind {
        ExprKind::Unary { op, .. } => assert!(matches!(op, UnaryOp::PostInc)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_ternary() {
    let expr = single_expr_stmt("void f() { a ? b : c; }");
    assert!(matches!(expr.kind, ExprKind::Ternary { .. }));
}

#[test]
fn test_call_no_args() {
    let expr = single_expr_stmt("void f() { foo(); }");
    match expr.kind {
        ExprKind::Call { args, .. } => assert!(args.is_empty()),
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_call_with_args() {
    let expr = single_expr_stmt("void f() { foo(1, 2, 3); }");
    match expr.kind {
        ExprKind::Call { args, .. } => assert_eq!(args.len(), 3),
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_array_index() {
    let expr = single_expr_stmt("void f() { arr[0]; }");
    assert!(matches!(expr.kind, ExprKind::Index { .. }));
}

#[test]
fn test_field_access() {
    let expr = single_expr_stmt("void f() { s.x; }");
    assert!(matches!(expr.kind, ExprKind::Field { .. }));
}

#[test]
fn test_cast() {
    let expr = single_expr_stmt("void f() { (int)x; }");
    match expr.kind {
        ExprKind::Cast { ty, .. } => assert!(matches!(ty, Type::Int)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_sizeof_type() {
    let expr = single_expr_stmt("void f() { sizeof(int); }");
    match expr.kind {
        ExprKind::Sizeof(SizeofArg::Type(ty)) => assert!(matches!(ty, Type::Int)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_sizeof_expr() {
    let expr = single_expr_stmt("void f() { sizeof x; }");
    assert!(matches!(expr.kind, ExprKind::Sizeof(SizeofArg::Expr(_))));
}

#[test]
fn test_comma_expr() {
    let expr = single_expr_stmt("void f() { (a, b, c); }");
    // Запятая внутри скобок — ExprKind::Comma.
    assert!(matches!(expr.kind, ExprKind::Comma(_)));
    match expr.kind {
        ExprKind::Comma(exprs) => assert_eq!(exprs.len(), 3),
        _ => unreachable!(),
    }
}

#[test]
fn test_logical_and_or() {
    // `a && b || c` → `(a && b) || c` — Or имеет меньший приоритет.
    let expr = single_expr_stmt("void f() { a && b || c; }");
    match expr.kind {
        ExprKind::Binary { op: BinaryOp::Or, lhs, .. } => {
            assert!(matches!(lhs.kind, ExprKind::Binary { op: BinaryOp::And, .. }));
        }
        other => panic!("{other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Struct / Union / Enum
// ---------------------------------------------------------------------------

#[test]
fn test_struct_def() {
    let decl = single_decl("struct Point { int x; int y; };");
    match decl.kind {
        DeclKind::StructDef(s) => {
            assert_eq!(s.fields.len(), 2);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_union_def() {
    let decl = single_decl("union Data { int i; float f; };");
    match decl.kind {
        DeclKind::UnionDef(u) => {
            assert_eq!(u.fields.len(), 2);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_enum_def() {
    let decl = single_decl("enum Color { Red, Green, Blue };");
    match decl.kind {
        DeclKind::EnumDef(e) => {
            assert_eq!(e.variants.len(), 3);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_enum_with_values() {
    let decl = single_decl("enum Flags { A = 1, B = 2, C = 4 };");
    match decl.kind {
        DeclKind::EnumDef(e) => {
            assert!(e.variants.iter().all(|v| v.value.is_some()));
        }
        other => panic!("{other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Множество деклараций верхнего уровня
// ---------------------------------------------------------------------------

#[test]
fn test_multiple_top_level() {
    let tu = parse("int x; int y; void f() {}");
    assert_eq!(tu.items.len(), 3);
}

#[test]
fn test_fn_then_global() {
    let tu = parse("void foo() {} int bar;");
    assert!(matches!(tu.items[0], ExternalDecl::FunctionDef(_)));
    assert!(matches!(tu.items[1], ExternalDecl::Decl(_)));
}

// ---------------------------------------------------------------------------
// Ошибки
// ---------------------------------------------------------------------------

#[test]
fn test_missing_semicolon() {
    parse_err("int x");
}

#[test]
fn test_missing_closing_brace() {
    parse_err("void f() {");
}

#[test]
fn test_empty_input() {
    let tu = parse("");
    assert!(tu.items.is_empty());
}
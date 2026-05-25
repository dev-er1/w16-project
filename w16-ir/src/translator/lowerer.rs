// w16-ir\src\translator\lowerer.rs
//
//! # Переводчик W16-HIR в W16-MIR
//!
//! HIR -> MIR: структурный control flow превращается в явный SSA CFG.
//! Mutable HIR locals становятся SSA-значениями и параметрами блоков
//! там, где значения проходят через control-flow edges (while, if).
//!
//! ## Инварианты которые мы гарантируем:
//! - Каждый BasicBlock имеет ровно один terminator.
//! - Каждый ValueId имеет ровно одно определение (SSA).
//! - `predecessors` заполнены для всех блоков.
//! - `values[id].uses` отслеживают все места использования.
//! - Литералы и константы материализуются как `MIRInst::Const` -- не "призраки".

use std::collections::HashMap;

use crate::hir::{self, BinaryOp, CastKind, Expr, Function, Literal, Module, Stmt, Type, UnaryOp};
use crate::mir::{
    self, BasicBlock, CastKind as MirCastKind, FCmpOp, FunctionAnalysis, ICmpOp, MIRConstant,
    MIRFunction, MIRInst, MIRModule, MIRTerminator, ValueData, ValueDef, ValueId,
};

// =============================================================================
// КОНТЕКСТ ПЕРЕВОДА
// =============================================================================

/// Контекст перевода одной HIR-функции в MIR-функцию.
///
/// Намеренно не хранит `&[MIRConstant]` -- это устраняет borrow-конфликт:
/// `lower_expr` получает `constants: &mut Vec<MIRConstant>` напрямую,
/// поэтому Rust не видит одновременного shared+mutable borrow на один Vec.
///
/// Резолв по имени (`Expr::Const`) передаётся через отдельный `&[MIRConstant]`
/// который берётся как snapshot до начала мутации.
struct Ctx<'f> {
    next_value_id: ValueId,
    next_block_id: mir::BlockId,
    current_block: mir::BlockId,
    blocks: Vec<BasicBlock>,
    value_types: Vec<Type>,
    values: Vec<ValueData>,
    locals: HashMap<String, ValueId>,

    /// true если блок[i] уже получил финальный terminator (не заглушку)
    terminated: Vec<bool>,

    /// Карта имён функций -> FunctionId; нужна для резолва `Expr::Call`
    func_name_to_id: &'f HashMap<String, mir::FunctionId>,

    /// Стек блоков-выходов из циклов (куда прыгает `break`).
    loop_exit_stack: Vec<mir::BlockId>,

    /// Стек заголовков циклов (куда прыгает `continue`).
    loop_header_stack: Vec<mir::BlockId>,
}

impl<'f> Ctx<'f> {
    fn new(func_name_to_id: &'f HashMap<String, mir::FunctionId>) -> Self {
        Self {
            next_value_id: 0,
            next_block_id: 0,
            current_block: 0,
            blocks: Vec::new(),
            value_types: Vec::new(),
            values: Vec::new(),
            locals: HashMap::new(),
            terminated: Vec::new(),
            func_name_to_id,
            loop_exit_stack: Vec::new(),
            loop_header_stack: Vec::new()
        }
    }

    // -------------------------------------------------------------------------
    // SSA
    // -------------------------------------------------------------------------

    fn new_value(&mut self, typ: Type, def: ValueDef) -> ValueId {
        let id = self.next_value_id;
        self.next_value_id += 1;
        self.value_types.push(typ);
        self.values.push(ValueData {
            typ,
            def,
            uses: Vec::new(),
            is_dead: false,
        });
        id
    }

    fn record_use(&mut self, used: ValueId, by: ValueId) {
        self.values[used].uses.push(by);
    }

    fn ty(&self, id: ValueId) -> Type {
        self.value_types[id]
    }

    // -------------------------------------------------------------------------
    // Блоки
    // -------------------------------------------------------------------------

    fn new_block(&mut self, name: impl Into<String>) -> mir::BlockId {
        let id = self.next_block_id;
        self.next_block_id += 1;
        self.blocks.push(BasicBlock {
            id,
            name: name.into(),
            params: Vec::new(),
            instructions: Vec::new(),
            predecessors: Vec::new(),
            terminator: MIRTerminator::Halt, // заглушка; заменится через set_term
            is_dead: false,
        });
        self.terminated.push(false);
        id
    }

    fn switch_to(&mut self, id: mir::BlockId) {
        self.current_block = id;
    }

    /// Устанавливает terminator текущего блока.
    ///
    /// Если блок уже завершён -- вызов игнорируется. Это позволяет спокойно
    /// выставлять `Jmp merge` после then/else веток, которые могли уже
    /// завершиться через `return` или `halt`.
    fn set_term(&mut self, term: MIRTerminator) {
        if self.terminated[self.current_block] {
            return;
        }
        // Регистрируем CFG-рёбра для predecessors
        match &term {
            MIRTerminator::Jmp { target, .. } => {
                let (cur, t) = (self.current_block, *target);
                self.add_pred(t, cur);
            }
            MIRTerminator::Br {
                then_blk, else_blk, ..
            } => {
                let (cur, t, e) = (self.current_block, *then_blk, *else_blk);
                self.add_pred(t, cur);
                self.add_pred(e, cur);
            }
            _ => {}
        }
        self.blocks[self.current_block].terminator = term;
        self.terminated[self.current_block] = true;
    }

    fn is_terminated(&self) -> bool {
        self.terminated[self.current_block]
    }

    fn add_pred(&mut self, block: mir::BlockId, pred: mir::BlockId) {
        if !self.blocks[block].predecessors.contains(&pred) {
            self.blocks[block].predecessors.push(pred);
        }
    }

    // -------------------------------------------------------------------------
    // Эмит инструкций
    // -------------------------------------------------------------------------

    /// Эмитит инструкцию в текущий блок.
    /// Возвращает ValueId результата. Автоматически отслеживает uses операндов.
    fn emit(&mut self, inst: MIRInst) -> ValueId {
        let typ = infer_type(&inst);
        let inst_idx = self.blocks[self.current_block].instructions.len();
        let result = self.new_value(typ, ValueDef::Inst(self.current_block, inst_idx));

        for operand in inst_operands(&inst) {
            self.record_use(operand, result);
        }

        self.blocks[self.current_block].instructions.push(inst);
        result
    }

    // -------------------------------------------------------------------------
    // Утилиты
    // -------------------------------------------------------------------------

    fn resolve_func(&self, name: &str) -> mir::FunctionId {
        *self
            .func_name_to_id
            .get(name)
            .unwrap_or_else(|| panic!("Undefined function: @{name}"))
    }
}

// =============================================================================
// ВСПОМОГАТЕЛЬНЫЕ ФУНКЦИИ
// =============================================================================

/// Определяет тип результата MIR-инструкции.
pub fn infer_type(inst: &MIRInst) -> Type {
    match inst {
        MIRInst::Add(..)
        | MIRInst::Sub(..)
        | MIRInst::Mul(..)
        | MIRInst::UDiv(..)
        | MIRInst::URem(..)
        | MIRInst::And(..)
        | MIRInst::Or(..)
        | MIRInst::Xor(..)
        | MIRInst::Not(..)
        | MIRInst::Shl(..)
        | MIRInst::Shr(..)
        | MIRInst::Mov(..)
        | MIRInst::Const(_)  // реальный тип в MIRConstant, здесь U64 как placeholder
        | MIRInst::Select { .. }
        | MIRInst::Call { .. } => Type::U64,

        MIRInst::IDiv(..) | MIRInst::IRem(..) | MIRInst::Neg(..) | MIRInst::Sar(..) => Type::I64,

        MIRInst::FAdd(..)
        | MIRInst::FSub(..)
        | MIRInst::FMul(..)
        | MIRInst::FDiv(..)
        | MIRInst::FRem(..)
        | MIRInst::FNeg(..)
        | MIRInst::FAbs(..) => Type::F64,

        MIRInst::ICmp { .. } | MIRInst::FCmp { .. } => Type::Bool,

        MIRInst::Cast { dest_typ, .. } => *dest_typ,
        MIRInst::Load { typ, .. } => *typ,
        MIRInst::Store { .. } => Type::Unit,
        MIRInst::PrintInt(_)
        | MIRInst::PrintUInt(_)
        | MIRInst::PrintFloat(_)
        | MIRInst::PrintStr(_) => Type::Unit,
    }
}

/// Возвращает все ValueId которые инструкция использует как операнды.
fn inst_operands(inst: &MIRInst) -> Vec<ValueId> {
    match inst {
        MIRInst::Add(a, b)
        | MIRInst::Sub(a, b)
        | MIRInst::Mul(a, b)
        | MIRInst::UDiv(a, b)
        | MIRInst::IDiv(a, b)
        | MIRInst::URem(a, b)
        | MIRInst::IRem(a, b)
        | MIRInst::FAdd(a, b)
        | MIRInst::FSub(a, b)
        | MIRInst::FMul(a, b)
        | MIRInst::FDiv(a, b)
        | MIRInst::FRem(a, b)
        | MIRInst::And(a, b)
        | MIRInst::Or(a, b)
        | MIRInst::Xor(a, b)
        | MIRInst::Shl(a, b)
        | MIRInst::Shr(a, b)
        | MIRInst::Sar(a, b) => vec![*a, *b],

        MIRInst::Neg(a)
        | MIRInst::FNeg(a)
        | MIRInst::FAbs(a)
        | MIRInst::Not(a)
        | MIRInst::Mov(a) => vec![*a],

        MIRInst::ICmp { lhs, rhs, .. } | MIRInst::FCmp { lhs, rhs, .. } => vec![*lhs, *rhs],

        MIRInst::Cast { src, .. } => vec![*src],
        MIRInst::Load { addr, .. } => vec![*addr],
        MIRInst::Store { addr, value } => vec![*addr, *value],
        MIRInst::Select {
            cond,
            then_v,
            else_v,
        } => vec![*cond, *then_v, *else_v],
        MIRInst::Call { args, .. } => args.clone(),
        MIRInst::Const(_) => vec![],
        MIRInst::PrintInt(a)
        | MIRInst::PrintUInt(a)
        | MIRInst::PrintFloat(a)
        | MIRInst::PrintStr(a) => vec![*a],
    }
}

// =============================================================================
// ТОЧКА ВХОДА
// =============================================================================

/// Переводит HIR-модуль в MIR-модуль.
pub fn lower_hir_to_mir(hir_module: &Module) -> MIRModule {
    // Материализуем константы модуля
    let mut constants: Vec<MIRConstant> = hir_module
        .constants
        .iter()
        .map(|c| MIRConstant {
            name: c.name.clone(),
            typ: c.ty,
            value: c.value.clone(),
        })
        .collect();

    // Карта имён функций -> FunctionId (строится один раз, используется при резолве Call)
    let func_name_to_id: HashMap<String, mir::FunctionId> = hir_module
        .functions
        .iter()
        .enumerate()
        .map(|(id, f)| (f.name.clone(), id))
        .collect();

    let mut functions = Vec::with_capacity(hir_module.functions.len());
    for (func_id, hir_func) in hir_module.functions.iter().enumerate() {
        let mir_func = lower_function(hir_func, func_id, &mut constants, &func_name_to_id);
        functions.push(mir_func);
    }

    // Один FunctionAnalysis на каждую функцию, все начинают как невалидные  --
    // анализ пересчитается при первом обращении оптимизатора или верификатора.
    let analysis = (0..functions.len())
        .map(|_| FunctionAnalysis::default())
        .collect();

    MIRModule {
        name: hir_module.name.clone(),
        constants,
        functions,
        analysis,
    }
}

// =============================================================================
// ПЕРЕВОД ФУНКЦИИ
// =============================================================================

fn lower_function(
    hir_func: &Function,
    func_id: mir::FunctionId,
    constants: &mut Vec<MIRConstant>,
    func_name_to_id: &HashMap<String, mir::FunctionId>,
) -> MIRFunction {
    // ctx не хранит &constants -- это ключ к отсутствию borrow-конфликта
    let mut ctx = Ctx::new(func_name_to_id);

    let entry = ctx.new_block("entry");
    ctx.switch_to(entry);

    let mut param_types = Vec::new();
    for param in &hir_func.params {
        param_types.push(param.ty);
        let val = ctx.new_value(param.ty, ValueDef::External);
        ctx.locals.insert(param.name.clone(), val);
    }

    for stmt in &hir_func.body {
        lower_stmt(&mut ctx, stmt, constants);
    }

    if !ctx.is_terminated() {
        ctx.set_term(MIRTerminator::Ret(Vec::new()));
    }

    let return_types = match &hir_func.return_ty {
        hir::ReturnType::Unit => vec![],
        hir::ReturnType::Single(ty) => vec![*ty],
        hir::ReturnType::Tuple(tys) => tys.clone(),
    };

    MIRFunction {
        id: func_id,
        name: hir_func.name.clone(),
        return_types,
        blocks: ctx.blocks,
        param_types,
        value_types: ctx.value_types,
        values: ctx.values,
    }
}

// =============================================================================
// ПЕРЕВОД ОПЕРАТОРОВ
// =============================================================================

fn lower_stmt(ctx: &mut Ctx, stmt: &Stmt, constants: &mut Vec<MIRConstant>) {
    // Недостижимые операторы после return/halt пропускаем
    if ctx.is_terminated() {
        return;
    }

    match stmt {
        // -----------------------------------------------------------------
        // let $name: ty = expr
        // -----------------------------------------------------------------
        Stmt::Let { name, value, .. } => {
            let val = lower_expr(ctx, value, constants);
            ctx.locals.insert(name.clone(), val);
        }

        // -----------------------------------------------------------------
        // $name = expr
        // SSA rename: locals map обновляется, старое значение остаётся
        // доступным через uses и будет убрано DCE если не используется.
        // -----------------------------------------------------------------
        Stmt::Assign { name, value } => {
            let val = lower_expr(ctx, value, constants);
            ctx.locals.insert(name.clone(), val);
        }

        // -----------------------------------------------------------------
        // if (cond) { then } else { else }
        //
        //   ^current -> Br cond, ^if_then, ^if_else
        //   ^if_then  -> ... -> Jmp ^if_merge  (если не завершился)
        //   ^if_else  -> ... -> Jmp ^if_merge  (если не завершился)
        //   ^if_merge -> (продолжение)
        // -----------------------------------------------------------------
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            let cond_val = lower_expr(ctx, cond, constants);
            let then_blk = ctx.new_block("if_then");
            let else_blk = ctx.new_block("if_else");
            let merge_blk = ctx.new_block("if_merge");

            ctx.set_term(MIRTerminator::Br {
                cond: cond_val,
                then_blk,
                then_args: Vec::new(),
                else_blk,
                else_args: Vec::new(),
            });

            // Снимок locals ДО веток — оба блока стартуют с одного состояния.
            // Без этого else видит изменения then, что нарушает SSA-доминирование.
            let pre_if_locals = ctx.locals.clone();

            // --- Then-ветка ---
            ctx.switch_to(then_blk);
            ctx.locals = pre_if_locals.clone();
            for s in then_body {
                lower_stmt(ctx, s, constants);
            }
            let then_locals = ctx.locals.clone();
            let then_terminated = ctx.is_terminated();
            if !then_terminated {
                ctx.set_term(MIRTerminator::Jmp {
                    target: merge_blk,
                    args: Vec::new(),
                });
            }

            // --- Else-ветка: стартует с pre_if_locals, не с then_locals ---
            ctx.switch_to(else_blk);
            ctx.locals = pre_if_locals.clone();
            for s in else_body {
                lower_stmt(ctx, s, constants);
            }
            let else_locals = ctx.locals.clone();
            let else_terminated = ctx.is_terminated();
            if !else_terminated {
                ctx.set_term(MIRTerminator::Jmp {
                    target: merge_blk,
                    args: Vec::new(),
                });
            }

            // --- Merge: phi-параметры для переменных изменённых в ветках ---
            ctx.switch_to(merge_blk);

            let changed_names: Vec<String> = pre_if_locals
                .keys()
                .filter(|name| {
                    let pre = pre_if_locals[*name];
                    let t_val = then_locals.get(*name).copied().unwrap_or(pre);
                    let e_val = else_locals.get(*name).copied().unwrap_or(pre);
                    t_val != pre || e_val != pre
                })
                .cloned()
                .collect();

            let needs_phi = !(changed_names.is_empty() || then_terminated && else_terminated);

            if needs_phi {
                for name in &changed_names {
                    let pre_id = pre_if_locals[name];
                    let ty = ctx.value_types[pre_id];
                    let phi_id = ctx.new_value(ty, ValueDef::Param(merge_blk));
                    ctx.blocks[merge_blk].params.push((phi_id, ty));
                    ctx.locals.insert(name.clone(), phi_id);
                }

                if !then_terminated {
                    let args: Vec<ValueId> = changed_names
                        .iter()
                        .map(|name| {
                            let pre = pre_if_locals[name];
                            then_locals.get(name).copied().unwrap_or(pre)
                        })
                        .collect();
                    ctx.blocks[then_blk].terminator = MIRTerminator::Jmp {
                        target: merge_blk,
                        args,
                    };
                }

                if !else_terminated {
                    let args: Vec<ValueId> = changed_names
                        .iter()
                        .map(|name| {
                            let pre = pre_if_locals[name];
                            else_locals.get(name).copied().unwrap_or(pre)
                        })
                        .collect();
                    ctx.blocks[else_blk].terminator = MIRTerminator::Jmp {
                        target: merge_blk,
                        args,
                    };
                }
            } else if !then_terminated && else_terminated {
                ctx.locals = then_locals;
            } else if then_terminated && !else_terminated {
                ctx.locals = else_locals;
            } else {
                ctx.locals = pre_if_locals;
            }
        }

        // -----------------------------------------------------------------
        // while (cond) { body }
        //
        // SSA-корректный паттерн через параметры блоков (= phi nodes):
        //
        //   ^current           -> Jmp ^loop_header(live_vars...)
        //   ^loop_header(φ...) -> Br cond, ^loop_body, ^loop_exit
        //   ^loop_body         -> ... -> Jmp ^loop_header(updated_vars...)
        //   ^loop_exit         -> (продолжение, locals = φ из header)
        //
        // Все "живые" locals передаются через параметры header-блока,
        // чтобы uses из тела цикла корректно ссылались на phi-версии.
        // -----------------------------------------------------------------
        Stmt::While { cond, body } => {
            let header_blk = ctx.new_block("loop_header");
            let body_blk = ctx.new_block("loop_body");
            let exit_blk = ctx.new_block("loop_exit");

            ctx.loop_exit_stack.push(exit_blk);
            ctx.loop_header_stack.push(header_blk);

            // Снимок locals до цикла -- в этом порядке создаём параметры header
            let loop_vars: Vec<(String, ValueId)> = ctx
                .locals
                .iter()
                .map(|(name, &id)| (name.clone(), id))
                .collect();

            // Создаём phi-параметры для header-блока
            let mut header_params: Vec<(ValueId, Type)> = Vec::new();
            let mut phi_locals: HashMap<String, ValueId> = HashMap::new();
            for (name, pre_id) in &loop_vars {
                let ty = ctx.ty(*pre_id);
                let param_id = ctx.new_value(ty, ValueDef::Param(header_blk));
                header_params.push((param_id, ty));
                phi_locals.insert(name.clone(), param_id);
            }
            ctx.blocks[header_blk].params = header_params;

            // Переход из текущего блока в header с передачей текущих значений
            let pre_args: Vec<ValueId> = loop_vars.iter().map(|(_, id)| *id).collect();
            ctx.set_term(MIRTerminator::Jmp {
                target: header_blk,
                args: pre_args,
            });

            // Header: locals = phi-параметры, вычисляем условие
            ctx.switch_to(header_blk);
            for (name, &param_id) in &phi_locals {
                ctx.locals.insert(name.clone(), param_id);
            }
            let cond_val = lower_expr(ctx, cond, constants);
            ctx.set_term(MIRTerminator::Br {
                cond: cond_val,
                then_blk: body_blk,
                then_args: Vec::new(),
                else_blk: exit_blk,
                else_args: Vec::new(),
            });

            // Body: переводим операторы тела
            ctx.switch_to(body_blk);
            for s in body {
                lower_stmt(ctx, s, constants);
            }

            // Обратный Jmp в header с обновлёнными значениями
            if !ctx.is_terminated() {
                let back_args: Vec<ValueId> = loop_vars
                    .iter()
                    .map(|(name, _)| {
                        // Если переменная была переназначена в теле -- берём новый ValueId,
                        // иначе -- phi-параметр header (значение "не изменилось")
                        *ctx.locals
                            .get(name.as_str())
                            .unwrap_or_else(|| phi_locals.get(name.as_str()).unwrap())
                    })
                    .collect();
                ctx.set_term(MIRTerminator::Jmp {
                    target: header_blk,
                    args: back_args,
                });
            }

            // После цикла: locals = phi-параметры header (значение на выходе из loop)
            ctx.switch_to(exit_blk);
            for (name, &param_id) in &phi_locals {
                ctx.locals.insert(name.clone(), param_id);
            }

            ctx.loop_exit_stack.pop();
            ctx.loop_header_stack.pop();
        }

        Stmt::Break => {
            let exit = *ctx.loop_exit_stack.last()
                .expect("break should be in loop");
            ctx.set_term(MIRTerminator::Jmp { target: exit, args: vec![] });
        }
        Stmt::Continue => {
            let header = *ctx.loop_header_stack.last()
                .expect("continue shouldn't be there");
            ctx.set_term(MIRTerminator::Jmp { target: header, args: vec![] });
        }

        // -----------------------------------------------------------------
        // return expr, ...
        // -----------------------------------------------------------------
        Stmt::Return(exprs) => {
            let vals: Vec<ValueId> = exprs
                .iter()
                .map(|e| lower_expr(ctx, e, constants))
                .collect();
            ctx.set_term(MIRTerminator::Ret(vals));
        }

        // -----------------------------------------------------------------
        // halt
        // -----------------------------------------------------------------
        Stmt::Halt => {
            ctx.set_term(MIRTerminator::Halt);
        }

        // -----------------------------------------------------------------
        // print(expr, ...) -- вывести значение(я) на консоль
        // Тип инструкции (PrintInt/PrintUInt/PrintFloat/PrintStr) определяется
        // типом выражения во время lowering.
        // -----------------------------------------------------------------
        Stmt::Print(exprs) => {
            for e in exprs {
                let val_id = lower_expr(ctx, e, constants);
                let ty = ctx.ty(val_id);
                let inst = match ty {
                    Type::I64 => MIRInst::PrintInt(val_id),
                    Type::U64 => MIRInst::PrintUInt(val_id),
                    Type::F64 => MIRInst::PrintFloat(val_id),
                    Type::Ptr => MIRInst::PrintStr(val_id),
                    Type::Bool => MIRInst::PrintInt(val_id), // Bool как 0/1
                    Type::Unit => return,                    // ничего не печатаем для Unit
                };
                let _ = ctx.emit(inst); // Игнорируем результат (Unit)
            }
        }

        // -----------------------------------------------------------------
        // expr; -- выражение как оператор, результат игнорируется
        // -----------------------------------------------------------------
        Stmt::Expr(expr) => {
            lower_expr(ctx, expr, constants);
        }
    }
}

// =============================================================================
// ПЕРЕВОД ВЫРАЖЕНИЙ
// =============================================================================

fn lower_expr(ctx: &mut Ctx, expr: &Expr, constants: &mut Vec<MIRConstant>) -> ValueId {
    match expr {
        // -----------------------------------------------------------------
        // Литералы: материализуем через запись в пул констант + MIRInst::Const.
        // Это гарантирует что bytecode lowerer увидит реальные данные.
        // -----------------------------------------------------------------
        Expr::Literal(lit) => {
            let (typ, mir_lit) = match lit {
                Literal::Int(v) => (Type::U64, Literal::Int(*v)),
                Literal::Float(v) => (Type::F64, Literal::Float(*v)),
                Literal::Bool(v) => (Type::Bool, Literal::Bool(*v)),
                Literal::String(s) => (Type::Ptr, Literal::String(s.clone())),
            };
            let name = format!("__lit_{}", constants.len());
            let const_id = constants.len();
            constants.push(MIRConstant {
                name,
                typ,
                value: mir_lit,
            });
            ctx.emit(MIRInst::Const(const_id))
        }

        // -----------------------------------------------------------------
        // Локальная переменная: просто возвращаем текущий SSA ValueId.
        // Никакой инструкции не нужно.
        // -----------------------------------------------------------------
        Expr::Local(name) => *ctx
            .locals
            .get(name.as_str())
            .unwrap_or_else(|| panic!("Undefined local: ${name}")),

        // -----------------------------------------------------------------
        // Глобальная константа: резолвим по имени в пуле.
        // Ищем по имени до мутации -- borrow заканчивается до emit.
        // -----------------------------------------------------------------
        Expr::Const(name) => {
            let const_id = constants
                .iter()
                .position(|c| c.name == *name)
                .unwrap_or_else(|| panic!("Undefined constant: {name}"));
            ctx.emit(MIRInst::Const(const_id))
        }

        // -----------------------------------------------------------------
        // Вызов функции
        // -----------------------------------------------------------------
        Expr::Call { function, args } => {
            let func_id = ctx.resolve_func(function);
            let arg_ids: Vec<ValueId> =
                args.iter().map(|a| lower_expr(ctx, a, constants)).collect();
            ctx.emit(MIRInst::Call {
                func: func_id,
                args: arg_ids,
            })
        }

        // -----------------------------------------------------------------
        // Унарные операции
        // -----------------------------------------------------------------
        Expr::Unary { op, expr } => {
            let val = lower_expr(ctx, expr, constants);
            match op {
                UnaryOp::Neg => match ctx.ty(val) {
                    Type::F64 => ctx.emit(MIRInst::FNeg(val)),
                    _ => ctx.emit(MIRInst::Neg(val)),
                },
                UnaryOp::Not => ctx.emit(MIRInst::Not(val)),
            }
        }

        // -----------------------------------------------------------------
        // Бинарные операции
        // Знаковость арифметики и сравнений определяется по типу левого операнда.
        // -----------------------------------------------------------------
        Expr::Binary { op, lhs, rhs } => {
            let l = lower_expr(ctx, lhs, constants);
            let r = lower_expr(ctx, rhs, constants);
            let lty = ctx.ty(l);

            match op {
                BinaryOp::Add => match lty {
                    Type::F64 => ctx.emit(MIRInst::FAdd(l, r)),
                    _ => ctx.emit(MIRInst::Add(l, r)),
                },
                BinaryOp::Sub => match lty {
                    Type::F64 => ctx.emit(MIRInst::FSub(l, r)),
                    _ => ctx.emit(MIRInst::Sub(l, r)),
                },
                BinaryOp::Mul => match lty {
                    Type::F64 => ctx.emit(MIRInst::FMul(l, r)),
                    _ => ctx.emit(MIRInst::Mul(l, r)),
                },
                BinaryOp::Div => match lty {
                    Type::F64 => ctx.emit(MIRInst::FDiv(l, r)),
                    Type::I64 => ctx.emit(MIRInst::IDiv(l, r)),
                    _ => ctx.emit(MIRInst::UDiv(l, r)),
                },
                BinaryOp::Rem => match lty {
                    Type::F64 => ctx.emit(MIRInst::FRem(l, r)),
                    Type::I64 => ctx.emit(MIRInst::IRem(l, r)),
                    _ => ctx.emit(MIRInst::URem(l, r)),
                },
                BinaryOp::BitAnd => ctx.emit(MIRInst::And(l, r)),
                BinaryOp::BitOr => ctx.emit(MIRInst::Or(l, r)),
                BinaryOp::BitXor => ctx.emit(MIRInst::Xor(l, r)),

                BinaryOp::Shl => ctx.emit(MIRInst::Shl(l, r)),
                BinaryOp::Shr => match lty {
                    Type::I64 => ctx.emit(MIRInst::Sar(l, r)), // Арифметический сдвиг для знаковых
                    _ => ctx.emit(MIRInst::Shr(l, r)),           // Логический для беззнаковых
                },
                // Сравнения: F64 -> FCmp; I64 -> signed ICmp; иначе unsigned ICmp
                BinaryOp::Eq => cmp(ctx, l, r, lty, ICmpOp::Eq, ICmpOp::Eq, FCmpOp::Eq),
                BinaryOp::Ne => cmp(ctx, l, r, lty, ICmpOp::Ne, ICmpOp::Ne, FCmpOp::Ne),
                BinaryOp::Lt => cmp(ctx, l, r, lty, ICmpOp::Slt, ICmpOp::Ult, FCmpOp::Lt),
                BinaryOp::Le => cmp(ctx, l, r, lty, ICmpOp::Sle, ICmpOp::Ule, FCmpOp::Le),
                BinaryOp::Gt => cmp(ctx, l, r, lty, ICmpOp::Sgt, ICmpOp::Ugt, FCmpOp::Gt),
                BinaryOp::Ge => cmp(ctx, l, r, lty, ICmpOp::Sge, ICmpOp::Uge, FCmpOp::Ge),
            }
        }

        // -----------------------------------------------------------------
        // select(cond, then_value, else_value) — полностью реализован
        // -----------------------------------------------------------------
        Expr::Select {
            cond,
            then_value,
            else_value,
        } => {
            let cond_v = lower_expr(ctx, cond, constants);
            let then_v = lower_expr(ctx, then_value, constants);
            let else_v = lower_expr(ctx, else_value, constants);
            ctx.emit(MIRInst::Select {
                cond: cond_v,
                then_v,
                else_v,
            })
        }

        // -----------------------------------------------------------------
        // Явное приведение типа
        // -----------------------------------------------------------------
        Expr::Cast { kind, expr } => {
            let val = lower_expr(ctx, expr, constants);
            let (mir_kind, dest_typ) = lower_cast(kind);
            ctx.emit(MIRInst::Cast {
                kind: mir_kind,
                src: val,
                dest_typ,
            })
        }

        // -----------------------------------------------------------------
        // load.<type>(addr)
        // -----------------------------------------------------------------
        Expr::Load { ty, addr } => {
            let addr_val = lower_expr(ctx, addr, constants);
            ctx.emit(MIRInst::Load {
                addr: addr_val,
                typ: *ty,
            })
        }

        // -----------------------------------------------------------------
        // store.<type>(addr, value)
        // Store не производит SSA-значения — результат типа Unit,
        // DCE уберёт этот ValueId, сохранив сам side-effect.
        // -----------------------------------------------------------------
        Expr::Store { addr, value, .. } => {
            let addr_val = lower_expr(ctx, addr, constants);
            let value_val = lower_expr(ctx, value, constants);
            ctx.emit(MIRInst::Store {
                addr: addr_val,
                value: value_val,
            })
        }
    }
}

// =============================================================================
// УТИЛИТЫ
// =============================================================================

/// Эмитит инструкцию сравнения с правильным знаком по типу операндов.
fn cmp(
    ctx: &mut Ctx,
    l: ValueId,
    r: ValueId,
    lty: Type,
    signed_op: ICmpOp,
    unsigned_op: ICmpOp,
    float_op: FCmpOp,
) -> ValueId {
    match lty {
        Type::F64 => ctx.emit(MIRInst::FCmp {
            op: float_op,
            lhs: l,
            rhs: r,
        }),
        Type::I64 => ctx.emit(MIRInst::ICmp {
            op: signed_op,
            lhs: l,
            rhs: r,
        }),
        _ => ctx.emit(MIRInst::ICmp {
            op: unsigned_op,
            lhs: l,
            rhs: r,
        }),
    }
}

/// Переводит HIR CastKind -> (MIR CastKind, тип результата).
fn lower_cast(kind: &CastKind) -> (MirCastKind, Type) {
    match kind {
        CastKind::I2F => (MirCastKind::I2F, Type::F64),
        CastKind::U2F => (MirCastKind::U2F, Type::F64),
        CastKind::F2I => (MirCastKind::F2I, Type::I64),
        CastKind::F2U => (MirCastKind::F2U, Type::U64),
        CastKind::I2U => (MirCastKind::I2U, Type::U64),
        CastKind::U2I => (MirCastKind::U2I, Type::I64),
        CastKind::TruncU64ToU32 => (MirCastKind::TruncU64ToU32, Type::U64),
        CastKind::ZextU32ToU64 => (MirCastKind::ZextU32ToU64, Type::U64),
        CastKind::SextI32ToI64 => (MirCastKind::SextI32ToI64, Type::I64),
        CastKind::Bitcast => (MirCastKind::Bitcast, Type::U64),
    }
}

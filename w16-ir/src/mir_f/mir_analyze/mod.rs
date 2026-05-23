//! # MIR Анализы
//!
//! Анализы вычисляют информацию об IR которая нужна оптимизатору.
//! Результаты **не хранятся внутри MIRFunction** -- они хранятся здесь
//! в отдельных структурах и инвалидируются при изменении IR.
//!
//! ## Доступные анализы
//!
//! - `DominatorTree` -- дерево доминаторов. Нужно для: проверки SSA use-before-def,
//!   LICM (loop-invariant code motion), обнаружения back edges.
//!
//! - `LivenessInfo` -- liveness (живые значения на входе/выходе блока).
//!   Нужно для: DCE, register allocator (будущий bytecode lowerer).
//!
//! - `CallGraph` -- граф вызовов модуля.
//!   Нужно для: inlining, определения рекурсивных функций, порядка оптимизации.

pub mod induction_vars;
pub mod loop_info;

use std::collections::HashSet;

use crate::mir::{BlockId, FunctionId, MIRFunction, MIRInst, MIRModule, ValueId};

// =============================================================================
// ДЕРЕВО ДОМИНАТОРОВ
// =============================================================================

/// Дерево доминаторов функции.
///
/// Блок A **доминирует** блок B если каждый путь от entry до B проходит через A.
/// Построено алгоритмом простого обратного dataflow (достаточно для большинства функций).
#[derive(Debug, Clone)]
pub struct DominatorTree {
    /// `idom[b]` = непосредственный доминатор блока b.
    /// `idom[0]` = 0 (entry доминирует сам себя).
    /// `idom[b]` = None для недостижимых блоков.
    pub idom: Vec<Option<BlockId>>,
}

impl DominatorTree {
    /// Строит дерево доминаторов для функции алгоритмом Cooper et al. (simple dataflow).
    pub fn build(func: &MIRFunction) -> Self {
        let n = func.blocks.len();
        if n == 0 {
            return Self { idom: Vec::new() };
        }

        // RPO (reverse postorder) обход -- доминаторы считаются в этом порядке
        let rpo = rpo_order(func);
        let rpo_idx: Vec<usize> = {
            let mut idx = vec![usize::MAX; n];
            for (i, &b) in rpo.iter().enumerate() {
                idx[b] = i;
            }
            idx
        };

        let mut idom: Vec<Option<BlockId>> = vec![None; n];
        idom[0] = Some(0); // entry доминирует сам себя

        let mut changed = true;
        while changed {
            changed = false;
            for &b in rpo.iter().skip(1) {
                // пропускаем entry
                let block = &func.blocks[b];
                // Новый idom = пересечение idom-ов всех обработанных predecessors
                let mut new_idom: Option<BlockId> = None;
                for &pred in &block.predecessors {
                    if idom[pred].is_none() {
                        continue; // predecessor ещё не обработан
                    }
                    new_idom = Some(match new_idom {
                        None => pred,
                        Some(d) => intersect(d, pred, &idom, &rpo_idx),
                    });
                }
                if new_idom != idom[b] {
                    idom[b] = new_idom;
                    changed = true;
                }
            }
        }

        Self { idom }
    }

    /// Проверяет доминирование: A доминирует B?
    pub fn dominates(&self, a: BlockId, b: BlockId) -> bool {
        let mut cur = b;
        loop {
            if cur == a {
                return true;
            }
            match self.idom[cur] {
                Some(parent) if parent != cur => cur = parent,
                _ => return false,
            }
        }
    }

    /// Проверяет строгое доминирование: A строго доминирует B (A != B)?
    pub fn strictly_dominates(&self, a: BlockId, b: BlockId) -> bool {
        a != b && self.dominates(a, b)
    }
}

/// Алгоритм пересечения двух доминаторов (Cooper et al.).
fn intersect(
    mut a: BlockId,
    mut b: BlockId,
    idom: &[Option<BlockId>],
    rpo_idx: &[usize],
) -> BlockId {
    while a != b {
        while rpo_idx[a] > rpo_idx[b] {
            a = idom[a].unwrap();
        }
        while rpo_idx[b] > rpo_idx[a] {
            b = idom[b].unwrap();
        }
    }
    a
}

/// Reverse postorder обход CFG от entry.
fn rpo_order(func: &MIRFunction) -> Vec<BlockId> {
    let n = func.blocks.len();
    let mut visited = vec![false; n];
    let mut postorder = Vec::with_capacity(n);
    dfs_postorder(func, 0, &mut visited, &mut postorder);
    postorder.reverse();
    postorder
}

fn dfs_postorder(
    func: &MIRFunction,
    block: BlockId,
    visited: &mut Vec<bool>,
    postorder: &mut Vec<BlockId>,
) {
    if visited[block] {
        return;
    }
    visited[block] = true;
    for succ in func.blocks[block].terminator.successors() {
        dfs_postorder(func, succ, visited, postorder);
    }
    postorder.push(block);
}

// =============================================================================
// LIVENESS ANALYSIS
// =============================================================================

/// Liveness информация для всех блоков функции.
///
/// Значение **живо** на входе блока если существует путь от этого блока
/// до использования значения, не проходящий через его определение.
#[derive(Debug, Clone)]
pub struct LivenessInfo {
    /// `live_in[b]` = множество ValueId живых на **входе** блока b.
    pub live_in: Vec<HashSet<ValueId>>,
    /// `live_out[b]` = множество ValueId живых на **выходе** блока b.
    pub live_out: Vec<HashSet<ValueId>>,
}

impl LivenessInfo {
    /// Вычисляет liveness через обратный dataflow (backward analysis).
    ///
    /// Уравнения:
    /// ```text
    /// live_out[b] = U live_in[s] для всех successors s
    /// live_in[b] = use[b] U (live_out[b] \ def[b])
    /// ```
    pub fn compute(func: &MIRFunction) -> Self {
        let n = func.blocks.len();
        let mut live_in: Vec<HashSet<ValueId>> = vec![HashSet::new(); n];
        let mut live_out: Vec<HashSet<ValueId>> = vec![HashSet::new(); n];

        // Вычисляем use и def для каждого блока
        let (uses, defs) = compute_use_def(func);

        // Итерируем до фиксированной точки (backward)
        let mut changed = true;
        while changed {
            changed = false;
            // Обходим в обратном порядке для быстрой сходимости
            for b in (0..n).rev() {
                // live_out[b] = ∪ live_in[succ]
                let new_live_out: HashSet<ValueId> = func.blocks[b]
                    .terminator
                    .successors()
                    .iter()
                    .flat_map(|&s| live_in[s].iter().copied())
                    .collect();

                // live_in[b] = use[b] ∪ (live_out[b] \ def[b])
                let new_live_in: HashSet<ValueId> = uses[b]
                    .iter()
                    .copied()
                    .chain(
                        new_live_out
                            .iter()
                            .copied()
                            .filter(|v| !defs[b].contains(v)),
                    )
                    .collect();

                if new_live_out != live_out[b] || new_live_in != live_in[b] {
                    live_out[b] = new_live_out;
                    live_in[b] = new_live_in;
                    changed = true;
                }
            }
        }

        Self { live_in, live_out }
    }

    /// Значение живо на входе блока?
    pub fn is_live_in(&self, block: BlockId, val: ValueId) -> bool {
        self.live_in[block].contains(&val)
    }

    /// Значение живо на выходе блока?
    pub fn is_live_out(&self, block: BlockId, val: ValueId) -> bool {
        self.live_out[block].contains(&val)
    }
}

/// Вычисляет use/def множества для каждого блока.
/// use[b] = значения используемые в b до их определения в b.
/// def[b] = значения определяемые в b.
fn compute_use_def(func: &MIRFunction) -> (Vec<HashSet<ValueId>>, Vec<HashSet<ValueId>>) {
    let n = func.blocks.len();
    let mut uses: Vec<HashSet<ValueId>> = vec![HashSet::new(); n];
    let mut defs: Vec<HashSet<ValueId>> = vec![HashSet::new(); n];

    for (b, block) in func.blocks.iter().enumerate() {
        // Параметры блока -- определения
        for (param_id, _) in &block.params {
            defs[b].insert(*param_id);
        }

        for (inst_idx, inst) in block.instructions.iter().enumerate() {
            // Операнды инструкции -- uses (если ещё не определены в этом блоке)
            for operand in inst_operands(inst) {
                if !defs[b].contains(&operand) {
                    uses[b].insert(operand);
                }
            }
            // Результат инструкции -- def
            if let Some(result) = func
                .values
                .iter()
                .position(|v| v.def == crate::mir::ValueDef::Inst(b, inst_idx))
            {
                defs[b].insert(result);
            }
        }

        // Операнды terminator-а -- uses
        for operand in block.terminator.operands() {
            if !defs[b].contains(&operand) {
                uses[b].insert(operand);
            }
        }
    }

    (uses, defs)
}

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
// ГРАФ ВЫЗОВОВ
// =============================================================================

/// Граф вызовов всего модуля.
#[derive(Debug, Clone)]
pub struct CallGraph {
    /// `callees[f]` = все функции которые f вызывает (прямые вызовы).
    pub callees: Vec<HashSet<FunctionId>>,
    /// `callers[f]` = все функции которые вызывают f.
    pub callers: Vec<HashSet<FunctionId>>,
    /// Рекурсивные функции (прямая или косвенная рекурсия).
    pub recursive: HashSet<FunctionId>,
}

impl CallGraph {
    /// Строит граф вызовов для всего модуля.
    pub fn build(module: &MIRModule) -> Self {
        let n = module.functions.len();
        let mut callees: Vec<HashSet<FunctionId>> = vec![HashSet::new(); n];
        let mut callers: Vec<HashSet<FunctionId>> = vec![HashSet::new(); n];

        for (func_id, func) in module.functions.iter().enumerate() {
            for block in &func.blocks {
                for inst in &block.instructions {
                    if let MIRInst::Call {
                        func: callee_id, ..
                    } = inst
                    {
                        callees[func_id].insert(*callee_id);
                        callers[*callee_id].insert(func_id);
                    }
                }
            }
        }

        // Находим рекурсивные функции через SCC (Tarjan)
        let recursive = find_recursive(&callees, n);

        Self {
            callees,
            callers,
            recursive,
        }
    }

    /// Функция рекурсивна?
    pub fn is_recursive(&self, func: FunctionId) -> bool {
        self.recursive.contains(&func)
    }

    /// Количество call-sites функции в модуле.
    pub fn call_site_count(&self, func: FunctionId) -> usize {
        self.callers[func].len()
    }
}

/// Находит все рекурсивные функции через поиск циклов в графе вызовов (DFS).
fn find_recursive(callees: &[HashSet<FunctionId>], n: usize) -> HashSet<FunctionId> {
    let mut recursive = HashSet::new();
    let mut visited = vec![false; n];
    let mut on_stack = vec![false; n];

    fn dfs(
        node: FunctionId,
        callees: &[HashSet<FunctionId>],
        visited: &mut Vec<bool>,
        on_stack: &mut Vec<bool>,
        recursive: &mut HashSet<FunctionId>,
    ) {
        visited[node] = true;
        on_stack[node] = true;
        for &callee in &callees[node] {
            if callee >= callees.len() {
                continue;
            }
            if !visited[callee] {
                dfs(callee, callees, visited, on_stack, recursive);
            } else if on_stack[callee] {
                // Цикл найден -- все функции на стеке рекурсивны
                recursive.insert(callee);
                recursive.insert(node);
            }
        }
        on_stack[node] = false;
    }

    for i in 0..n {
        if !visited[i] {
            dfs(i, callees, &mut visited, &mut on_stack, &mut recursive);
        }
    }

    recursive
}

// =============================================================================
// ЗАПОЛНЕНИЕ FunctionAnalysis
// =============================================================================

/// Вычисляет и заполняет `FunctionAnalysis` для всех функций модуля.
/// Вызывается оптимизатором перед началом pass-ов.
pub fn compute_all_analysis(module: &mut MIRModule) {
    let call_graph = CallGraph::build(module);
    let n = module.functions.len();

    // Считаем call_site_count для каждой функции
    // (число уникальных call-site блоков, не уникальных caller-функций)
    let mut call_site_counts = vec![0usize; n];
    for func in &module.functions {
        for block in &func.blocks {
            for inst in &block.instructions {
                if let MIRInst::Call {
                    func: callee_id, ..
                } = inst
                {
                    if *callee_id < n {
                        call_site_counts[*callee_id] += 1;
                    }
                }
            }
        }
    }

    for func_id in 0..n {
        let is_recursive = call_graph.is_recursive(func_id);
        let call_site_count = call_site_counts[func_id];
        let is_reachable = func_id == 0 || call_site_count > 0;
        let is_pure = compute_is_pure(&module.functions[func_id]);
        let inst_count = module.functions[func_id]
            .blocks
            .iter()
            .map(|b| b.instructions.len())
            .sum::<usize>();
        // Порог inlining: <= 32 инструкций, не рекурсивна
        let is_inlinable = !is_recursive && inst_count <= 32;

        module.analysis[func_id] = crate::mir::FunctionAnalysis {
            is_pure,
            is_inlinable,
            is_recursive,
            is_reachable,
            call_site_count,
            is_valid: true,
        };
    }
}

/// Функция чистая если у неё нет Store и нет Call других функций.
fn compute_is_pure(func: &MIRFunction) -> bool {
    for block in &func.blocks {
        for inst in &block.instructions {
            match inst {
                MIRInst::Store { .. } => return false,
                MIRInst::Call { .. } => return false,
                _ => {}
            }
        }
    }
    true
}

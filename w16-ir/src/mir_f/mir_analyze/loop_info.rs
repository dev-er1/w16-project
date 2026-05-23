// w16-ir\src\mir_analyze\loop_info.rs
//
//! # Анализ структуры циклов
//!
//! Находит все естественные циклы в CFG функции через back edges.
//!
//! ## Определения
//!
//! **Back edge**: ребро (tail -> header) где header доминирует tail.
//! Это признак цикла в SSA CFG.
//!
//! **Естественный цикл** с header H и back edge (T -> H):
//! - Header = H (единственная точка входа в цикл извне)
//! - Body = все блоки от которых есть путь до T не проходящий через H, плюс H
//! - Preheader = блок который создаётся (или уже существует) перед H,
//!   единственный predecessor H извне цикла. Нужен для LICM.
//! - Exit blocks = блоки вне цикла куда ведут рёбра из тела цикла.
//!
//! ## Ограничения текущей реализации
//!
//! - Вложенные циклы поддерживаются (каждый back edge = отдельный Loop).
//! - Irreducible CFG (несколько входов в цикл) не поддерживается --
//!   lowerer такие не генерирует, поэтому это нормально.

use std::collections::HashSet;

use super::DominatorTree;
use crate::mir::{BlockId, MIRFunction};

// =============================================================================
// СТРУКТУРЫ
// =============================================================================

/// Одно back edge в CFG.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackEdge {
    /// Хвост (источник): блок у которого terminator указывает на header.
    pub tail: BlockId,
    /// Голова (цель): блок который доминирует tail.
    pub header: BlockId,
}

/// Описание одного естественного цикла.
#[derive(Debug, Clone)]
pub struct Loop {
    /// Back edge от которого построен цикл.
    pub back_edge: BackEdge,

    /// Header блок. Единственная точка входа извне.
    /// В нашем lowerer-е это `^loop_header` из `Stmt::While`.
    pub header: BlockId,

    /// Все блоки принадлежащие телу цикла (включая header).
    /// Порядок не определён.
    pub body: HashSet<BlockId>,

    /// Блоки вне цикла куда ведут рёбра из body.
    /// Обычно один (`^loop_exit`).
    pub exits: Vec<BlockId>,
}

impl Loop {
    /// Принадлежит ли блок телу цикла?
    #[inline]
    pub fn contains(&self, block: BlockId) -> bool {
        self.body.contains(&block)
    }

    /// Количество блоков в теле цикла (включая header).
    #[inline]
    pub fn size(&self) -> usize {
        self.body.len()
    }
}

/// Результат анализа циклов для всей функции.
#[derive(Debug, Clone)]
pub struct LoopInfo {
    /// Все найденные естественные циклы.
    pub loops: Vec<Loop>,
}

impl LoopInfo {
    /// Строит LoopInfo для функции.
    /// Требует уже построенный DominatorTree.
    pub fn build(func: &MIRFunction, domtree: &DominatorTree) -> Self {
        let back_edges = find_back_edges(func, domtree);
        let loops = back_edges
            .into_iter()
            .map(|be| build_loop(func, be))
            .collect();
        Self { loops }
    }

    /// Возвращает самый внутренний цикл содержащий данный блок, если есть.
    /// "Самый внутренний" = с наименьшим body.size().
    pub fn innermost_loop(&self, block: BlockId) -> Option<&Loop> {
        self.loops
            .iter()
            .filter(|l| l.contains(block))
            .min_by_key(|l| l.size())
    }

    /// Все циклы чей header -- это данный блок.
    pub fn loops_with_header(&self, header: BlockId) -> impl Iterator<Item = &Loop> {
        self.loops.iter().filter(move |l| l.header == header)
    }
}

// =============================================================================
// ПОСТРОЕНИЕ
// =============================================================================

/// Находит все back edges в CFG.
/// Back edge (tail -> header) если header доминирует tail.
fn find_back_edges(func: &MIRFunction, domtree: &DominatorTree) -> Vec<BackEdge> {
    let mut back_edges = Vec::new();

    for block in func.live_blocks() {
        for succ in block.terminator.successors() {
            // Если successor доминирует текущий блок -> это back edge
            if domtree.dominates(succ, block.id) {
                back_edges.push(BackEdge {
                    tail: block.id,
                    header: succ,
                });
            }
        }
    }

    back_edges
}

/// Строит Loop для одного back edge через обратный DFS от tail до header.
///
/// Алгоритм: body = {header} ∪ {все блоки с которых можно дойти до tail
/// идя по CFG назад, не выходя за header}.
fn build_loop(func: &MIRFunction, back_edge: BackEdge) -> Loop {
    let mut body: HashSet<BlockId> = HashSet::new();
    body.insert(back_edge.header);

    // Обратный DFS от tail: идём по predecessors
    let mut stack = vec![back_edge.tail];
    while let Some(b) = stack.pop() {
        if body.contains(&b) {
            continue;
        }
        body.insert(b);
        // Добавляем predecessors этого блока в стек
        for &pred in &func.blocks[b].predecessors {
            if !body.contains(&pred) {
                stack.push(pred);
            }
        }
    }

    // Находим exit blocks: successor вне body
    let mut exits: Vec<BlockId> = Vec::new();
    for &b in &body {
        for succ in func.blocks[b].terminator.successors() {
            if !body.contains(&succ) && !exits.contains(&succ) {
                exits.push(succ);
            }
        }
    }

    Loop {
        back_edge,
        header: back_edge.header,
        body,
        exits,
    }
}

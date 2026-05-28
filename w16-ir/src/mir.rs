// w16-ir\src\mir.rs
//
//! # W16 MIR — типизированный SSA IR
//!
//! MIR это последнее представление кода перед компиляцией в байт-код.
//! Он ближе к машинному коду чем HIR, но всё ещё читаем человеком.
//!
//! ## Архитектура middle-end (что идёт после lowerer-а)
//!
//! ```text
//! HIR
//!  └─ lowerer.rs         HIR -> MIR
//!      └─ mir_verify     Проверка SSA-корректности, типов, CFG-структуры.
//!          │             Запускается после lowerer-а и после каждого pass-а оптимизатора.
//!          │             Если verify говорит "ок" — бэкенд доверяет IR полностью.
//!          └─ mir_analysis   Liveness, dominators, use-def chains.
//!              │             Результаты хранятся в FunctionAnalysis (не в MIRFunction).
//!              │             Инвалидируются при любом изменении IR.
//!              └─ mir_optimize   Pass-ы оптимизаций.
//!                  │             DCE, Constant Folding, Inlining.
//!                  │             Каждый pass после себя запускает verify (в debug-режиме).
//!                  └─ bytecode_lowerer   MIR -> W16 bytecode
//! ```
//!
//! ## Почему анализы не хранятся в MIRFunction
//!
//! Флаги вроде `is_can_inline` или `is_used` — это результаты анализа, а не
//! свойства IR. После любого изменения IR флаги становятся stale. Если хранить
//! их внутри `MIRFunction`, нельзя знать валидны ли они сейчас.
//! Решение: анализы живут в `FunctionAnalysis` рядом с функцией, но отдельно.
//! Оптимизатор инвалидирует `FunctionAnalysis` при изменении IR функции.
//!
//! ## Инварианты SSA которые гарантирует lowerer и проверяет verifier
//!
//! - `blocks[i].id == i` всегда (BlockId = индекс в Vec).
//! - Каждый ValueId определён ровно один раз.
//! - Все использования ValueId доминируются его определением.
//! - Каждый блок заканчивается ровно одним terminator.
//! - `predecessors[b]` содержит все блоки у которых terminator ссылается на `b`.
//! - Аргументы Jmp/Br соответствуют по количеству и типу параметрам целевого блока.
//!
//! ## Стратегия удаления блоков и значений
//!
//! Физическое удаление из Vec ломает инвариант `blocks[i].id == i`.
//! Поэтому мёртвые блоки и значения **помечаются флагом**, а не удаляются.
//! Финальный `compact()` перенумеровывает IR перед передачей в bytecode lowerer.

pub use crate::hir::{Literal, Type};

// =============================================================================
// БАЗОВЫЕ ТИПЫ-АЛИАСЫ
// =============================================================================

/// ID SSA-значения (%1, %2, ...). Индекс в `MIRFunction::values`.
pub type ValueId = usize;

/// ID базового блока (^entry, ^loop, ...). Индекс в `MIRFunction::blocks`.
/// Инвариант: `blocks[id].id == id` всегда.
pub type BlockId = usize;

/// ID функции в модуле. Индекс в `MIRModule::functions`.
pub type FunctionId = usize;

/// ID константы в модуле. Индекс в `MIRModule::constants`.
pub type ConstantId = usize;

// =============================================================================
// SSA: ОПРЕДЕЛЕНИЯ И ИСПОЛЬЗОВАНИЯ ЗНАЧЕНИЙ
// =============================================================================

/// Где определено SSA-значение.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueDef {
    /// Результат инструкции: (блок, индекс инструкции в блоке).
    Inst(BlockId, usize),
    /// Параметр блока (phi-node): блок которому принадлежит.
    Param(BlockId),
    /// Прямая ссылка на глобальную константу модуля.
    Constant(ConstantId),
    /// Параметр функции (аргумент извне).
    External,
}

/// Полные метаданные SSA-значения.
/// Позволяет за O(1) найти определение и все точки использования.
#[derive(Debug, Clone)]
pub struct ValueData {
    /// Тип значения.
    pub typ: Type,
    /// Где значение определено.
    pub def: ValueDef,
    /// ValueId всех инструкций/терминаторов которые используют это значение.
    /// Тип `ValueId` (а не `usize`) — потому что каждое использование само
    /// является результатом какой-то инструкции. Use-list нужен для:
    /// - DCE: если uses пуст и нет side-effect -> значение мёртвое.
    /// - `replace_all_uses_with`: итерируемся по uses и патчим операнды.
    pub uses: Vec<ValueId>,
    /// Значение помечено мёртвым (DCE). Физически не удалено — инвариант индексов.
    pub is_dead: bool,
}

impl ValueData {
    pub fn new(typ: Type, def: ValueDef) -> Self {
        Self {
            typ,
            def,
            uses: Vec::new(),
            is_dead: false,
        }
    }
}

// =============================================================================
// ИНСТРУКЦИИ
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ICmpOp {
    Eq,
    Ne,
    Slt,
    Sle,
    Sgt,
    Sge, // знаковые
    Ult,
    Ule,
    Ugt,
    Uge, // беззнаковые
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FCmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastKind {
    I2F,
    U2F,
    F2I,
    F2U,
    I2U,
    U2I,
    TruncU64ToU32,
    ZextU32ToU64,
    SextI32ToI64,
    Bitcast,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MIRInst {
    // --- Целочисленная арифметика ---
    Add(ValueId, ValueId),
    Sub(ValueId, ValueId),
    Mul(ValueId, ValueId),
    UDiv(ValueId, ValueId),
    IDiv(ValueId, ValueId),
    URem(ValueId, ValueId),
    IRem(ValueId, ValueId),
    Neg(ValueId),

    // --- Плавающая арифметика ---
    FAdd(ValueId, ValueId),
    FSub(ValueId, ValueId),
    FMul(ValueId, ValueId),
    FDiv(ValueId, ValueId),
    FRem(ValueId, ValueId),
    FNeg(ValueId),
    FAbs(ValueId),

    // --- Битовые операции ---
    And(ValueId, ValueId),
    Or(ValueId, ValueId),
    Xor(ValueId, ValueId),
    Not(ValueId),
    Shl(ValueId, ValueId),
    Shr(ValueId, ValueId), // логический сдвиг вправо
    Sar(ValueId, ValueId), // арифметический сдвиг вправо

    // --- Сравнения ---
    ICmp {
        op: ICmpOp,
        lhs: ValueId,
        rhs: ValueId,
    },
    FCmp {
        op: FCmpOp,
        lhs: ValueId,
        rhs: ValueId,
    },

    // --- Данные и память ---
    /// Загрузка глобальной константы по ConstantId.
    Const(ConstantId),
    /// Копирование SSA-значения (используется при inlining и φ-разрешении).
    Mov(ValueId),
    Load {
        addr: ValueId,
        typ: Type,
    },
    Store {
        addr: ValueId,
        value: ValueId,
    },
    Cast {
        kind: CastKind,
        src: ValueId,
        dest_typ: Type,
    },
    Select {
        cond: ValueId,
        then_v: ValueId,
        else_v: ValueId,
    },
    Call {
        func: FunctionId,
        args: Vec<ValueId>,
    },

    // --- Вывод на консоль (side-effects) ---
    PrintInt(ValueId),   // Вывести как signed i64
    PrintUInt(ValueId),  // Вывести как unsigned u64
    PrintFloat(ValueId), // Вывести как f64
    PrintStr(ValueId),   // Вывести как string (адрес из пула)
}

// =============================================================================
// TERMINATORS
// =============================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum MIRTerminator {
    /// Безусловный переход. `args` передаются в параметры блока `target`.
    Jmp { target: BlockId, args: Vec<ValueId> },
    /// Условный переход.
    Br {
        cond: ValueId,
        then_blk: BlockId,
        then_args: Vec<ValueId>,
        else_blk: BlockId,
        else_args: Vec<ValueId>,
    },
    /// Возврат из функции. Пустой Vec = void return.
    Ret(Vec<ValueId>),
    /// Остановка runtime.
    Halt,
}

impl MIRTerminator {
    /// Все ValueId которые terminator использует как операнды.
    /// Нужно для use-tracking и verifier-а.
    pub fn operands(&self) -> Vec<ValueId> {
        match self {
            MIRTerminator::Jmp { args, .. } => args.clone(),
            MIRTerminator::Br {
                cond,
                then_args,
                else_args,
                ..
            } => {
                let mut v = vec![*cond];
                v.extend_from_slice(then_args);
                v.extend_from_slice(else_args);
                v
            }
            MIRTerminator::Ret(vals) => vals.clone(),
            MIRTerminator::Halt => vec![],
        }
    }

    /// Все блоки-преемники этого terminator-а.
    /// Нужно для построения CFG и predecessors.
    pub fn successors(&self) -> Vec<BlockId> {
        match self {
            MIRTerminator::Jmp { target, .. } => vec![*target],
            MIRTerminator::Br {
                then_blk, else_blk, ..
            } => vec![*then_blk, *else_blk],
            MIRTerminator::Ret(_) | MIRTerminator::Halt => vec![],
        }
    }
}

// =============================================================================
// БАЗОВЫЙ БЛОК
// =============================================================================

#[derive(Debug, Clone)]
pub struct BasicBlock {
    /// Уникальный ID блока. Инвариант: `blocks[id].id == id`.
    pub id: BlockId,
    /// Имя для отладки и дампа ("entry", "loop_header", ...).
    pub name: String,
    /// Параметры блока (= phi-nodes). `(ValueId результата, тип)`.
    pub params: Vec<(ValueId, Type)>,
    /// Линейные инструкции. Результат каждой — отдельный ValueId.
    pub instructions: Vec<MIRInst>,
    /// Блоки чьи terminators ссылаются на этот блок.
    pub predecessors: Vec<BlockId>,
    /// Завершающая инструкция блока.
    pub terminator: MIRTerminator,
    /// Блок помечен мёртвым (недостижим из entry). Не удаляется физически.
    pub is_dead: bool,
}

impl BasicBlock {
    pub fn new(id: BlockId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            params: Vec::new(),
            instructions: Vec::new(),
            predecessors: Vec::new(),
            terminator: MIRTerminator::Halt,
            is_dead: false,
        }
    }

    /// Все преемники этого блока (делегируем terminator-у).
    pub fn successors(&self) -> Vec<BlockId> {
        self.terminator.successors()
    }
}

// =============================================================================
// ФУНКЦИЯ
// =============================================================================

#[derive(Debug, Clone)]
pub struct MIRFunction {
    /// Уникальный ID функции в модуле. Индекс в `MIRModule::functions`.
    pub id: FunctionId,
    pub name: String,
    /// Типы параметров функции (в порядке объявления).
    pub param_types: Vec<Type>,
    /// Типы возвращаемых значений.
    pub return_types: Vec<Type>,
    /// Все блоки функции. Индекс == BlockId (инвариант).
    pub blocks: Vec<BasicBlock>,
    /// Тип каждого SSA-значения. `value_types[id]` == тип значения `id`.
    pub value_types: Vec<Type>,
    /// Полные метаданные каждого SSA-значения.
    pub values: Vec<ValueData>,
}

impl MIRFunction {
    /// Получить блок по BlockId за O(1).
    #[inline(always)]
    pub fn block(&self, id: BlockId) -> &BasicBlock {
        &self.blocks[id]
    }

    #[inline(always)]
    pub fn block_mut(&mut self, id: BlockId) -> &mut BasicBlock {
        &mut self.blocks[id]
    }

    /// Тип SSA-значения по ValueId за O(1).
    #[inline(always)]
    pub fn value_type(&self, id: ValueId) -> Type {
        self.value_types[id]
    }

    /// Метаданные SSA-значения по ValueId за O(1).
    #[inline(always)]
    pub fn value(&self, id: ValueId) -> &ValueData {
        &self.values[id]
    }

    #[inline(always)]
    pub fn value_mut(&mut self, id: ValueId) -> &mut ValueData {
        &mut self.values[id]
    }

    /// Entry block всегда blocks[0].
    #[inline(always)]
    pub fn entry(&self) -> &BasicBlock {
        &self.blocks[0]
    }

    /// Итерация по живым блокам (не помеченным is_dead).
    pub fn live_blocks(&self) -> impl Iterator<Item = &BasicBlock> {
        self.blocks.iter().filter(|b| !b.is_dead)
    }

    /// Итерация по живым значениям.
    pub fn live_values(&self) -> impl Iterator<Item = (ValueId, &ValueData)> {
        self.values.iter().enumerate().filter(|(_, v)| !v.is_dead)
    }

    /// Заменяет все использования `old` на `new` во всех инструкциях и терминаторах.
    /// Нужно для Constant Folding и Copy Propagation.
    /// После вызова use-list `old` становится пустым, use-list `new` — обновлённым.
    pub fn replace_all_uses_with(&mut self, old: ValueId, new: ValueId) {
        // Патчим все инструкции во всех блоках
        for block in &mut self.blocks {
            for inst in &mut block.instructions {
                replace_in_inst(inst, old, new);
            }
            replace_in_terminator(&mut block.terminator, old, new);
        }
        // Обновляем use-lists
        let old_uses = std::mem::take(&mut self.values[old].uses);
        for user_id in &old_uses {
            self.values[new].uses.push(*user_id);
        }
        // old теперь не используется — его uses пусты
    }
}

/// Заменяет `old` на `new` внутри одной инструкции.
fn replace_in_inst(inst: &mut MIRInst, old: ValueId, new: ValueId) {
    let patch = |v: &mut ValueId| {
        if *v == old {
            *v = new;
        }
    };
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
        | MIRInst::Sar(a, b) => {
            patch(a);
            patch(b);
        }
        MIRInst::Neg(a)
        | MIRInst::FNeg(a)
        | MIRInst::FAbs(a)
        | MIRInst::Not(a)
        | MIRInst::Mov(a) => {
            patch(a);
        }
        MIRInst::ICmp { lhs, rhs, .. } | MIRInst::FCmp { lhs, rhs, .. } => {
            patch(lhs);
            patch(rhs);
        }
        MIRInst::Cast { src, .. } => patch(src),
        MIRInst::Load { addr, .. } => patch(addr),
        MIRInst::Store { addr, value } => {
            patch(addr);
            patch(value);
        }
        MIRInst::Select {
            cond,
            then_v,
            else_v,
        } => {
            patch(cond);
            patch(then_v);
            patch(else_v);
        }
        MIRInst::Call { args, .. } => args.iter_mut().for_each(patch),
        MIRInst::Const(_) => {}
        MIRInst::PrintInt(a)
        | MIRInst::PrintUInt(a)
        | MIRInst::PrintFloat(a)
        | MIRInst::PrintStr(a) => {
            patch(a);
        }
    }
}

/// Заменяет `old` на `new` внутри terminator-а.
fn replace_in_terminator(term: &mut MIRTerminator, old: ValueId, new: ValueId) {
    let patch = |v: &mut ValueId| {
        if *v == old {
            *v = new;
        }
    };
    match term {
        MIRTerminator::Jmp { args, .. } => args.iter_mut().for_each(patch),
        MIRTerminator::Br {
            cond,
            then_args,
            else_args,
            ..
        } => {
            patch(cond);
            then_args.iter_mut().for_each(&patch);
            else_args.iter_mut().for_each(&patch);
        }
        MIRTerminator::Ret(vals) => vals.iter_mut().for_each(patch),
        MIRTerminator::Halt => {}
    }
}

// =============================================================================
// АНАЛИЗ ФУНКЦИИ (результаты — отдельно от IR)
// =============================================================================

/// Результаты анализа одной функции.
/// Хранится рядом с `MIRFunction`, но не внутри неё — чтобы было ясно
/// когда данные актуальны, а когда инвалидированы изменением IR.
///
/// Оптимизатор инвалидирует `FunctionAnalysis` при любом изменении IR функции,
/// и пересчитывает перед следующим pass-ом который требует анализа.
#[derive(Debug, Clone, Default)]
pub struct FunctionAnalysis {
    /// Функция не имеет побочных эффектов (нет Store, нет Call с side-effects).
    /// Нужно: Constant Folding, Dead Code Elimination.
    pub is_pure: bool,

    /// Функция достаточно мала для inlining (размер в инструкциях <= порога).
    /// Нужно: Inliner.
    pub is_inlinable: bool,

    /// Функция рекурсивна (прямо или через цикл вызовов).
    /// Если true — inliner не трогает её без явного `#[inline]`.
    pub is_recursive: bool,

    /// Функция вызывается хотя бы один раз в модуле.
    /// Если false — DCE может удалить её целиком.
    pub is_reachable: bool,

    /// Количество call-sites этой функции в модуле.
    /// Если == 1 — inlining почти всегда выгоден (размер кода не растёт).
    pub call_site_count: usize,

    /// Данные актуальны (не инвалидированы после изменения IR).
    pub is_valid: bool,
}

impl FunctionAnalysis {
    /// Инвалидирует анализ. Вызывается оптимизатором при изменении IR.
    pub fn invalidate(&mut self) {
        *self = Self::default();
    }
}

// =============================================================================
// КОНСТАНТА
// =============================================================================

#[derive(Debug, Clone, PartialEq)]
pub struct MIRConstant {
    /// Имя константы (из HIR или `__lit_N` для анонимных литералов).
    pub name: String,
    pub typ: Type,
    pub value: Literal,
}

// =============================================================================
// МОДУЛЬ
// =============================================================================

#[derive(Debug, Clone)]
pub struct MIRModule {
    pub name: String,
    pub constants: Vec<MIRConstant>,
    pub functions: Vec<MIRFunction>,
    /// Результаты анализа для каждой функции. Индекс == FunctionId.
    /// Живут рядом с функциями, но отдельно — чтобы IR оставался чистым.
    pub analysis: Vec<FunctionAnalysis>,
}

impl MIRModule {
    pub fn new(name: String) -> Self {
        Self {
            name,
            constants: Vec::new(),
            functions: Vec::new(),
            analysis: Vec::new(),
        }
    }

    // -------------------------------------------------------------------------
    // Функции
    // -------------------------------------------------------------------------

    #[inline(always)]
    pub fn get_function(&self, id: FunctionId) -> &MIRFunction {
        &self.functions[id]
    }

    #[inline(always)]
    pub fn get_function_mut(&mut self, id: FunctionId) -> &mut MIRFunction {
        &mut self.functions[id]
    }

    pub fn find_function_id(&self, name: &str) -> Option<FunctionId> {
        self.functions.iter().position(|f| f.name == name)
    }

    /// Добавляет функцию и создаёт для неё пустой FunctionAnalysis.
    pub fn add_function(&mut self, func: MIRFunction) -> FunctionId {
        let id = self.functions.len();
        self.functions.push(func);
        self.analysis.push(FunctionAnalysis::default());
        id
    }

    /// Инвалидирует анализ функции после изменения её IR.
    pub fn invalidate_analysis(&mut self, id: FunctionId) {
        self.analysis[id].invalidate();
    }

    // -------------------------------------------------------------------------
    // Константы
    // -------------------------------------------------------------------------

    #[inline(always)]
    pub fn get_constant(&self, id: ConstantId) -> &MIRConstant {
        &self.constants[id]
    }

    pub fn find_constant_id(&self, name: &str) -> Option<ConstantId> {
        self.constants.iter().position(|c| c.name == name)
    }

    /// Добавляет константу или возвращает ID уже существующей (дедупликация по значению).
    /// Constant Folding использует дедупликацию чтобы не плодить дубли в пуле.
    pub fn intern_constant(&mut self, name: String, typ: Type, value: Literal) -> ConstantId {
        // Ищем по значению (не по имени) — анонимные литералы могут иметь разные имена
        if let Some(id) = self
            .constants
            .iter()
            .position(|c| c.typ == typ && c.value == value)
        {
            return id;
        }
        let id = self.constants.len();
        self.constants.push(MIRConstant { name, typ, value });
        id
    }
}

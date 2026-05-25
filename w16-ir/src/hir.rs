// w16-ir\src\hir.rs
//
//! Типизированное HIR-представление W16.
//!
//! HIR близок к исходному языку: здесь ещё есть лексические переменные,
//! структурные `if`/`while`, выражения и объявления функций. Этот слой удобен
//! для semantic verifier и будущих высокоуровневых оптимизаций. Позже HIR будет
//! понижаться в MIR, где control flow станет явным SSA CFG.

/// Полный HIR-модуль.
#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    /// Имя модуля из `module <name>`.
    pub name: String,
    /// Глобальные константы модуля.
    pub constants: Vec<ConstDecl>,
    /// Функции модуля.
    pub functions: Vec<Function>,
}

/// Объявление глобальной константы.
#[derive(Debug, Clone, PartialEq)]
pub struct ConstDecl {
    /// Имя константы без префикса.
    pub name: String,
    /// Объявленный тип константы.
    pub ty: Type,
    /// Литеральное значение константы.
    pub value: Literal,
}

/// HIR-функция.
#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    /// Имя функции без `@`.
    pub name: String,
    /// Параметры функции.
    pub params: Vec<Param>,
    /// Возвращаемый тип или tuple типов.
    pub return_ty: ReturnType,
    /// Тело функции как список структурных операторов.
    pub body: Vec<Stmt>,
}

/// Параметр функции.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    /// Имя параметра без `$`.
    pub name: String,
    /// Тип параметра.
    pub ty: Type,
}

/// Тип результата функции.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReturnType {
    /// Функция ничего не возвращает.
    Unit,
    /// Функция возвращает одно значение.
    Single(Type),
    /// Функция возвращает несколько значений.
    Tuple(Vec<Type>),
}

/// Базовые типы W16 IR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Type {
    /// Signed 64-bit integer.
    I64,
    /// Unsigned 64-bit integer.
    U64,
    /// 64-bit floating point value.
    F64,
    /// Логическое значение для условий.
    Bool,
    /// Адрес/указатель в памяти runtime.
    Ptr,
    /// Отсутствие значения.
    Unit,
}

/// HIR-оператор.
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// `let $name: ty = value`.
    Let {
        /// Имя локальной переменной без `$`.
        name: String,
        /// Объявленный тип локальной переменной.
        ty: Type,
        /// Инициализирующее выражение.
        value: Expr,
    },

    /// Присваивание уже объявленной локальной переменной.
    Assign {
        /// Имя локальной переменной без `$`.
        name: String,

        /// Новое значение.
        value: Expr,
    },
    /// Структурный `if`.
    If {
        /// Условие, обязано иметь тип `bool`.
        cond: Expr,

        /// Тело then-ветки.
        then_body: Vec<Stmt>,

        /// Тело else-ветки. Пустой список означает отсутствие `else`.
        else_body: Vec<Stmt>,
    },

    /// Структурный `while`.
    While {
        /// Условие, обязано иметь тип `bool`.
        cond: Expr,

        /// Тело цикла.
        body: Vec<Stmt>,
    },

    /// Структурный `do-while`.
    DoWhile {
        /// Тело цикла — выполняется до первой проверки условия.
        body: Vec<Stmt>,
        
        /// Условие, обязано иметь тип `bool`.
        cond: Expr,
    },

    /// Возврат из функции. Пустой список означает `return ()`.
    Return(Vec<Expr>),

    /// Выход из текущего цикла.
    Break,

    /// Переход к следующей итерации цикла.
    Continue,

    /// Немедленная остановка runtime.
    Halt,

    /// Вывод значения(-й) в консоль: `print($x)` или `print($x, $y)`.
    Print(Vec<Expr>),

    /// Выражение как оператор.
    Expr(Expr),
}

/// HIR-выражение.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Литеральное значение.
    Literal(Literal),
    /// Локальная переменная `$name`.
    Local(String),
    /// Глобальная константа.
    Const(String),
    /// Вызов функции `@name(args...)`.
    Call {
        /// Имя функции без `@`.
        function: String,
        /// Аргументы вызова.
        args: Vec<Expr>,
    },
    /// Унарная операция.
    Unary {
        /// Вид операции.
        op: UnaryOp,
        /// Операнд.
        expr: Box<Expr>,
    },
    /// Бинарная операция.
    Binary {
        /// Вид операции.
        op: BinaryOp,
        /// Левый операнд.
        lhs: Box<Expr>,
        /// Правый операнд.
        rhs: Box<Expr>,
    },
    /// Тернарный выбор значения.
    Select {
        /// Условие, обязано иметь тип `bool`.
        cond: Box<Expr>,
        /// Значение при истинном условии.
        then_value: Box<Expr>,
        /// Значение при ложном условии.
        else_value: Box<Expr>,
    },
    /// Явное приведение типа.
    Cast {
        /// Вид приведения.
        kind: CastKind,
        /// Исходное выражение.
        expr: Box<Expr>,
    },
    /// Чтение из runtime memory.
    Load {
        /// Тип загружаемого значения.
        ty: Type,
        /// Адрес.
        addr: Box<Expr>,
    },
    /// Запись в runtime memory.
    Store {
        /// Тип записываемого значения.
        ty: Type,
        /// Адрес.
        addr: Box<Expr>,
        /// Значение.
        value: Box<Expr>,
    },
}

/// Литеральные значения HIR.
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    /// Integer literal. До уточнения типа считается `u64`.
    Int(u64),
    /// Floating-point literal.
    Float(f64),
    /// Boolean literal.
    Bool(bool),
    /// String literal. В HIR типизируется как `ptr`.
    String(String),
}

/// Унарные операции.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// Логическое отрицание.
    Not,
    /// Арифметическое отрицание.
    Neg,
}

/// Бинарные операции.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    /// Сложение.
    Add,
    /// Вычитание.
    Sub,
    /// Умножение.
    Mul,
    /// Деление.
    Div,
    /// Остаток от деления.
    Rem,
    /// Равно.
    Eq,
    /// Не равно.
    Ne,
    /// Меньше.
    Lt,
    /// Меньше или равно.
    Le,
    /// Больше.
    Gt,
    /// Больше или равно.
    Ge,
    /// Побитовое И.
    BitAnd,
    /// Побитовое ИЛИ.
    BitOr,
    /// Побитовое исключающее ИЛИ.
    BitXor,
    /// Логический сдвиг влево.
    Shl,
    /// Логический сдвиг вправо.
    Shr,
}

/// Поддерживаемые явные cast-операции.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastKind {
    /// `i64 -> f64`.
    I2F,
    /// `u64 -> f64`.
    U2F,
    /// `f64 -> i64`.
    F2I,
    /// `f64 -> u64`.
    F2U,
    /// `i64 -> u64`.
    I2U,
    /// `u64 -> i64`.
    U2I,
    /// Усечение `u64 -> u32` (младшие 32 бита).
    TruncU64ToU32,
    /// Расширение `u32 -> u64` с нулями.
    ZextU32ToU64,
    /// Расширение `i32 -> i64` со знаком.
    SextI32ToI64,
    /// Биткаст без изменения битов.
    Bitcast,
}

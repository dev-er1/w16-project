# Текстовый Синтаксис W16 IR

Этот файл задаёт планируемый текстовый синтаксис для двух IR-слоёв W16:

- **W16-HIR** — типизированный структурный IR, близкий к исходному языку.
- **W16-MIR** — типизированный SSA control-flow graph IR.

Синтаксис нужен для dumps, тестов, golden-файлов и ручного чтения. Внутренние
Rust-структуры могут отличаться, но любой корректный in-memory IR должен
печататься в таком виде.

## Общие Лексические Правила

```text
identifier  ::= [A-Za-z_][A-Za-z0-9_]*
function    ::= "@" identifier
block       ::= "^" identifier
value       ::= "%" integer
local       ::= "$" identifier
type        ::= "i64" | "u64" | "f64" | "bool" | "ptr" | "unit"
integer     ::= decimal | "0x" hex
float       ::= decimal "." decimal
string      ::= quoted UTF-8 string
```

Префиксы имён имеют фиксированное значение:

```text
@main       символ функции
^loop       метка basic block
%12         SSA-значение в MIR
$count      лексическая переменная в HIR
```

## W16-HIR

HIR хранит структурные конструкции и лексические переменные. Он проще для
чтения и ближе к исходному языку, чем MIR.

### Модуль

```text
module <name> {
  const <name>: <type> = <literal>

  fn @name(<param-list>) -> <return-type> {
    <hir-statement>*
  }
}
```

Пример:

```text
module bench {
  const LIMIT: u64 = 10000000

  fn @sum_to_limit() -> u64 {
    let $i: u64 = 0
    let $sum: u64 = 0

    while ($i < LIMIT) {
      $sum = $sum + $i
      $i = $i + 1
    }

    return $sum
  }
}
```

### Параметры Функций

```text
fn @add($a: u64, $b: u64) -> u64 {
  return $a + $b
}
```

Синтаксис IR допускает несколько возвращаемых значений, даже если первый
bytecode-lowering будет поддерживать только одно:

```text
fn @pair($x: u64) -> (u64, u64) {
  return ($x, $x + 1)
}
```

### HIR-Операторы

```text
let $name: <type> = <expr>
$name = <expr>
if (<expr>) { <stmt>* } else { <stmt>* }
while (<expr>) { <stmt>* }
return <expr>
return (<expr-list>)
halt
expr <expr>
```

### HIR-Выражения

```text
literal
$local
@function(<arg-list>)
(<expr>)
<expr> + <expr>
<expr> - <expr>
<expr> * <expr>
<expr> / <expr>
<expr> % <expr>
<expr> == <expr>
<expr> != <expr>
<expr> < <expr>
<expr> <= <expr>
<expr> > <expr>
<expr> >= <expr>
<expr> & <expr>
<expr> | <expr>
<expr> ^ <expr>
!<expr>
-<expr>
select(<cond>, <then-expr>, <else-expr>)
cast.<kind>(<expr>)
load.<type>(<addr>)
store.<type>(<addr>, <value>)
```

Виды `cast`:

```text
i2f
u2f
f2i
f2u
```

### Правила Типов HIR

HIR verifier должен отвергать:

- Использование необъявленных локальных переменных.
- Повторное объявление имени в одной области видимости.
- Присваивание значения несовместимого типа.
- Условие `if` или `while` не типа `bool`.
- Арифметику над несовместимыми numeric-типами.
- Возврат значения, не совпадающего с сигнатурой функции.

## W16-MIR

MIR — типизированный SSA IR. В нём есть явные blocks и terminators. Это
основное представление для оптимизаций и lowering в bytecode.

### Модуль

```text
module <name> {
  const <name>: <type> = <literal>

  fn @name(<param-list>) -> <return-type> {
    <block>*
  }
}
```

### Blocks

```text
^block_name(<block-param-list>):
  <mir-inst>*
  <terminator>
```

Параметры блоков заменяют phi-ноды. Так SSA остаётся чистым, а аргументы
переходов видны явно.

Пример:

```text
^loop(%i: u64, %sum: u64):
  %cond: bool = icmp.ult %i, %limit
  br %cond, ^body(%i, %sum), ^exit(%sum)
```

### MIR-Инструкции

```text
%dst: <type> = const.<type> <literal>
%dst: <type> = mov %src

%dst: <type> = add %lhs, %rhs
%dst: <type> = sub %lhs, %rhs
%dst: <type> = mul %lhs, %rhs
%dst: <type> = udiv %lhs, %rhs
%dst: <type> = idiv %lhs, %rhs
%dst: <type> = urem %lhs, %rhs
%dst: <type> = irem %lhs, %rhs
%dst: <type> = neg %src

%dst: f64 = fadd %lhs, %rhs
%dst: f64 = fsub %lhs, %rhs
%dst: f64 = fmul %lhs, %rhs
%dst: f64 = fdiv %lhs, %rhs
%dst: f64 = frem %lhs, %rhs
%dst: f64 = fneg %src
%dst: f64 = fabs %src

%dst: <type> = and %lhs, %rhs
%dst: <type> = or %lhs, %rhs
%dst: <type> = xor %lhs, %rhs
%dst: <type> = not %src
%dst: <type> = shl %lhs, %rhs
%dst: <type> = shr %lhs, %rhs
%dst: <type> = sar %lhs, %rhs

%dst: bool = icmp.eq %lhs, %rhs
%dst: bool = icmp.ne %lhs, %rhs
%dst: bool = icmp.slt %lhs, %rhs
%dst: bool = icmp.sle %lhs, %rhs
%dst: bool = icmp.sgt %lhs, %rhs
%dst: bool = icmp.sge %lhs, %rhs
%dst: bool = icmp.ult %lhs, %rhs
%dst: bool = icmp.ule %lhs, %rhs
%dst: bool = icmp.ugt %lhs, %rhs
%dst: bool = icmp.uge %lhs, %rhs

%dst: bool = fcmp.eq %lhs, %rhs
%dst: bool = fcmp.ne %lhs, %rhs
%dst: bool = fcmp.lt %lhs, %rhs
%dst: bool = fcmp.le %lhs, %rhs
%dst: bool = fcmp.gt %lhs, %rhs
%dst: bool = fcmp.ge %lhs, %rhs

%dst: <type> = select %cond, %then_value, %else_value
%dst: <type> = cast.<kind> %src

%dst: <type> = load.<type> %addr
store.<type> %addr, %value
```

### MIR-Terminators

Каждый block обязан заканчиваться ровно одним terminator:

```text
jmp ^target(<arg-list>)
br %cond, ^then(<arg-list>), ^else(<arg-list>)
ret
ret <value>
ret (<value-list>)
halt
```

Terminators не создают значения.

### Пример MIR

MIR-форма tight loop `sum += i; i++`:

```text
module bench {
  fn @sum_to_limit() -> u64 {
  ^entry:
    %zero: u64 = const.u64 0
    %one: u64 = const.u64 1
    %limit: u64 = const.u64 10000000
    jmp ^loop(%zero, %zero)

  ^loop(%i: u64, %sum: u64):
    %cond: bool = icmp.ult %i, %limit
    br %cond, ^body(%i, %sum), ^exit(%sum)

  ^body(%i_body: u64, %sum_body: u64):
    %next_sum: u64 = add %sum_body, %i_body
    %next_i: u64 = add %i_body, %one
    jmp ^loop(%next_i, %next_sum)

  ^exit(%result: u64):
    ret %result
  }
}
```

### Правила MIR Verifier

MIR verifier должен отвергать:

- Block без terminator.
- Инструкции после terminator.
- Использование неопределённого `%value`.
- Дублирующиеся определения `%value`.
- Условие branch не типа `bool`.
- Аргументы branch/jmp не совпадают с параметрами целевого block.
- Return values не совпадают с сигнатурой функции.
- Type mismatch в arithmetic, comparison, `select`, load или store.
- Недостижимые blocks, если текущий pass требует canonical CFG.

## HIR To MIR Lowering

Структурный control flow HIR понижается в MIR blocks:

```text
while condition { body }
```

становится:

```text
jmp ^loop(...)
^loop(...):
  %cond = ...
  br %cond, ^body(...), ^exit(...)
^body(...):
  ...
  jmp ^loop(...)
^exit(...):
```

Mutable HIR locals превращаются в SSA-значения и параметры блоков там, где
значения проходят через control-flow edges.

## MIR To Bytecode Lowering

MIR lowering должен:

1. Назначить каждому live MIR value регистр W16.
2. Сгенерировать `Load8`, `Load16` или `LoadConst` для констант.
3. Напрямую сгенерировать арифметические и comparison opcodes.
4. Превратить block labels в индексы инструкций bytecode.
5. Материализовать индексы branch target в регистры, потому что текущий W16
   bytecode прыгает через регистры.

Пример MIR branch:

```text
br %cond, ^body(%i, %sum), ^exit(%sum)
```

Первое понижение в bytecode:

```text
Load16 r_target, exit_ip
Jz r_cond, r_target
; fallthrough body path or explicit Jmp to body
```

Будущая версия bytecode может добавить immediate branch opcodes, но MIR не
должен от этого зависеть.

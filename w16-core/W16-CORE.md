# w16-core

`w16-core` — это нижний runtime-слой W16. В этом крейте находятся bytecode, регистровая виртуальная машина и JIT-компиляция через Cranelift.

Если кратко:

```text
w16-ir / w16-lib
    |
    v
W16 bytecode
    |
    +--> VM interpreter
    |
    +--> Cranelift JIT
```

`w16-core` не знает про HIR, MIR, CLI, WCC или другие языки. Он принимает уже готовый `Bytecode` и выполняет его.

## За что отвечает крейт

`w16-core` отвечает за:

- формат W16 bytecode;
- список opcode-ов;
- constant pool;
- структуру инструкции;
- регистровую VM;
- JIT-компиляцию bytecode;
- публичные функции запуска;
- базовые ошибки runtime;
- тесты VM/JIT;
- бенчмарки VM/JIT.

## За что крейт не отвечает

`w16-core` не должен:

- парсить HIR;
- хранить AST HIR или MIR;
- выполнять semantic analysis;
- содержать MIR-оптимизации;
- зависеть от `w16-cli`;
- зависеть от WCC или других frontend-ов;
- генерировать исполняемые файлы.

Генерация исполняемых файлов относится к экспериментальному `w16c`, а HIR/MIR pipeline относится к `w16-ir`.

## Структура

```text
w16-core/src/lib.rs
    Публичный API крейта: re-export-ы, run(), run_by_jit(), VMError.

w16-core/src/bytecode.rs
    OpCode, Instruction, ConstantPool, Bytecode.

w16-core/src/interpreter/
    Регистровая виртуальная машина.

w16-core/src/interpreter/vm.rs
    Основной цикл выполнения bytecode.

w16-core/src/jit/
    JIT backend.

w16-core/src/jit/jit_compiler.rs
    Cranelift JIT-компилятор.

w16-core/tests/
    Тесты VM и JIT.

w16-core/benches/
    Бенчмарки VM и JIT.
```

## Bytecode

W16 использует регистровый bytecode. Каждая инструкция имеет фиксированный размер 4 байта:

```text
| opcode | a | b | c |
| 8 bit  | 8 | 8 | 8 |
```

Поля:

```text
opcode  код операции
a       обычно регистр назначения
b       первый операнд или младший байт imm16
c       второй операнд или старший байт imm16
```

Пример:

```text
Add r0, r1, r2
```

Означает:

```text
r0 = r1 + r2
```

Для инструкций с 16-битным immediate используются поля `b` и `c` вместе:

```text
imm16 = b | (c << 8)
```

Это используется, например, для `Load16` и `LoadConst`.

## Constant Pool

Так как инструкция фиксирована и занимает только 4 байта, большие значения не помещаются прямо в instruction layout. Для этого используется `ConstantPool`.

Примеры значений, которые могут попадать в constant pool:

- большие целые значения;
- `f64`;
- строки;
- данные, на которые bytecode ссылается по индексу.

Инструкция `LoadConst` загружает значение из constant pool по индексу:

```text
LoadConst r0, const_index
```

## Регистровая модель

VM использует 256 виртуальных регистров:

```text
r0..r255
```

Каждый регистр имеет размер `u64`. Интерпретация битов зависит от opcode:

```text
integer ops  -> u64 / i64
float ops    -> f64 bits
memory ops   -> address / raw bytes
```

`r0` используется как основной регистр результата на уровне W16 ABI. Например, `w16-lib::RunResult::r0()` возвращает значение из регистра `0`.

## Группы opcode-ов

Основные группы операций:

```text
0x00..0x0F   halt / no-op
0x10..0x1F   data movement and memory
0x20..0x3F   arithmetic
0x40..0x5F   comparisons and select
0x60..0x66   bit operations
0x67..0x72   control flow
0x80..0x8F   casts
0x90..0x93   I/O
```

Фактический список находится в:

```text
w16-core/src/bytecode.rs
```

## VM

VM находится в:

```text
w16-core/src/interpreter/vm.rs
```

Она выполняет bytecode напрямую:

```text
Bytecode
    |
    v
decode instruction
    |
    v
execute opcode
    |
    v
update registers / memory / pc
```

VM — основной стабильный способ запуска W16-программ. Если есть сомнение между поведением VM и JIT, эталоном обычно должна считаться VM.

Публичный запуск через VM:

```rust
use w16_core::{run, Bytecode};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytecode = Bytecode::new();
    let registers = run(&bytecode, 1024 * 1024)?;
    println!("{}", registers[0]);
    Ok(())
}
```

## JIT

JIT находится в:

```text
w16-core/src/jit/jit_compiler.rs
```

Он компилирует W16 bytecode в native code in memory через Cranelift:

```text
Bytecode
    |
    v
Cranelift IR
    |
    v
machine code in memory
    |
    v
execute
```

Публичный запуск через JIT:

```rust
use w16_core::{run_by_jit, Bytecode};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytecode = Bytecode::new();
    let registers = run_by_jit(&bytecode)?;
    println!("{}", registers[0]);
    Ok(())
}
```

JIT быстрее на некоторых задачах, но сложнее. Новые opcode-ы должны быть добавлены и в VM, и в JIT, иначе поведение режимов будет расходиться.

## Публичный API

Главные re-export-ы:

```rust
pub use crate::bytecode::{Bytecode, ConstantPool, Instruction, OpCode};
pub use crate::interpreter::vm::{REGISTER_COUNT, VM};
```

Главные функции:

```rust
run(bytecode, memory_size)
run_by_jit(bytecode)
```

Ошибки VM:

```rust
VMError::InvalidOpCode(...)
VMError::MemoryAccessViolation(...)
VMError::ConstantPoolError
VMError::DivisionByZero
```

Удобные макросы для тестов:

```rust
inst!(OpCode::Add, 0, 1, 2)
inst_imm!(OpCode::Load16, 0, 1234)
```

## Как добавить новый opcode

Если вы добавляете новый opcode, проверьте все слои, которые его касаются.

Обычно нужно изменить:

```text
w16-core/src/bytecode.rs
w16-core/src/interpreter/vm.rs
w16-core/src/jit/jit_compiler.rs
w16-ir/src/compiler_to_bytecode.rs
```

И добавить тесты:

```text
w16-core/tests/vm_tests/
w16-core/tests/jit_tests/
w16-ir/tests/
```

Минимальный checklist:

- opcode добавлен в `OpCode`;
- VM умеет его выполнять;
- JIT умеет его компилировать или явно возвращает ошибку;
- MIR -> bytecode compiler умеет его генерировать, если операция доступна из MIR;
- есть тест на нормальный случай;
- есть тест на ошибочный случай, если opcode может падать;
- поведение VM и JIT совпадает.

## Инварианты runtime

`w16-core` должен сохранять несколько простых правил:

- инструкция всегда занимает 4 байта;
- регистров не больше `REGISTER_COUNT`;
- memory access не должен выходить за границы памяти;
- division by zero должен возвращать ошибку, а не вызывать неопределённое поведение;
- constant pool access должен проверять границы;
- JIT не должен принимать bytecode, который он не умеет безопасно скомпилировать.

## Тесты

Запуск тестов `w16-core`:

```powershell
cargo test -p w16-core
```

Запуск всех тестов workspace:

```powershell
cargo test
```

Основные группы тестов:

```text
w16-core/tests/vm_tests/
    arithmetic, memory, registers, control flow, call stack.

w16-core/tests/jit_tests/
    JIT behavior tests.
```

## Бенчмарки

Бенчмарки находятся в:

```text
w16-core/benches/
```

Запуск:

```powershell
cargo bench -p w16-core
```

Отдельные bench target-ы:

```text
fast_vm
fast_jit
```

Бенчмарки нужны не только для поиска ускорений, но и для проверки, что оптимизации не ухудшают базовый runtime без причины.

## Где проходит граница с другими крейтами

```text
w16-ir
    HIR/MIR, проверки, оптимизации, компиляция в bytecode.

w16-lib
    Удобный API, который вызывает w16-ir и w16-core.

w16-cli
    Командная строка, которая вызывает w16-lib/w16c.

w16c
    Экспериментальная AOT-компиляция bytecode в объектный/исполняемый файл.

w16-langs/w16-cc
    C frontend, который генерирует W16 HIR.
```

`w16-core` должен оставаться маленьким и строгим: bytecode входит, результат выполнения выходит.

## Связанные документы

- [`../ARCHITECTURE_RU.md`](../ARCHITECTURE_RU.md)
- [`../w16-ir/W16-IR.md`](../w16-ir/W16-IR.md)
- [`../w16-lib/W16-LIB.md`](../w16-lib/W16-LIB.md)
- [`../w16-cli/W16-CLI.md`](../w16-cli/W16-CLI.md)
- [`../w16c/README.md`](../w16c/README.md)

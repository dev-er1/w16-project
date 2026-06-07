# Архитектура W16

Этот документ описывает, как устроен проект W16: какие крейты за что отвечают, как данные проходят через pipeline, где находятся стабильные API, а где экспериментальные части. Он написан как карта для будущих контрибьюторов: если нужно понять проект не по отдельным файлам, а целиком, начинать лучше отсюда.

## Коротко

W16 — это runtime и набор промежуточных представлений для создания языков программирования. Проект не является одним монолитным компилятором. Он разделён на несколько слоёв:

```text
Исходный язык
    |
    | frontend конкретного языка
    v
W16 HIR
    |
    | parser / semantic / lowering
    v
W16 MIR
    |
    | verifier / optimizer / bytecode compiler
    v
W16 bytecode
    |
    | VM или JIT
    v
Выполнение программы
```

Главная идея: разные языки могут компилироваться не напрямую в машинный код, а в общий W16 MIR. После этого они получают общий backend: оптимизации, bytecode, VM, JIT и в будущем более зрелый AOT.

## Workspace

Корневой workspace содержит основные крейты W16:

```text
w16-cli     - интерфейс командной строки для пользователя
w16-core    - bytecode, VM и JIT runtime
w16-ir      - HIR, MIR, проверки, оптимизации и компиляция в bytecode
w16-lib     - удобный публичный facade API поверх w16-ir и w16-core
w16c        - экспериментальная AOT-компиляция bytecode в объектные/исполняемые файлы
```

Отдельный workspace `w16-langs` содержит экспериментальные языки, которые используют W16 как backend:

```text
w16-langs/w16-cc - экспериментальный C frontend, сокращённо WCC
```

Такое разделение важно: ядро W16 не должно зависеть от конкретного языка, а примеры языков могут развиваться быстрее и свободнее.

## Главные слои

### 1. Frontend языка

Frontend — это часть, которая понимает конкретный исходный язык.

Например, `w16-langs/w16-cc` разбирает C-подобный код:

```text
C source
    |
    v
lexer
    |
    v
parser
    |
    v
semantic checker
    |
    v
C AST
    |
    v
W16 HIR
```

Frontend языка не должен знать детали VM, JIT или формата инструкций bytecode. Его задача — корректно разобрать исходный язык, проверить семантику и перевести результат в W16 HIR или MIR.

### 2. HIR

HIR — это высокоуровневое промежуточное представление W16.

Оно находится в:

```text
w16-ir/src/hir.rs
w16-ir/src/lexer/
w16-ir/src/parser/
w16-ir/src/semantic/
```

HIR нужен для того, чтобы фронтендам было удобно описывать программу в терминах функций, переменных, выражений, типов, циклов, условий и вызовов. Он ближе к исходному языку, чем bytecode, но уже принадлежит W16.

Типичный путь для HIR:

```text
HIR text
    |
    v
lexer -> tokens
    |
    v
parser -> HIR AST
    |
    v
semantic verifier
    |
    v
lowerer -> MIR
```

Текстовый HIR нужен для тестов, отладки, CLI и ручного написания примеров. AST HIR нужен для фронтендов вроде WCC, которые могут строить HIR напрямую без текстовой сериализации.

### 3. MIR

MIR — это более низкоуровневое промежуточное представление перед bytecode.

Оно находится в:

```text
w16-ir/src/mir.rs
w16-ir/src/mir_f/
w16-ir/src/translator/
```

MIR ближе к машинной модели: базовые блоки, SSA-значения, terminator-ы, явные операции, переходы и вызовы. Именно здесь должны жить серьёзные оптимизации и строгие проверки корректности.

Основные части MIR-слоя:

```text
w16-ir/src/translator/lowerer.rs
    HIR -> MIR

w16-ir/src/mir_f/mir_verify/
    проверка корректности MIR

w16-ir/src/mir_f/mir_analyze/
    анализы: циклы, induction variables и другие данные для оптимизаций

w16-ir/src/mir_f/mir_optimizer/
    оптимизационные проходы

w16-ir/src/compiler_to_bytecode.rs
    MIR -> W16 bytecode
```

MIR должен быть главным местом для оптимизаций, потому что он достаточно низкоуровневый для анализа, но ещё не потерял структуру программы.

### 4. Bytecode

Bytecode — это исполняемый формат W16 runtime.

Он находится в:

```text
w16-core/src/bytecode.rs
```

W16 bytecode использует регистровую модель. Инструкция имеет фиксированный размер 4 байта:

```text
| opcode | a | b | c |
```

Где:

```text
opcode - операция
a      - регистр назначения
b      - первый операнд
c      - часть immediate-значения
```

Большие значения хранятся в constant pool, а инструкция содержит индекс константы. Это позволяет сохранить компактный формат инструкций.

Bytecode не должен знать про HIR, MIR или исходный язык. Он является нижним переносимым слоем исполнения.

### 5. Runtime

Runtime находится в:

```text
w16-core/src/interpreter/
w16-core/src/jit/
```

Он умеет запускать bytecode двумя путями:

```text
bytecode -> VM
bytecode -> JIT -> native code in memory
```

VM — это основной стабильный способ выполнения. JIT — более быстрый, но более сложный путь, основанный на Cranelift.

Публичные функции runtime находятся в `w16-core/src/lib.rs`:

```rust
run(bytecode, memory_size)
run_by_jit(bytecode)
```

## Подробная навигация по крейтам

## w16-core

`w16-core` — это нижний слой W16. Он не должен зависеть от HIR, MIR, CLI или конкретных языков.

Отвечает за:

- формат bytecode;
- список opcode-ов;
- constant pool;
- виртуальную машину;
- ошибки VM;
- JIT-компиляцию bytecode;
- макросы для удобного создания инструкций в тестах.

Важные файлы:

```text
w16-core/src/bytecode.rs
    OpCode, Instruction, Bytecode, ConstantPool.

w16-core/src/interpreter/vm.rs
    Реализация регистровой VM.

w16-core/src/jit/jit_compiler.rs
    Cranelift JIT backend.

w16-core/src/lib.rs
    Публичный API крейта.

w16-core/tests/
    Тесты VM и JIT.

w16-core/benches/
    Бенчмарки VM и JIT.
```

Граница ответственности:

```text
w16-core принимает только bytecode.
w16-core не парсит HIR.
w16-core не знает про C, WCC или другие языки.
w16-core не должен содержать middle-end оптимизации.
```

Если нужно добавить новый opcode, обычно нужно менять:

```text
w16-core/src/bytecode.rs
w16-core/src/interpreter/vm.rs
w16-core/src/jit/jit_compiler.rs
w16-ir/src/compiler_to_bytecode.rs
тесты VM/JIT/IR
```

## w16-ir

`w16-ir` — это compiler middle-end проекта.

Отвечает за:

- токенизацию текстового HIR;
- парсинг HIR;
- AST HIR;
- семантические проверки HIR;
- AST MIR;
- lowering из HIR в MIR;
- MIR verifier;
- MIR-анализы;
- MIR-оптимизации;
- компиляцию MIR в bytecode.

Важные файлы:

```text
w16-ir/src/lexer/
    Лексер текстового HIR.

w16-ir/src/parser/
    Парсер текстового HIR.

w16-ir/src/hir.rs
    Структуры HIR.

w16-ir/src/semantic/
    Проверки HIR.

w16-ir/src/mir.rs
    Структуры MIR.

w16-ir/src/translator/lowerer.rs
    Перевод HIR -> MIR.

w16-ir/src/mir_f/
    MIR verifier, анализы и оптимизации.

w16-ir/src/compiler_to_bytecode.rs
    Перевод MIR -> bytecode.

w16-ir/docs/IR_SYNTAX.md
    Документация по синтаксису HIR/MIR.

w16-ir/tests/
    Тесты HIR pipeline и оптимизатора.
```

Граница ответственности:

```text
w16-ir не должен запускать программу сам.
w16-ir может создавать bytecode, но выполнение отдаёт w16-core.
w16-ir не должен зависеть от CLI.
w16-ir не должен зависеть от WCC.
```

Идеальный pipeline внутри `w16-ir`:

```text
source text
    |
    v
tokens
    |
    v
HIR AST
    |
    v
HIR semantic verifier
    |
    v
MIR AST
    |
    v
MIR verifier
    |
    v
MIR optimizer
    |
    v
MIR verifier
    |
    v
bytecode
```

Правило для оптимизаций: каждый оптимизационный проход должен сохранять корректность MIR. Если pass меняет CFG, значения, типы или terminator-ы, после него полезно запускать verifier хотя бы в debug/test режиме.

## w16-lib

`w16-lib` — это удобный публичный facade API.

Он нужен для пользователей, которые не хотят вручную собирать pipeline из `w16-ir` и `w16-core`.

Отвечает за:

- запуск HIR из текста;
- запуск HIR из токенов;
- запуск HIR AST;
- запуск MIR AST;
- запуск готового bytecode;
- выбор режима выполнения: VM или JIT;
- единый тип ошибки для верхнего уровня;
- удобный builder `W16`.

Важный файл:

```text
w16-lib/src/lib.rs
```

Примерная роль:

```rust
let result = w16_lib::run_hir_text(source)?;
```

Внутри это превращается в:

```text
HIR text -> HIR AST -> MIR -> bytecode -> VM/JIT
```

Граница ответственности:

```text
w16-lib не реализует свой compiler backend.
w16-lib не должен содержать отдельную VM.
w16-lib не должен включать AOT как стабильную часть API.
w16-lib должен быть самым удобным входом для пользователей библиотеки.
```

Именно `w16-lib` лучше всего показывать в README, crates.io и примерах.

## w16-cli

`w16-cli` — это командная строка для конечного пользователя.

Отвечает за:

- разбор аргументов;
- вывод help;
- запуск HIR-файлов;
- выбор VM/JIT режима;
- debug dump внутренних стадий;
- измерение времени выполнения.

Важные файлы:

```text
w16-cli/src/main.rs
    Точка входа.

w16-cli/src/cmd.rs
    Модель команд и список команд для help.

w16-cli/src/parser.rs
    Разбор аргументов CLI.

w16-cli/src/executer.rs
    Выполнение разобранной команды.

w16-cli/src/help.rs
    Форматирование help.

w16-cli/src/error.rs
    Ошибки CLI.
```

CLI должен быть тонким слоем. Он не должен сам реализовывать компилятор, оптимизатор или runtime. Его задача — взять аргументы пользователя и вызвать `w16-lib`, `w16-ir`, `w16-core` или экспериментальные backend-и через понятный интерфейс.

Команды верхнего уровня сейчас должны оставаться простыми:

```text
w16 run <file>
w16 build <file>
w16 dbg <stage> <file>
w16 version
w16 help
```

## w16c

`w16c` — экспериментальный AOT backend.

Отвечает за:

- компиляцию W16 bytecode в объектный файл через Cranelift;
- попытку собрать исполняемый файл через системный линковщик;
- работу с платформенными деталями линковки.

Важные файлы:

```text
w16c/src/lib.rs
w16c/src/dotobj/mod.rs
w16c/README.md
```

Статус:

> experimental


Причина: AOT требует внешнего линковщика и платформенных соглашений. На Windows это обычно `link.exe` из Visual Studio/MSVC. На Linux/macOS обычно нужен `cc` или другой системный linker driver.

Граница ответственности:

```text
w16c не должен быть обязательным для запуска W16-программ.
w16-lib не должен зависеть от w16c как от стабильного runtime.
w16-cli может показывать AOT-команды только если они явно поддерживаются окружением.
```

## w16-langs/w16-cc

`w16-cc`, или WCC, — экспериментальный C frontend поверх W16.

Он находится отдельно от корневого workspace:

```text
w16-langs/w16-cc
```

Отвечает за:

- lexer C-подобного языка;
- parser;
- semantic checker;
- таблицу символов;
- диагностику;
- C AST;
- перевод C AST в W16 HIR;
- запуск через W16 runtime;
- поддержку некоторых C-типов и значений, включая F80.

Важные файлы:

```text
w16-langs/w16-cc/src/frontend/lexer/
    Токены и лексер.

w16-langs/w16-cc/src/frontend/parser/
    AST и парсер.

w16-langs/w16-cc/src/frontend/semantic/
    Проверки имён, типов и символов.

w16-langs/w16-cc/src/codegen/
    Перевод AST в W16 HIR.

w16-langs/w16-cc/src/value/f80.rs
    80-битное floating-point значение для long double.

w16-langs/w16-cc/tests/
    Тесты lexer/parser/codegen/F80.
```

WCC важен как showcase: он показывает, что W16 можно использовать как backend для отдельного языка.

Граница ответственности:

```text
WCC может зависеть от w16-lib.
WCC не должен быть обязательной частью w16-core.
WCC не должен ломать стабильность HIR/MIR API ради внутренних удобств.
```

## Основные pipeline-и

## Запуск HIR через CLI

```text
пользователь
    |
    v
w16 run main.w16h
    |
    v
w16-cli
    |
    v
w16-lib
    |
    v
w16-ir: lexer -> parser -> semantic -> lowerer -> optimizer -> bytecode
    |
    v
w16-core: VM или JIT
```

## Запуск HIR через библиотеку

```text
Rust-приложение
    |
    v
w16_lib::run_hir_text(source)
    |
    v
w16-ir
    |
    v
w16-core
```

Этот путь должен быть самым стабильным для внешних пользователей.

## Запуск frontend-а языка

```text
C source
    |
    v
w16-cc frontend
    |
    v
C AST
    |
    v
W16 HIR AST
    |
    v
w16-lib
    |
    v
W16 runtime
```

Такой путь показывает, как другие языки могут использовать W16.

## Debug pipeline

Debug-команды должны помогать увидеть внутренние стадии:

```text
tokens
HIR
MIR before optimization
MIR after optimization
bytecode
full pipeline
```

Это важно для контрибьюторов: если программа работает неправильно, можно быстро понять, где сломался pipeline.

## Границы стабильности

Не все части проекта одинаково стабильны.

### Более стабильные

```text
w16-core bytecode model
w16-core VM
w16-ir HIR parser/semantic path
w16-lib facade API
```

Эти части стоит менять осторожно, с тестами и понятной миграцией.

### Активно развиваются

```text
w16-ir MIR
w16-ir optimizer
w16-cli commands
w16-langs/w16-cc
```

Здесь допустимы изменения API, если они улучшают архитектуру и покрыты тестами.

### Экспериментальные

```text
w16-core JIT
w16c AOT
native object/executable generation
```

Эти части могут быть быстрыми и интересными, но не должны блокировать основной путь запуска программ через VM.

## Правила для контрибьюторов

## Если вы меняете opcode

Проверьте:

```text
w16-core/src/bytecode.rs
w16-core/src/interpreter/vm.rs
w16-core/src/jit/jit_compiler.rs
w16-ir/src/compiler_to_bytecode.rs
w16-core/tests/
w16-ir/tests/
```

Новый opcode должен иметь:

- понятную семантику;
- тест VM;
- при возможности тест JIT;
- поддержку в MIR -> bytecode, если opcode генерируется компилятором.

## Если вы меняете HIR

Проверьте:

```text
w16-ir/src/hir.rs
w16-ir/src/parser/
w16-ir/src/semantic/
w16-ir/src/translator/lowerer.rs
w16-ir/tests/hir_pipeline_tests.rs
w16-ir/docs/IR_SYNTAX.md
```

HIR должен оставаться удобным для frontend-ов. Если изменение усложняет генерацию HIR из внешнего языка, стоит подумать, не лучше ли перенести сложность в lowerer или MIR.

## Если вы меняете MIR

Проверьте:

```text
w16-ir/src/mir.rs
w16-ir/src/translator/lowerer.rs
w16-ir/src/mir_f/mir_verify/
w16-ir/src/mir_f/mir_optimizer/
w16-ir/src/compiler_to_bytecode.rs
w16-ir/tests/optimizer_tests.rs
```

MIR должен сохранять инварианты:

- каждый `ValueId` определён корректно;
- типы операндов подходят операции;
- каждый базовый блок имеет terminator;
- переходы указывают на существующие блоки;
- аргументы переходов соответствуют параметрам целевого блока;
- оптимизации не оставляют IR в некорректном состоянии.

## Если вы меняете CLI

Проверьте:

```text
w16-cli/src/cmd.rs
w16-cli/src/parser.rs
w16-cli/src/executer.rs
w16-cli/src/help.rs
```

CLI должен оставаться предсказуемым. Пользователь не обязан знать внутреннюю архитектуру, чтобы запустить файл или посмотреть debug stage.

## Если вы меняете WCC

Проверьте:

```text
w16-langs/w16-cc/src/frontend/
w16-langs/w16-cc/src/codegen/
w16-langs/w16-cc/tests/
```

WCC должен генерировать корректный W16 HIR. Если C-код не поддерживается, лучше дать понятную ошибку, чем генерировать неправильный HIR.

## Тестирование

Основные тесты корневого workspace:

```bash
cargo test
```

Тесты экспериментальных языков:

```bash
cd w16-langs
cargo test
```

Бенчмарки runtime:

```bash
cargo bench -p w16-core
```

Перед PR желательно прогонять:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets
cargo test
```

И отдельно:

```bash
cd w16-langs
cargo test
```

## Как читать проект впервые

Если вы впервые открыли W16, лучше идти так:

1. `README_RU.md` — общее понимание проекта.
2. `ARCHITECTURE_RU.md` — карта архитектуры.
3. `w16-lib/src/lib.rs` — удобный публичный API.
4. `w16-ir/docs/IR_SYNTAX.md` — синтаксис HIR/MIR.
5. `w16-ir/src/hir.rs` — модель HIR.
6. `w16-ir/src/mir.rs` — модель MIR.
7. `w16-core/src/bytecode.rs` — формат bytecode.
8. `w16-core/src/interpreter/vm.rs` — исполнение bytecode.
9. `w16-langs/w16-cc/src/codegen/` — пример frontend-а, который генерирует W16 HIR.

## Что считать хорошим вкладом

Хороший вклад в W16 обычно делает одно из трёх:

- улучшает корректность;
- улучшает понятность;
- улучшает скорость без потери корректности.

Примеры хороших задач:

- новый тест на HIR/MIR pipeline;
- улучшение диагностики с span-ами;
- маленькая оптимизация MIR с verifier-тестом;
- поддержка нового C-конструкта в WCC;
- исправление расхождения VM и JIT;
- документация с примером запуска;
- уменьшение неясности в CLI help.

## Что пока лучше не делать

Пока проект находится на ранней стадии, лучше не усложнять ядро без необходимости:

- не добавлять крупные языковые возможности прямо в `w16-core`;
- не делать `w16-core` зависимым от WCC или CLI;
- не считать AOT стабильным основным путём;
- не добавлять оптимизацию MIR без теста на корректность;
- не менять bytecode format без явной причины и миграционного плана.

## Ключевая архитектурная идея

W16 должен оставаться набором слоёв, а не одним большим компилятором:

```text
frontends -> HIR -> MIR -> bytecode -> runtime
```

Если это разделение сохраняется, проект можно развивать сразу в нескольких направлениях:

- улучшать VM;
- развивать JIT;
- писать новые языки поверх W16;
- стабилизировать `w16-lib`;
- экспериментировать с AOT;
- добавлять оптимизации в MIR.

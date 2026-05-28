# w16-cli

`w16-cli` — это командная строка W16. Она предназначена для запуска HIR-файлов, просмотра внутренних стадий компиляции и экспериментальной сборки исполняемых файлов.

CLI остаётся тонким слоем над другими крейтами:

```text
w16-cli
    |
    v
w16-lib
    |
    v
w16-ir -> w16-core
```

Он не реализует свой парсер HIR, оптимизатор, VM или JIT. Его задача — разобрать аргументы пользователя, прочитать файл и вызвать нужный слой W16.

## Установка

Из workspace:

```bash
cargo run -p w16-cli -- help
```

Если бинарник уже собран или установлен как `w16`:

```bash
w16 help
```

## Команды

```text
w16 run <file> [-i | -j] [--time]
w16 build <file>
w16 dbg <stage> <file>
w16 version
w16 help
```

## `run`

Запускает HIR-файл через W16 runtime.

```bash
w16 run main.w16h
```

По умолчанию используется интерпретатор:

```bash
w16 run main.w16h -i
```

Для запуска через JIT:

```bash
w16 run main.w16h -j
```

Для вывода времени выполнения:

```bash
w16 run main.w16h --time
```

Флаги можно комбинировать:

```bash
w16 run main.w16h -j --time
```

Режимы:

```text
-i      VM mode
-j      JIT mode
--time  print execution time
```

## `dbg`

Показывает внутренние стадии W16 pipeline. Это основная команда для отладки HIR, MIR и bytecode.

```bash
w16 dbg tokens main.w16h
w16 dbg hir main.w16h
w16 dbg mir main.w16h
w16 dbg mir-opt main.w16h
w16 dbg bytecode main.w16h
w16 dbg full main.w16h
```

Доступные стадии:

```text
tokens    HIR token stream
hir       HIR AST
mir       MIR AST before optimizer
mir-opt   MIR AST after optimizer
bytecode  compiled bytecode instructions
full      all stages combined
```

Эта команда полезна, когда программа ведёт себя неправильно и нужно понять, на каком этапе появилась ошибка:

```text
HIR text -> tokens -> HIR AST -> MIR -> optimized MIR -> bytecode -> runtime
```

## `build`

Экспериментальная команда для AOT-сборки.

```bash
w16 build main.w16h
```

Она делает примерно следующее:

```text
HIR source
    |
    v
w16-lib: HIR -> MIR -> bytecode
    |
    v
w16c: bytecode -> object file -> executable
```

**Важно**: `build` зависит от `w16c`, а `w16c` использует платформенный линковщик. На Windows обычно нужен `link.exe` из Visual Studio/MSVC. На Linux/macOS обычно нужен `cc` или совместимый linker driver.

Поэтому `build` стоит считать experimental-командой. Основной стабильный путь запуска программ сейчас:

```bash
w16 run main.w16h
```

## `version`

Печатает версию CLI:

```bash
w16 version
```

## `help`

Показывает список команд:

```bash
w16 help
```

## Пример HIR-файла

```text
module main {
  fn @main() -> u64 {
    print(42)
    return 42
  }
}
```

Запуск:

```bash
w16 run main.w16h --time
```

Просмотр всех стадий:

```bash
w16 dbg full main.w16h
```

## Внутренняя структура

```text
w16-cli/src/main.rs
    Точка входа.

w16-cli/src/cmd.rs
    Модель команд, флагов и registry для help.

w16-cli/src/parser.rs
    Разбор аргументов командной строки.

w16-cli/src/executer.rs
    Выполнение уже разобранной команды.

w16-cli/src/help.rs
    Вывод справки.

w16-cli/src/error.rs
    Ошибки CLI.
```

## Граница ответственности

`w16-cli` должен:

- принимать аргументы пользователя;
- валидировать команду;
- читать входной файл;
- вызывать `w16-lib`, `w16-core` или `w16c`;
- печатать понятные ошибки и результаты.

`w16-cli` не должен:

- реализовывать собственную VM;
- дублировать HIR parser;
- содержать MIR-оптимизации;
- зависеть от конкретного frontend-а языка вроде WCC как от обязательной части.

Если поддержка WCC появится в общем CLI, лучше держать её в отдельной группе команд:

```bash
w16 cc check main.c
w16 cc run main.c
w16 cc emit-hir main.c
```

Так основной HIR-путь останется простым и предсказуемым.

## Связанные документы

- [`../w16-lib/W16-LIB.md`](../w16-lib/W16-LIB.md)
- [`../w16-core/W16-CORE.md`](../w16-core/W16-CORE.md)
- [`../w16-ir/W16-IR.md`](../w16-ir/W16-IR.md)
- [`../w16c/README.md`](../w16c/README.md)
- [`../ARCHITECTURE_RU.md`](../ARCHITECTURE_RU.md)

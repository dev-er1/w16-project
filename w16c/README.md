# w16c

AOT-компилятор W16 байткода в нативные исполняемые файлы.

## Как работает

```
W16 байткод -> Cranelift ObjectModule -> .obj/.o -> линковщик -> .exe/.elf
```

1. **Генерация объектного файла** — `cranelift-object` транслирует W16 инструкции
   в машинный код целевой платформы и упаковывает в COFF (Windows) или ELF (Linux/macOS).

2. **Линковка** — вызывается системный линковщик:
   - Windows: `link.exe` от Visual Studio (через крейт `cc`) или из PATH
   - Linux/macOS: `cc` (обычно gcc или clang)

   Результат линкуется с `w16-fns.lib` — статической библиотекой W16 runtime
   (функции печати, memory helpers и т.д.).

## API

```rust
use w16c::{compile_to_executable, compile_to_executable_with_opts, OptLevel};
use target_lexicon::Triple;

// Стандартная компиляция (OptLevel::Speed)
compile_to_executable(&bytecode, "my_program", Triple::host())?;

// С явным уровнем оптимизации
compile_to_executable_with_opts(
    &bytecode,
    "my_program",
    Triple::host(),
    OptLevel::None,  // быстрая компиляция для отладки
)?;
```

## Уровни оптимизации

| `OptLevel`      | Cranelift | Описание                          |
|-----------------|-----------|-----------------------------------|
| `None`          | `none`    | Без оптимизаций, быстрая сборка   |
| `Speed`         | `speed`   | Оптимизация скорости (по умолчанию)|
| `SpeedAndSize`  | `speed_and_size` | Оптимизация скорости и размера |

## Требования

- **Windows**: Visual Studio Build Tools (для `link.exe`) + `w16-fns.lib` в `%USERPROFILE%\w16\static-lib\`
- **Linux/macOS**: `cc` в PATH (gcc или clang)

## Ограничения

- Кросс-компиляция поддерживается на уровне объектного файла (Cranelift умеет),
  но линковщик всегда нативный — для настоящей кросс-компиляции нужен `lld`.
- Не поддерживает динамические переходы (`DynamicJumpTarget`) — только статические адреса.
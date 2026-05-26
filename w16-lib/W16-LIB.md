# w16-lib

Библиотека для использования [W16](https://github.com/dev-er1/w16-project).

## Как использовать W16
Если вы делаете свой язык, и хотите использовать W16 как runtime — во первых огромное спасибо,
за то что выбрали именно w16! Во вторых, инструкция как использовать W16:

1. Подключите библиотеку:
```bash
cargo add w16-lib
```
2. Сделайте транслятор вашего языка, в **W16 HIR**, вы можете транслировать ваш язык в
- HIR в виде текста.
- HIR в виде AST.

Либо вы можете транслировать в:
- W16 MIR.
- W16 Bytecode.

И в итоге вот как будет выглядеть использование W16(псевдокод):
```rust
use w16_lib::{run_hir_text_as, ExecutionMode};

fn main() {
    let source: &str = /* HIR в виде текста */;

    /// Выполнить ваш W16 HIR в виде текста
    let result: Result<RunResult, W16Error> = run_hir_text_as(&source, ExecutionMode::Interpreter /* Либо "ExecutionMode::Jit", если хотите JIT-компилировать*/);
}
```

## Как использовать w16 ( только примеры )

- Выполнить текстовый HIR
```rust
use w16_lib::{run_hir_text_as, ExecutionMode};

fn main() {
    let source: &str = /* HIR в виде текста */;

    /// Выполнить ваш W16 HIR в виде текста
    let result: Result<RunResult, W16Error> = run_hir_text_as(&source, ExecutionMode::Interpreter /* Либо "ExecutionMode::Jit", если хотите JIT-компилировать*/);
}
```

- Выполнить HIR в виде [`Module`]

*Для этого подключите библиотеку `w16-ir`*:
```bash
cargo add w16-ir
```

```rust
use w16_lib::{run_hir_text_as, ExecutionMode};
use w16_ir::hir::Module;

fn main() {
    let source: Module = Module { name: /* Название модуля. String */, constants: /* Константы в модуле. Vec<ConstDecl> */, functions: /* Функции в модуле. Vec<Function> */};

    let result: Result<RunResult, W16Error> = W16::new().run_hir_ast(&source);
}
```
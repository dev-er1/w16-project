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
    let source: String = /* HIR в виде текста */;

    /// Выполнить ваш W16 HIR в виде текста
    let result: Result<RunResult, W16Error> = run_hir_text_as(&source, ExecutionMode::Interpreter /* Либо "ExecutionMode::Jit", если хотите JIT-компилировать*/)
}
```
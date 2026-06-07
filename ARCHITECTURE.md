# W16 Architecture

This document explains how the W16 project is structured: which crates own which responsibilities, how data moves through the pipeline, where the stable APIs are, and which parts are still experimental. It is meant to be a map for future contributors. If you want to understand the project as a whole instead of reading files at random, this is the best place to start.

## Short Version

W16 is a runtime and a set of intermediate representations for building programming languages. The project is not a single monolithic compiler. It is split into several layers:

```text
Source language
    |
    | frontend for a concrete language
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
    | VM or JIT
    v
Program execution
```

The main idea is that different languages can compile into the shared W16 HIR or MIR instead of compiling directly to machine code. After that, they all receive the same backend: optimizations, bytecode, VM, JIT, and eventually a more mature AOT path.

## Workspace

The root workspace contains the core W16 crates:

```text
w16-cli     - command-line interface for users
w16-core    - bytecode, VM, and JIT runtime
w16-ir      - HIR, MIR, checks, optimizations, and bytecode compilation
w16-lib     - convenient public facade API over w16-ir and w16-core
w16c        - experimental AOT compilation from bytecode to object/executable files
```

The separate `w16-langs` workspace contains experimental languages that use W16 as a backend:

```text
w16-langs/w16-cc - experimental C frontend, shortened as WCC
```

This separation is important: the W16 core should not depend on a concrete language, while example languages can evolve more freely.

## Main Layers

### 1. Language Frontend

A frontend is the part that understands a concrete source language.

For example, `w16-langs/w16-cc` parses C-like code:

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

A language frontend should not know the details of the VM, JIT, or bytecode instruction format. Its job is to parse the source language, check semantics, and translate the result into W16 HIR or MIR.

### 2. HIR

HIR is the high-level intermediate representation of W16.

It lives in:

```text
w16-ir/src/hir.rs
w16-ir/src/lexer/
w16-ir/src/parser/
w16-ir/src/semantic/
```

HIR exists so frontends can describe programs in terms of functions, variables, expressions, types, loops, conditions, and calls. It is closer to the source language than bytecode, but it already belongs to W16.

The typical HIR path is:

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

Textual HIR is useful for tests, debugging, CLI usage, and hand-written examples. HIR AST is useful for frontends such as WCC, which can build HIR directly without serializing it to text first.

### 3. MIR

MIR is the lower-level intermediate representation before bytecode.

It lives in:

```text
w16-ir/src/mir.rs
w16-ir/src/mir_f/
w16-ir/src/translator/
```

MIR is closer to the machine model: basic blocks, SSA values, terminators, explicit operations, jumps, and calls. This is where serious optimizations and strict correctness checks should live.

The main MIR components are:

```text
w16-ir/src/translator/lowerer.rs
    HIR -> MIR

w16-ir/src/mir_f/mir_verify/
    MIR correctness checks

w16-ir/src/mir_f/mir_analyze/
    analyses: loops, induction variables, and other data for optimizations

w16-ir/src/mir_f/mir_optimizer/
    optimization passes

w16-ir/src/compiler_to_bytecode.rs
    MIR -> W16 bytecode
```

MIR should be the main place for optimizations because it is low-level enough for analysis while still preserving the structure of the program.

### 4. Bytecode

Bytecode is the executable format of the W16 runtime.

It lives in:

```text
w16-core/src/bytecode.rs
```

W16 bytecode uses a register-based model. Each instruction has a fixed size of 4 bytes:

```text
| opcode | a | b | c |
```

Where:

```text
opcode - operation
a      - usually the destination register
b      - first operand
c      - second operand or part of an immediate value
```

Large values are stored in the constant pool, and instructions contain an index into that pool. This keeps the instruction format compact.

Bytecode should not know about HIR, MIR, or the original source language. It is the lower portable execution layer.

### 5. Runtime

The runtime lives in:

```text
w16-core/src/interpreter/
w16-core/src/jit/
```

It can execute bytecode in two ways:

```text
bytecode -> VM
bytecode -> JIT -> native code in memory
```

The VM is the main stable execution path. The JIT is a faster but more complex path based on Cranelift.

The public runtime functions are in `w16-core/src/lib.rs`:

```rust
run(bytecode, memory_size)
run_by_jit(bytecode)
```

## Crate Navigation

## w16-core

`w16-core` is the lowest W16 layer. It should not depend on HIR, MIR, CLI, or concrete languages.

It owns:

- bytecode format;
- opcode list;
- constant pool;
- virtual machine;
- VM errors;
- JIT compilation of bytecode;
- macros for creating instructions in tests.

Important files:

```text
w16-core/src/bytecode.rs
    OpCode, Instruction, Bytecode, ConstantPool.

w16-core/src/interpreter/vm.rs
    Register VM implementation.

w16-core/src/jit/jit_compiler.rs
    Cranelift JIT backend.

w16-core/src/lib.rs
    Public crate API.

w16-core/tests/
    VM and JIT tests.

w16-core/benches/
    VM and JIT benchmarks.
```

Responsibility boundary:

```text
w16-core accepts bytecode only.
w16-core does not parse HIR.
w16-core does not know about C, WCC, or other languages.
w16-core should not contain middle-end optimizations.
```

If you need to add a new opcode, you usually need to update:

```text
w16-core/src/bytecode.rs
w16-core/src/interpreter/vm.rs
w16-core/src/jit/jit_compiler.rs
w16-ir/src/compiler_to_bytecode.rs
VM/JIT/IR tests
```

## w16-ir

`w16-ir` is the compiler middle-end of the project.

It owns:

- tokenization of textual HIR;
- HIR parsing;
- HIR AST;
- HIR semantic checks;
- MIR AST;
- lowering from HIR to MIR;
- MIR verifier;
- MIR analyses;
- MIR optimizations;
- compilation from MIR to bytecode.

Important files:

```text
w16-ir/src/lexer/
    Textual HIR lexer.

w16-ir/src/parser/
    Textual HIR parser.

w16-ir/src/hir.rs
    HIR structures.

w16-ir/src/semantic/
    HIR checks.

w16-ir/src/mir.rs
    MIR structures.

w16-ir/src/translator/lowerer.rs
    HIR -> MIR translation.

w16-ir/src/mir_f/
    MIR verifier, analyses, and optimizations.

w16-ir/src/compiler_to_bytecode.rs
    MIR -> bytecode translation.

w16-ir/docs/IR_SYNTAX.md
    HIR/MIR syntax documentation.

w16-ir/tests/
    HIR pipeline and optimizer tests.
```

Responsibility boundary:

```text
w16-ir should not execute programs by itself.
w16-ir can create bytecode, but execution belongs to w16-core.
w16-ir should not depend on CLI.
w16-ir should not depend on WCC.
```

The ideal internal `w16-ir` pipeline is:

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

Optimization rule: every optimization pass must preserve MIR correctness. If a pass changes CFG, values, types, or terminators, it is useful to run the verifier after it, at least in debug/test mode.

## w16-lib

`w16-lib` is the convenient public facade API.

It exists for users who do not want to assemble the pipeline manually from `w16-ir` and `w16-core`.

It owns:

- running HIR from text;
- running HIR from tokens;
- running HIR AST;
- running MIR AST;
- running ready bytecode;
- selecting the execution mode: VM or JIT;
- one high-level error type;
- the convenient `W16` builder.

Important file:

```text
w16-lib/src/lib.rs
```

Example role:

```rust
let result = w16_lib::run_hir_text(source)?;
```

Internally this becomes:

```text
HIR text -> HIR AST -> MIR -> bytecode -> VM/JIT
```

Responsibility boundary:

```text
w16-lib does not implement its own compiler backend.
w16-lib should not contain a separate VM.
w16-lib should not include AOT as a stable API.
w16-lib should be the most convenient entry point for library users.
```

`w16-lib` is the crate that should be highlighted in the README, crates.io pages, and examples.

## w16-cli

`w16-cli` is the command-line interface for end users.

It owns:

- argument parsing;
- help output;
- running HIR files;
- choosing VM/JIT mode;
- debug dumps of internal stages;
- execution timing.

Important files:

```text
w16-cli/src/main.rs
    Entry point.

w16-cli/src/cmd.rs
    Command model and command registry for help output.

w16-cli/src/parser.rs
    CLI argument parsing.

w16-cli/src/executer.rs
    Execution of parsed commands.

w16-cli/src/help.rs
    Help formatting.

w16-cli/src/error.rs
    CLI errors.
```

CLI should stay thin. It should not implement its own compiler, optimizer, or runtime. Its job is to take user arguments and call `w16-lib`, `w16-ir`, `w16-core`, or experimental backends through a clear interface.

Top-level commands should remain simple:

```text
w16 run <file>
w16 build <file>
w16 dbg <stage> <file>
w16 version
w16 help
```

If WCC support is added to the CLI, it is better to keep it in a separate command group:

```text
w16 cc check <file.c>
w16 cc run <file.c>
w16 cc emit-hir <file.c>
```

This keeps the HIR path and the C path separate.

## w16c

`w16c` is the experimental AOT backend.

It owns:

- compiling W16 bytecode into an object file through Cranelift;
- attempting to build an executable through a system linker;
- platform-specific linking details.

Important files:

```text
w16c/src/lib.rs
w16c/src/dotobj/mod.rs
w16c/README.md
```

Status:

> experimental

Reason: AOT requires an external linker and platform conventions. On Windows this is usually `link.exe` from Visual Studio/MSVC. On Linux/macOS this usually means `cc` or another system linker driver.

Responsibility boundary:

```text
w16c should not be required to run W16 programs.
w16-lib should not depend on w16c as a stable runtime.
w16-cli may expose AOT commands only when they are explicitly supported by the environment.
```

## w16-langs/w16-cc

`w16-cc`, or WCC, is an experimental C frontend over W16.

It lives outside the root workspace:

```text
w16-langs/w16-cc
```

It owns:

- lexer for a C-like language;
- parser;
- semantic checker;
- symbol table;
- diagnostics;
- C AST;
- C AST -> W16 HIR translation;
- execution through W16 runtime;
- support for some C types and values, including F80.

Important files:

```text
w16-langs/w16-cc/src/frontend/lexer/
    Tokens and lexer.

w16-langs/w16-cc/src/frontend/parser/
    AST and parser.

w16-langs/w16-cc/src/frontend/semantic/
    Name, type, and symbol checks.

w16-langs/w16-cc/src/codegen/
    AST -> W16 HIR translation.

w16-langs/w16-cc/src/value/f80.rs
    80-bit floating-point value for long double.

w16-langs/w16-cc/tests/
    Lexer/parser/codegen/F80 tests.
```

WCC is important as a showcase: it demonstrates that W16 can be used as a backend for a separate language.

Responsibility boundary:

```text
WCC may depend on w16-lib.
WCC should not be a required part of w16-core.
WCC should not break HIR/MIR API stability for internal convenience.
```

## Main Pipelines

## Running HIR Through CLI

```text
user
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
w16-core: VM or JIT
```

## Running HIR Through the Library

```text
Rust application
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

This should be the most stable path for external users.

## Running a Language Frontend

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

This path shows how other languages can use W16.

## Debug Pipeline

Debug commands should help expose internal stages:

```text
tokens
HIR
MIR before optimization
MIR after optimization
bytecode
full pipeline
```

This is important for contributors: if a program behaves incorrectly, the pipeline stage that introduced the issue can be found quickly.

## Stability Boundaries

Not all parts of the project are equally stable.

### More Stable

```text
w16-core bytecode model
w16-core VM
w16-ir HIR parser/semantic path
w16-lib facade API
```

These parts should be changed carefully, with tests and a clear migration path.

### Actively Evolving

```text
w16-ir MIR
w16-ir optimizer
w16-cli commands
w16-langs/w16-cc
```

API changes are acceptable here if they improve the architecture and are covered by tests.

### Experimental

```text
w16-core JIT
w16c AOT
native object/executable generation
```

These parts may be fast and interesting, but they should not block the main VM execution path.

## Contributor Rules

## If You Change an Opcode

Check:

```text
w16-core/src/bytecode.rs
w16-core/src/interpreter/vm.rs
w16-core/src/jit/jit_compiler.rs
w16-ir/src/compiler_to_bytecode.rs
w16-core/tests/
w16-ir/tests/
```

A new opcode should have:

- clear semantics;
- a VM test;
- a JIT test if possible;
- MIR -> bytecode support if the compiler emits it.

## If You Change HIR

Check:

```text
w16-ir/src/hir.rs
w16-ir/src/parser/
w16-ir/src/semantic/
w16-ir/src/translator/lowerer.rs
w16-ir/tests/hir_pipeline_tests.rs
w16-ir/docs/IR_SYNTAX.md
```

HIR should remain convenient for frontends. If a change makes HIR generation from external languages harder, consider moving that complexity into the lowerer or MIR instead.

## If You Change MIR

Check:

```text
w16-ir/src/mir.rs
w16-ir/src/translator/lowerer.rs
w16-ir/src/mir_f/mir_verify/
w16-ir/src/mir_f/mir_optimizer/
w16-ir/src/compiler_to_bytecode.rs
w16-ir/tests/optimizer_tests.rs
```

MIR must preserve its invariants:

- every `ValueId` is defined correctly;
- operand types match their operations;
- every basic block has a terminator;
- jumps point to existing blocks;
- jump arguments match the target block parameters;
- optimizations do not leave IR in an invalid state.

## If You Change CLI

Check:

```text
w16-cli/src/cmd.rs
w16-cli/src/parser.rs
w16-cli/src/executer.rs
w16-cli/src/help.rs
```

CLI should remain predictable. Users should not need to know the internal architecture to run a file or inspect a debug stage.

## If You Change WCC

Check:

```text
w16-langs/w16-cc/src/frontend/
w16-langs/w16-cc/src/codegen/
w16-langs/w16-cc/tests/
```

WCC should generate correct W16 HIR. If a C construct is unsupported, it is better to emit a clear error than to generate incorrect HIR.

## Testing

Main root workspace tests:

```bash
cargo test
```

Experimental language tests:

```bash
cd w16-langs
cargo test
```

Runtime benchmarks:

```bash
cargo bench -p w16-core
```

Before a PR, it is recommended to run:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets
cargo test
```

And separately:

```bash
cd w16-langs
cargo test
```

## How to Read the Project for the First Time

If you are opening W16 for the first time, a good reading order is:

1. `README.md` or `README_EN.md` - general project overview.
2. `ARCHITECTURE.md` - architecture map.
3. `w16-lib/src/lib.rs` - convenient public API.
4. `w16-ir/docs/IR_SYNTAX.md` - HIR/MIR syntax.
5. `w16-ir/src/hir.rs` - HIR model.
6. `w16-ir/src/mir.rs` - MIR model.
7. `w16-core/src/bytecode.rs` - bytecode format.
8. `w16-core/src/interpreter/vm.rs` - bytecode execution.
9. `w16-langs/w16-cc/src/codegen/` - an example frontend that generates W16 HIR.

## What Counts as a Good Contribution

A good W16 contribution usually does one of three things:

- improves correctness;
- improves clarity;
- improves speed without losing correctness.

Examples of good tasks:

- a new HIR/MIR pipeline test;
- better diagnostics with spans;
- a small MIR optimization with a verifier test;
- support for a new C construct in WCC;
- a fix for a VM/JIT behavior mismatch;
- documentation with a runnable example;
- clearer CLI help.

## What Is Better to Avoid for Now

While the project is still early, it is better not to make the core more complex without a clear need:

- do not add large language features directly into `w16-core`;
- do not make `w16-core` depend on WCC or CLI;
- do not treat AOT as the stable main path yet;
- do not add a MIR optimization without a correctness test;
- do not change the bytecode format without a clear reason and a migration plan.

## Key Architectural Idea

W16 should remain a set of layers, not one large compiler:

```text
frontends -> HIR -> MIR -> bytecode -> runtime
```

If this separation is preserved, the project can evolve in several directions at once:

- improving the VM;
- developing the JIT;
- writing new languages on top of W16;
- stabilizing `w16-lib`;
- experimenting with AOT;
- adding MIR optimizations.

This is what makes W16 interesting for contributors: they can choose a small area and be useful without understanding the entire project down to the last byte.

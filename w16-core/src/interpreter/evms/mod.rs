//! # Experimental Virtual Machines
//!
//! Здесь находятся экспериментальные VM W16. Они могут быть быстрее обычной VM,
//! но не обязаны сохранять такой же баланс безопасности, диагностики и
//! стабильности API.
//!
//! Текущие EVM:
//!
//! - [`orca_evm`] - максимально быстрый unchecked interpreter для доверенного
//!   bytecode.

pub mod orca_evm;

pub use orca_evm::OrcaEvm;

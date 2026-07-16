//! foma: a finite-state toolkit and library — Rust port.
//!
//! Wave-2 literal (bug-for-bug) port of the C foma library. See
//! docs/port/rust-conventions.md for the binding conventions. Modules
//! mirror the C source files one-to-one and are added as each Wave-2
//! concern lands.

pub mod apply;
pub mod coaccessible;
pub mod constructions;
pub mod define;
pub mod determinize;
pub mod dynarray;
pub mod error;
pub mod extract;
pub mod flags;
pub mod iface;
pub mod int_stack;
pub mod io;
pub mod lexcread;
pub mod mem;
pub mod minimize;
pub mod options;
pub mod regex;
pub mod reverse;
pub mod rewrite;
pub mod session;
pub mod sigma;
pub mod spelling;
pub mod stack;
pub mod stringhash;
pub mod structures;
pub mod topsort;
pub mod trie;
pub mod types;
pub mod utf8;

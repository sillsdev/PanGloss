//! CLI session state — the re-entrant home for the interactive front-end state
//! that foma's C source kept in file-static globals.
//!
//! A `Session` owns the interactive command stack: the sentinel-terminated
//! doubly-linked arena that `stack.rs` manipulates through an `impl Session`
//! block. Threading `&mut Session` through the `iface` command layer replaces the
//! former `MAIN_STACK` / `ARENA` thread_locals, so independent sessions can
//! coexist on one thread (embeddable) with nothing hidden shared between them.
//!
use crate::define::{defined_functions_init, defined_networks_init};
use crate::dynarray::Lcg;
use crate::options::FomaOptions;
use crate::types::{DefinedFunctions, DefinedNetworks, StackEntry};

/// The mutable state of one interactive foma session.
pub struct Session {
    /// C: `struct stack_entry *main_stack` (the network-stack list head) as an
    /// arena index. `Some` after `new()`; the `stack_*` methods keep it valid.
    /// See `stack.rs` for the arena/sentinel representation.
    pub(crate) stack_head: Option<usize>,
    /// Arena backing the malloc'd `stack_entry` nodes (see `stack.rs`).
    pub(crate) stack_arena: Vec<StackEntry>,
    /// The session's option set (C: the `g_*` globals; CLI `set variable`).
    pub opts: FomaOptions,
    /// The defined-networks registry (C: `struct defined_networks *g_defines`,
    /// init'd by main). Always the dummy-head list `define.rs` operates on.
    pub defines: Box<DefinedNetworks>,
    /// The defined-functions registry (C: `g_defines_f`), same lifecycle.
    pub defines_f: Box<DefinedFunctions>,
    /// The C library `rand()` state that `stack_add` reads to name unnamed
    /// nets. C used the process-global libc state (default-seeded unless an
    /// apply_init had run); the session owns its own here.
    pub(crate) lcg: Lcg,
}

impl Session {
    /// Create a session with a freshly-initialised, empty command stack and
    /// default options.
    pub fn new() -> Session {
        let mut session = Session {
            stack_head: None,
            stack_arena: Vec::new(),
            opts: FomaOptions::default(),
            defines: defined_networks_init(),
            defines_f: defined_functions_init(),
            lcg: Lcg::new(),
        };
        session.stack_init();
        session
    }
}

impl Default for Session {
    fn default() -> Self {
        Session::new()
    }
}

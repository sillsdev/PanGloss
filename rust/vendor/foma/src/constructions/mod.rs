//! foma/constructions.c — literal Wave-2 (bug-for-bug) port per
//! docs/port/rust-conventions.md. Sem rules:
//! docs/spec/port/foma/constructions.md (per-file ids) plus the fomalib.h /
//! fomalibconf.h prototype ids.
//!
//! Slice 1: infrastructure (merge-sigma, state pointers, triplet hash)
//! and the product/regular constructions. Slice 2 (from fsm_escape down):
//! the elementary machines, derived regex operators, substitutions and the
//! remaining constructions.
//!
//! Interior pointers of the C (state_arr.transitions, the outarray/index
//! tails in fsm_compose) are represented as indices per the conventions.
//! The worklist is the global int stack; state numbering comes from the
//! triplet hash (keys are consecutive ints in insertion order).

pub(crate) use crate::coaccessible::fsm_coaccessible;
pub(crate) use crate::determinize::fsm_determinize;
pub(crate) use crate::dynarray::{
    fsm_construct_add_arc, fsm_construct_add_arc_nums, fsm_construct_copy_sigma,
    fsm_construct_done, fsm_construct_init, fsm_construct_set_final, fsm_construct_set_initial,
    fsm_get_arc_in, fsm_get_arc_num_in, fsm_get_arc_num_out, fsm_get_arc_out, fsm_get_arc_source,
    fsm_get_arc_target, fsm_get_next_arc, fsm_get_next_final, fsm_get_next_state,
    fsm_get_next_state_arc, fsm_get_num_states, fsm_get_symbol_number, fsm_read_done,
    fsm_read_init, fsm_read_is_final, fsm_read_reset, fsm_state_add_arc, fsm_state_close,
    fsm_state_end_state, fsm_state_init, fsm_state_set_current_state,
};
pub(crate) use crate::extract::{fsm_lower, fsm_upper};
pub(crate) use crate::flags::flag_check;
pub(crate) use crate::int_stack::IntStack;
pub(crate) use crate::minimize::fsm_minimize;
pub(crate) use crate::options::FomaOptions;
pub(crate) use crate::rewrite::fsm_clear_contexts;
pub(crate) use crate::sigma::{
    sigma_add, sigma_add_special, sigma_cleanup, sigma_find, sigma_find_number, sigma_max,
    sigma_remove, sigma_size, sigma_sort, sigma_substitute,
};
pub(crate) use crate::structures::{
    find_arccount, fsm_copy, fsm_create, fsm_destroy, fsm_empty_set, fsm_empty_string,
    fsm_identity, fsm_isempty, fsm_sigma_destroy, fsm_sigma_pairs_net,
};
pub(crate) use crate::topsort::fsm_topsort;
pub(crate) use crate::types::{
    EPSILON, Fsm, FsmState, Fsmcontexts, IDENTITY, M_LOWER, M_UPPER, NO, OP_IGNORE_ALL,
    OP_IGNORE_INTERNAL, PATHCOUNT_CYCLIC, PATHCOUNT_UNKNOWN, Sigma, UNK, UNKNOWN, YES,
};

/* C: #define KLEENE_STAR 0 / KLEENE_PLUS 1 / OPTIONALITY 2 and
#define COMPLEMENT 0 / COMPLETE 1 — file-local constants, no spec ids */

mod boolean;
mod closure;
mod derived;
mod helpers;
mod merge_sigma;
mod products;
mod triplet_hash;

pub use boolean::*;
pub use closure::*;
pub use derived::*;
pub use helpers::*;
pub use merge_sigma::*;
pub use products::*;
pub use triplet_hash::*;

#[cfg(test)]
mod tests;

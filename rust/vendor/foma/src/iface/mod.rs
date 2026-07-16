//! foma/iface.c — Wave-4 idiomatization split by command family (mirrors the
//! constructions/ split). The monolithic iface.rs was divided into submodules;
//! this mod re-exports the full public surface so `crate::iface::iface_*` (and
//! `crate::iface::print_stats`, `crate::iface::foma_net_print`) keep resolving
//! for the bins and for stack.rs. Cross-module and external names reach the
//! submodules through their `use super::*;` (the `pub(crate) use` re-exports
//! below). Sem rules: docs/spec/port/foma/iface.md (per-file `iface.*` ids)
//! plus the foma.h prototype ids (`foma.iface-*`) carried at each Rust site.

pub(crate) use std::cell::Cell;
pub(crate) use std::fs::File;
pub(crate) use std::io::{BufRead, BufReader, Write};

pub(crate) use flate2::Compression;
pub(crate) use flate2::write::GzEncoder;

pub(crate) use crate::apply::{
    apply_clear, apply_down, apply_init, apply_last_pairs, apply_lower_words, apply_random_lower,
    apply_random_upper, apply_random_words, apply_reset_enumerator, apply_set_collect_pairs,
    apply_set_obey_flags, apply_set_print_pairs, apply_set_print_space, apply_set_show_flags,
    apply_up, apply_upper_words, apply_words,
};
pub(crate) use crate::coaccessible::fsm_coaccessible;
pub(crate) use crate::constructions::{
    fsm_bimachine, fsm_close_sigma, fsm_compact, fsm_complement, fsm_complete, fsm_compose,
    fsm_concat, fsm_count, fsm_cross_product, fsm_equivalent, fsm_ignore, fsm_intersect,
    fsm_invert, fsm_kleene_plus, fsm_kleene_star, fsm_letter_machine, fsm_minus, fsm_sequentialize,
    fsm_shuffle, fsm_substitute_label, fsm_substitute_symbol, fsm_symbol, fsm_symbol_occurs,
    fsm_union,
};
pub(crate) use crate::define::{find_defined, remove_defined};
pub(crate) use crate::determinize::fsm_determinize;
pub(crate) use crate::extract::{fsm_lower, fsm_upper};
pub(crate) use crate::flags::{flag_eliminate, flag_twosided};
pub(crate) use crate::io::{
    Output, foma_write_prolog, fsm_read_binary_file_multiple, fsm_read_binary_file_multiple_init,
    fsm_read_prolog, fsm_read_spaced_text_file, fsm_read_text_file, load_defined, net_print_att,
    read_att, save_defined,
};
pub(crate) use crate::minimize::fsm_minimize;
pub(crate) use crate::options::FomaOptions;
pub(crate) use crate::reverse::fsm_reverse;
pub(crate) use crate::session::Session;
pub(crate) use crate::sigma::sigma_sort;
pub(crate) use crate::spelling::{
    apply_med, apply_med_get_cost, apply_med_get_instring, apply_med_set_heap_max,
    apply_med_set_med_cutoff, apply_med_set_med_limit, cmatrix_print, cmatrix_print_att,
};
pub(crate) use crate::structures::{
    fsm_copy, fsm_destroy, fsm_extract_ambiguous, fsm_extract_ambiguous_domain,
    fsm_extract_unambiguous, fsm_identity, fsm_isempty, fsm_isfunctional, fsm_isidentity,
    fsm_issequential, fsm_isunambiguous, fsm_sigma_net, fsm_sigma_pairs_net, fsm_sort_arcs,
};
pub(crate) use crate::topsort::fsm_topsort;
pub(crate) use crate::types::{
    AP_D, AP_U, ApplyHandle, EPSILON, Fsm, IDENTITY, M_LOWER, M_UPPER, OP_IGNORE_ALL,
    PATHCOUNT_CYCLIC, Sigma, UNKNOWN, YES,
};
pub(crate) use crate::utf8::{dequote_string, escape_string};

mod apply_cmds;
mod binary;
mod common;
mod io_cmds;
mod print;
mod stack_ops;
mod tests_cmds;
mod unary;
mod variables;

pub use apply_cmds::*;
pub use binary::*;
pub use common::*;
pub use io_cmds::*;
pub use print::*;
pub use stack_ops::*;
pub use tests_cmds::*;
pub use unary::*;
pub use variables::*;

#[cfg(test)]
mod tests;

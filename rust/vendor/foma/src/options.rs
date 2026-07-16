//! Interpreter/library options — the re-entrant home for the tunables foma's C
//! source kept as `g_*` globals (mem.c / foma.h) and exposed through the CLI's
//! `set variable` table (iface.c `global_vars[]`).
//!
//! A `FomaOptions` value is owned by each CLI `Session` (`session.opts`) and
//! passed by shared reference into the library functions whose behaviour it
//! tunes (`fsm_minimize`, `fsm_compose`, the lexc compiler, AT&T I/O, flag
//! elimination, ...). Nothing is hidden in thread-local state, so two sessions
//! on one thread can run with different settings.
//!
//! Field names follow the C globals minus the `g_` prefix; the CLI-visible
//! variable names ("hopcroft-min", "att-epsilon", ...) live in
//! `iface/variables.rs`. Booleans are `bool` here (C used 0/1 ints); the C
//! defaults are preserved exactly.

use smol_str::SmolStr;

/// The option set of one foma session (C: the `g_*` globals).
#[derive(Debug, Clone, PartialEq)]
pub struct FomaOptions {
    /// C `g_show_flags` — CLI "show-flags": print flag symbols in apply output.
    pub show_flags: bool,
    /// C `g_obey_flags` — CLI "obey-flags": enforce flag diacritics in apply.
    pub obey_flags: bool,
    /// C `g_flag_is_epsilon` — CLI "flag-is-epsilon": flags act as epsilon in
    /// composition.
    pub flag_is_epsilon: bool,
    /// C `g_print_space` — CLI "print-space": space between symbols in apply
    /// output.
    pub print_space: bool,
    /// C `g_print_pairs` — CLI "print-pairs": print both sides of each arc pair.
    pub print_pairs: bool,
    /// C `g_minimal` — CLI "minimal": `fsm_minimize` actually minimizes.
    pub minimal: bool,
    /// C `g_name_nets` — CLI "name-nets": name result nets after the regex.
    pub name_nets: bool,
    /// C `g_print_sigma` — CLI "print-sigma": print the sigma after net ops.
    pub print_sigma: bool,
    /// C `g_quit_on_fail` — CLI "quit-on-fail": abort scripts on failed tests.
    pub quit_on_fail: bool,
    /// C `g_quote_special` — CLI "quote-special": quote special characters in
    /// printed words.
    pub quote_special: bool,
    /// C `g_recursive_define` — CLI "recursive-define": allow a define to
    /// reference itself.
    pub recursive_define: bool,
    /// C `g_sort_arcs` — CLI "sort-arcs": keep arcs sorted (vestigial: settable,
    /// never read — as in C where only fsm_sort_arcs callers consulted it).
    pub sort_arcs: bool,
    /// C `g_verbose` — CLI "verbose": progress/statistics chatter.
    pub verbose: bool,
    /// C `g_minimize_hopcroft` — CLI "hopcroft-min": Hopcroft vs Brzozowski
    /// minimization.
    pub minimize_hopcroft: bool,
    /// C `g_compose_tristate` — CLI "compose-tristate": tristate composition
    /// filter.
    pub compose_tristate: bool,
    /// C `g_list_limit` — CLI "limit": max words printed by words/pairs.
    pub list_limit: i32,
    /// C `g_list_random_limit` — CLI "random-limit": max words printed by the
    /// random-* commands.
    pub list_random_limit: i32,
    /// C `g_med_limit` — CLI "med-limit": max matches in med search.
    pub med_limit: i32,
    /// C `g_med_cutoff` — CLI "med-cutoff": max edit distance in med search.
    pub med_cutoff: i32,
    /// C `g_lexc_align` — CLI "lexc-align": align lexc entry symbol pairs.
    pub lexc_align: bool,
    /// C `char *g_att_epsilon = "@0@"` — CLI "att-epsilon": the epsilon symbol
    /// used when reading/writing AT&T format.
    pub att_epsilon: SmolStr,
    /// C: `struct _fsm_options fsm_options` (foma.h), the library option set
    /// behind fsm_set_option/fsm_get_option — one field, folded in here.
    // [spec:foma:def:foma.fsm-options]
    pub skip_word_boundary_marker: bool,
}

impl Default for FomaOptions {
    fn default() -> FomaOptions {
        FomaOptions {
            show_flags: false,
            obey_flags: true,
            flag_is_epsilon: false,
            print_space: false,
            print_pairs: false,
            minimal: true,
            name_nets: false,
            print_sigma: true,
            quit_on_fail: true,
            quote_special: false,
            recursive_define: false,
            sort_arcs: true,
            verbose: true,
            minimize_hopcroft: true,
            compose_tristate: false,
            list_limit: 100,
            list_random_limit: 15,
            med_limit: 3,
            med_cutoff: 15,
            lexc_align: false,
            att_epsilon: SmolStr::new("@0@"),
            skip_word_boundary_marker: false,
        }
    }
}

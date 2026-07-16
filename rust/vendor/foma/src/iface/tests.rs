//! foma/iface.c Wave-4 split: the iface unit tests. `use super::*`
//! resolves to the iface module (re-exports every submodule's surface),
//! so every `/test` facet keeps a stable home here.
use super::*;
use crate::define::add_defined;
use crate::options::FomaOptions;
use crate::regex::fsm_parse_regex;
use crate::session::Session;
use crate::types::Fsm;

/// Push a compiled regex onto the CLI stack (fixture).
fn push(session: &mut Session, re: &str) {
    session.stack_add(fsm_parse_regex(&session.opts, re, None, None).unwrap());
}

/// Push a topsorted regex (fixture). fsm_count leaves pathcount untouched, so
/// a cyclic net only gets pathcount == PATHCOUNT_CYCLIC after fsm_topsort (the
/// REPL topsorts regex results; a bare fsm_parse_regex leaves it UNKNOWN).
fn push_topsorted(session: &mut Session, re: &str) {
    session.stack_add(fsm_topsort(
        fsm_parse_regex(&session.opts, re, None, None).unwrap(),
    ));
}

/// Pop the top net and test recognizer-language equality against `re`.
fn top_is(session: &mut Session, re: &str) -> bool {
    let expected = fsm_parse_regex(&session.opts, re, None, None).unwrap();
    let popped = session.stack_pop().unwrap();
    fsm_equivalent(&session.opts, popped, expected)
}

/// Does `net` accept `w` on the input (down) side?
fn accepts_down(net: &Fsm, w: &str) -> bool {
    let mut h = apply_init(net);
    let r = apply_down(&mut h, Some(w));
    apply_clear(h);
    r.is_some()
}

/// First apply-down output for input `w` (transducer image).
fn down1(net: &Fsm, w: &str) -> Option<String> {
    let mut h = apply_init(net);
    let r = apply_down(&mut h, Some(w));
    apply_clear(h);
    r
}

/// First apply-up output for input `w`.
fn up1(net: &Fsm, w: &str) -> Option<String> {
    let mut h = apply_init(net);
    let r = apply_up(&mut h, Some(w));
    apply_clear(h);
    r
}

// Print-only help family: byte-exact output is integration-tested; here we
// pin the return type (unit) and non-panic on empty and populated states.
// [spec:foma:sem:iface.iface-help-fn/test]
// [spec:foma:sem:foma.iface-help-fn/test]
// [spec:foma:sem:iface.iface-apropos-fn/test]
// [spec:foma:sem:foma.iface-apropos-fn/test]
// [spec:foma:sem:iface.iface-help-search-fn/test]
// [spec:foma:sem:foma.iface-help-search-fn/test]
// [spec:foma:sem:iface.iface-print-bool-fn/test]
// [spec:foma:sem:iface.iface-warranty-fn/test]
// [spec:foma:sem:foma.iface-warranty-fn/test]
#[test]
fn help_family_prints_without_touching_the_stack() {
    let session = Session::new();
    iface_help();
    iface_apropos("net"); // some matches
    iface_apropos("\u{1}zznomatchzz"); // no match → prints nothing
    iface_help_search("compose"); // matching entry
    iface_help_search("zznomatchzz"); // no match
    iface_print_bool(true);
    iface_print_bool(false);
    iface_warranty();
    // None of these read or write the stack.
    assert_eq!(session.stack_size(), 0);
}

// foma_net_print is re-exported from io.c; verify the re-export reaches the
// real serializer (returns 1, writes the foma save-format frame). Byte-exact
// content is covered by io.rs's own tests.
// [spec:foma:sem:iface.foma-net-print-fn/test]
#[test]
fn foma_net_print_writes_save_format() {
    let opts = &FomaOptions::default();
    let net = fsm_parse_regex(opts, "a", None, None).unwrap();
    let mut buf: Vec<u8> = Vec::new();
    foma_net_print(&net, &mut buf).expect("writing net to in-memory buffer");
    let s = String::from_utf8_lossy(&buf);
    assert!(
        s.starts_with("##foma-net 1.0##"),
        "got: {:?}",
        &s[..s.len().min(40)]
    );
    assert!(s.contains("##sigma##"));
    assert!(s.trim_end().ends_with("##end##"));
}

// iface_apply_set_params copies the four apply options onto the handle in
// order: print_space, print_pairs, show_flags, obey_flags.
// [spec:foma:sem:iface.iface-apply-set-params-fn/test]
// [spec:foma:sem:foma.iface-apply-set-params-fn/test]
#[test]
fn apply_set_params_copies_the_four_globals() {
    let opts = &FomaOptions::default();
    let net = fsm_parse_regex(opts, "a", None, None).unwrap();
    let mut h = apply_init(&net);
    let set_opts = FomaOptions {
        print_space: true,
        print_pairs: true,
        show_flags: true,
        obey_flags: false,
        ..FomaOptions::default()
    };
    iface_apply_set_params(&set_opts, &mut h);
    assert_eq!(h.print_space, 1);
    assert_eq!(h.print_pairs, 1);
    assert_eq!(h.show_flags, 1);
    assert_eq!(h.obey_flags, 0);
    apply_clear(h);
}

// apply up/down via the iface entry points: net is NOT consumed (size
// unchanged), and the empty-stack guard makes it a no-op.
// [spec:foma:sem:iface.iface-apply-down-fn/test]
// [spec:foma:sem:foma.iface-apply-down-fn/test]
// [spec:foma:sem:iface.iface-apply-up-fn/test]
// [spec:foma:sem:foma.iface-apply-up-fn/test]
#[test]
fn apply_down_and_up_keep_the_net_and_refuse_empty_stack() {
    let mut session = Session::new();
    // Empty stack: iface_stack_check(1) refuses; nothing pushed/popped.
    iface_apply_down(&mut session, "a");
    iface_apply_up(&mut session, "a");
    assert_eq!(session.stack_size(), 0);
    push(&mut session, "a:b"); // transducer a -> b
    iface_apply_down(&mut session, "a"); // prints "b"; net not consumed
    assert_eq!(session.stack_size(), 1);
    iface_apply_up(&mut session, "b"); // prints "a"; net not consumed
    assert_eq!(session.stack_size(), 1);
    // The net is unchanged and still maps a -> b.
    assert_eq!(
        down1(&session.stack_pop().unwrap(), "a"),
        Some("b".to_string())
    );
}

// apply med: configures the cached med handle (heap max 4194305, med-limit
// g_med_limit=3, med-cutoff g_med_cutoff=15) and does not consume the net;
// empty stack refuses.
// [spec:foma:sem:iface.iface-apply-med-fn/test]
// [spec:foma:sem:foma.iface-apply-med-fn/test]
#[test]
fn apply_med_configures_handle_and_refuses_empty_stack() {
    let mut session = Session::new();
    iface_apply_med(&mut session, "cat"); // empty stack: no-op
    assert_eq!(session.stack_size(), 0);
    push(&mut session, "c a t");
    iface_apply_med(&mut session, "cat");
    assert_eq!(session.stack_size(), 1); // net not consumed
    let amedh = session.stack_get_med_ah().unwrap();
    let (limit, cutoff, heap) =
        session.stack_entry_amedh(amedh, |m| (m.med_limit, m.med_cutoff, m.med_max_heap_size));
    assert_eq!(limit, 3);
    assert_eq!(cutoff, 15);
    assert_eq!(heap, 4194304 + 1);
}

// apply-file: invalid direction returns 1; empty stack returns 0; a bad
// input path returns 1; a good run returns 0, writes results to the output
// file, and leaves the net on the stack.
// [spec:foma:sem:iface.iface-apply-file-fn/test]
// [spec:foma:sem:foma.iface-apply-file-fn/test]
#[test]
fn apply_file_direction_stack_and_roundtrip() {
    let dir = std::env::temp_dir();
    let inpath = dir.join("foma_s1_applyfile_in.txt");
    let outpath = dir.join("foma_s1_applyfile_out.txt");
    std::fs::write(&inpath, "cat\n").unwrap();

    let mut session = Session::new();
    // Invalid direction: rejected before the stack/file checks.
    assert!(!iface_apply_file(
        &mut session,
        inpath.to_str().unwrap(),
        None,
        0
    ));
    // Valid direction, empty stack: iface_stack_check(1) fails → handled (true).
    assert!(iface_apply_file(
        &mut session,
        inpath.to_str().unwrap(),
        None,
        AP_D
    ));

    push(&mut session, "c a t"); // acceptor for "cat" over sigma {c,a,t}
    // Bad input path with a populated stack: open failure → false.
    assert!(!iface_apply_file(
        &mut session,
        "/no/such/foma/path",
        None,
        AP_D
    ));
    // Good run writing to a file.
    let rc = iface_apply_file(
        &mut session,
        inpath.to_str().unwrap(),
        Some(outpath.to_str().unwrap()),
        AP_D,
    );
    assert!(rc);
    assert_eq!(session.stack_size(), 1); // net not consumed
    let out = std::fs::read_to_string(&outpath).unwrap();
    assert!(out.contains("cat"), "output was: {:?}", out);
}

// close sigma: pop + push topsort(minimize(close_sigma(net,0))); size
// unchanged, language preserved for a net with no unknown symbols. Refusal
// path leaves the (empty) stack unchanged.
// [spec:foma:sem:iface.iface-close-fn/test]
// [spec:foma:sem:foma.iface-close-fn/test]
#[test]
fn close_preserves_language_and_refuses_empty() {
    let mut session = Session::new();
    iface_close(&mut session);
    assert_eq!(session.stack_size(), 0);
    push(&mut session, "a | b");
    iface_close(&mut session);
    assert_eq!(session.stack_size(), 1);
    assert!(top_is(&mut session, "a | b"));
}

// compact sigma: mutates top in place then pop + push topsort(minimize);
// size unchanged, language preserved.
// [spec:foma:sem:iface.iface-compact-fn/test]
// [spec:foma:sem:foma.iface-compact-fn/test]
#[test]
fn compact_preserves_language() {
    let mut session = Session::new();
    push(&mut session, "a | b");
    iface_compact(&mut session);
    assert_eq!(session.stack_size(), 1);
    assert!(top_is(&mut session, "a | b"));
}

// complete net: pop + push fsm_complete(net). The accepted language is
// unchanged (the added sink state is non-final); size unchanged.
// [spec:foma:sem:iface.iface-complete-fn/test]
// [spec:foma:sem:foma.iface-complete-fn/test]
#[test]
fn complete_keeps_language_and_adds_no_words() {
    let mut session = Session::new();
    push(&mut session, "a");
    iface_complete(&mut session);
    assert_eq!(session.stack_size(), 1);
    let net = session.stack_pop().unwrap();
    assert!(accepts_down(&net, "a"));
    assert!(!accepts_down(&net, "aa"));
    assert!(!accepts_down(&net, ""));
}

// determinize net: pop + push fsm_determinize(net); no minimize/topsort.
// Size unchanged, language preserved.
// [spec:foma:sem:iface.iface-determinize-fn/test]
// [spec:foma:sem:foma.iface-determinize-fn/test]
#[test]
fn determinize_preserves_language() {
    let mut session = Session::new();
    push(&mut session, "a | b");
    iface_determinize(&mut session);
    assert_eq!(session.stack_size(), 1);
    assert!(top_is(&mut session, "a | b"));
}

// compose net: folds the ENTIRE stack down to one; the net nearer the top
// is the first (upper) composition operand. Refuses with <2 nets.
// [spec:foma:sem:iface.iface-compose-fn/test]
// [spec:foma:sem:foma.iface-compose-fn/test]
#[test]
fn compose_folds_whole_stack_and_top_is_upper_operand() {
    let mut session = Session::new();
    // <2 nets: refusal, stack unchanged.
    push(&mut session, "a");
    iface_compose(&mut session);
    assert_eq!(session.stack_size(), 1);
    // 3 identity nets fold to a single net (fold-until-one).
    push(&mut session, "a");
    push(&mut session, "a");
    assert_eq!(session.stack_size(), 3);
    iface_compose(&mut session);
    assert_eq!(session.stack_size(), 1);
    assert!(top_is(&mut session, "a"));
    // Pop order: compose(top, second) = (a:b) .o. (b:c) = a:c.
    session = Session::new();
    push(&mut session, "b:c"); // bottom / second operand
    push(&mut session, "a:b"); // top / first (upper) operand
    iface_compose(&mut session);
    assert_eq!(session.stack_size(), 1);
    assert_eq!(
        down1(&session.stack_pop().unwrap(), "a"),
        Some("c".to_string())
    );
}

// concatenate: folds the whole stack; the top net is the LEFT operand each
// step, so [a,b,c] (bottom->top) yields "cba". Refuses with <2 nets. Wave 4 fix:
// no stray "dd" is printed.
// [spec:foma:sem:iface.iface-conc-fn+1/test]
// [spec:foma:sem:foma.iface-conc-fn+1/test]
#[test]
fn concatenate_folds_with_top_as_left_operand() {
    let mut session = Session::new();
    push(&mut session, "a");
    iface_conc(&mut session); // <2 nets: refusal
    assert_eq!(session.stack_size(), 1);
    push(&mut session, "b");
    push(&mut session, "c"); // stack bottom->top: a, b, c
    assert_eq!(session.stack_size(), 3);
    iface_conc(&mut session);
    assert_eq!(session.stack_size(), 1);
    assert!(top_is(&mut session, "c b a"));
}

// crossproduct net: SINGLE step (does not fold); top is the upper operand.
// [spec:foma:sem:iface.iface-crossproduct-fn/test]
// [spec:foma:sem:foma.iface-crossproduct-fn/test]
#[test]
fn crossproduct_is_single_step_top_is_upper() {
    let mut session = Session::new();
    push(&mut session, "z"); // bottom, untouched
    push(&mut session, "b"); // second / lower operand
    push(&mut session, "c"); // top / upper operand
    iface_crossproduct(&mut session);
    // Only the top two were consumed and one pushed: 3 -> 2.
    assert_eq!(session.stack_size(), 2);
    assert_eq!(
        down1(&session.stack_pop().unwrap(), "c"),
        Some("b".to_string())
    );
    assert!(top_is(&mut session, "z")); // bottom net still present
}

// intersect net: folds the whole stack (commutative). Refuses with <2 nets.
// [spec:foma:sem:iface.iface-intersect-fn/test]
// [spec:foma:sem:foma.iface-intersect-fn/test]
#[test]
fn intersect_folds_whole_stack() {
    let mut session = Session::new();
    push(&mut session, "a | b");
    iface_intersect(&mut session); // <2 nets: refusal
    assert_eq!(session.stack_size(), 1);
    push(&mut session, "b | c");
    push(&mut session, "b"); // three sets whose intersection is {b}
    assert_eq!(session.stack_size(), 3);
    iface_intersect(&mut session);
    assert_eq!(session.stack_size(), 1);
    assert!(top_is(&mut session, "b"));
}

// ignore net: SINGLE step; the top net is the base language A in A/B.
// [spec:foma:sem:iface.iface-ignore-fn/test]
// [spec:foma:sem:foma.iface-ignore-fn/test]
#[test]
fn ignore_is_single_step_base_is_top() {
    let mut session = Session::new();
    push(&mut session, "z"); // bottom, untouched
    push(&mut session, "x"); // second: ignored material B
    push(&mut session, "a"); // top: base language A
    iface_ignore(&mut session);
    assert_eq!(session.stack_size(), 2); // single step 3 -> 2
    let net = session.stack_pop().unwrap();
    assert!(accepts_down(&net, "a")); // base A with zero B's
    assert!(!accepts_down(&net, "x")); // B alone is not in A/B
    assert!(top_is(&mut session, "z"));
}

// invert net: pop + push fsm_invert(net); swaps upper/lower sides.
// [spec:foma:sem:iface.iface-invert-fn/test]
// [spec:foma:sem:foma.iface-invert-fn/test]
#[test]
fn invert_swaps_sides() {
    let mut session = Session::new();
    push(&mut session, "a:b"); // maps a -> b
    iface_invert(&mut session);
    assert_eq!(session.stack_size(), 1);
    // Inverted net is b:a; apply up "a" (lower) yields "b" (upper).
    assert_eq!(
        up1(&session.stack_pop().unwrap(), "a"),
        Some("b".to_string())
    );
}

// lower-side net: pop + push topsort(minimize(fsm_lower(net))).
// [spec:foma:sem:iface.iface-lower-side-fn/test]
// [spec:foma:sem:foma.iface-lower-side-fn/test]
#[test]
fn lower_side_takes_lower_projection() {
    let mut session = Session::new();
    push(&mut session, "a:b");
    iface_lower_side(&mut session);
    assert_eq!(session.stack_size(), 1);
    assert!(top_is(&mut session, "b"));
}

// letter machine: pop + push topsort(minimize(fsm_letter_machine(net)));
// single-char symbols are unchanged in language.
// [spec:foma:sem:iface.iface-letter-machine-fn/test]
// [spec:foma:sem:foma.iface-letter-machine-fn/test]
#[test]
fn letter_machine_preserves_single_char_language() {
    let mut session = Session::new();
    push(&mut session, "a b");
    iface_letter_machine(&mut session);
    assert_eq!(session.stack_size(), 1);
    assert!(top_is(&mut session, "a b"));
}

// minimize net: forces the `minimal` option on for the op, then RESTORES the
// saved value; size unchanged, language preserved.
// [spec:foma:sem:iface.iface-minimize-fn/test]
// [spec:foma:sem:foma.iface-minimize-fn/test]
#[test]
fn minimize_restores_g_minimal_and_preserves_language() {
    let mut session = Session::new();
    session.opts.minimal = false; // user turned `minimal` OFF
    push(&mut session, "a | b");
    iface_minimize(&mut session);
    assert!(!session.opts.minimal); // restored to saved value
    assert_eq!(session.stack_size(), 1);
    assert!(top_is(&mut session, "a | b"));
}

// negate net: pop + push topsort(minimize(complement(net))); complement is
// over the net's own sigma.
// [spec:foma:sem:iface.iface-negate-fn/test]
// [spec:foma:sem:foma.iface-negate-fn/test]
#[test]
fn negate_complements_over_sigma() {
    let mut session = Session::new();
    push(&mut session, "a"); // sigma {a}
    iface_negate(&mut session);
    assert_eq!(session.stack_size(), 1);
    let net = session.stack_pop().unwrap();
    assert!(!accepts_down(&net, "a")); // "a" excluded
    assert!(accepts_down(&net, "")); // epsilon in complement
    assert!(accepts_down(&net, "aa")); // aa in complement
}

// one-plus net (Kleene plus): pop + push topsort(minimize(kleene_plus)).
// [spec:foma:sem:iface.iface-one-plus-fn/test]
// [spec:foma:sem:foma.iface-one-plus-fn/test]
#[test]
fn one_plus_is_kleene_plus() {
    let mut session = Session::new();
    push(&mut session, "a");
    iface_one_plus(&mut session);
    assert_eq!(session.stack_size(), 1);
    let net = session.stack_pop().unwrap();
    assert!(accepts_down(&net, "a"));
    assert!(accepts_down(&net, "aa"));
    assert!(!accepts_down(&net, "")); // plus requires >= 1
}

// eliminate flags / eliminate flag <name>: pop + push flag_eliminate(net,
// None|Some(name)); on a flagless net the language is preserved.
// [spec:foma:sem:iface.iface-eliminate-flags-fn/test]
// [spec:foma:sem:foma.iface-eliminate-flags-fn/test]
// [spec:foma:sem:iface.iface-eliminate-flag-fn/test]
// [spec:foma:sem:foma.iface-eliminate-flag-fn/test]
#[test]
fn eliminate_flags_and_flag_wire_to_flag_eliminate() {
    let mut session = Session::new();
    iface_eliminate_flags(&mut session); // empty: refusal
    iface_eliminate_flag(&mut session, "X"); // empty: refusal
    assert_eq!(session.stack_size(), 0);
    push(&mut session, "a b");
    iface_eliminate_flags(&mut session);
    assert_eq!(session.stack_size(), 1);
    assert!(top_is(&mut session, "a b"));
    push(&mut session, "a b");
    iface_eliminate_flag(&mut session, "X");
    assert_eq!(session.stack_size(), 1);
    assert!(top_is(&mut session, "a b"));
}

// extract ambiguous / unambiguous / ambiguous-upper (domain): each pops +
// pushes its extraction; size unchanged.
// [spec:foma:sem:iface.iface-extract-ambiguous-fn/test]
// [spec:foma:sem:foma.iface-extract-ambiguous-fn/test]
// [spec:foma:sem:iface.iface-extract-unambiguous-fn/test]
// [spec:foma:sem:foma.iface-extract-unambiguous-fn/test]
// [spec:foma:sem:iface.iface-ambiguous-upper-fn/test]
// [spec:foma:sem:foma.iface-ambiguous-upper-fn/test]
#[test]
fn extract_ambiguous_unambiguous_and_domain() {
    // extract ambiguous of an ambiguous transducer keeps its paths.
    let mut session = Session::new();
    push(&mut session, "a:b | a:c");
    iface_extract_ambiguous(&mut session);
    assert_eq!(session.stack_size(), 1);
    assert!(accepts_down(&session.stack_pop().unwrap(), "a"));
    // extract ambiguous of an unambiguous transducer is empty.
    push(&mut session, "a:b");
    iface_extract_ambiguous(&mut session);
    assert!(!accepts_down(&session.stack_pop().unwrap(), "a"));

    // extract unambiguous of an unambiguous transducer keeps it (a -> b).
    push(&mut session, "a:b");
    iface_extract_unambiguous(&mut session);
    assert_eq!(session.stack_size(), 1);
    assert_eq!(
        down1(&session.stack_pop().unwrap(), "a"),
        Some("b".to_string())
    );
    // extract unambiguous of an ambiguous transducer is empty.
    push(&mut session, "a:b | a:c");
    iface_extract_unambiguous(&mut session);
    assert!(!accepts_down(&session.stack_pop().unwrap(), "a"));

    // ambiguous upper = domain of ambiguous inputs.
    push(&mut session, "a:b | a:c");
    iface_ambiguous_upper(&mut session);
    assert_eq!(session.stack_size(), 1);
    assert!(accepts_down(&session.stack_pop().unwrap(), "a"));
    push(&mut session, "a:b"); // unambiguous → empty domain
    iface_ambiguous_upper(&mut session);
    assert!(!accepts_down(&session.stack_pop().unwrap(), "a"));
}

// extract number: scans to the first ASCII digit then atoi; no digit → 0; stops
// at first non-digit. Wave 4 fix: a '-' immediately before the first digit is
// included, so "abc-5" reads -5 (was 5).
// [spec:foma:sem:iface.iface-extract-number-fn+1/test]
// [spec:foma:sem:foma.iface-extract-number-fn+1/test]
#[test]
fn extract_number_scans_to_first_digit() {
    assert_eq!(iface_extract_number("abc-5"), -5);
    assert_eq!(iface_extract_number("42abc"), 42);
    assert_eq!(iface_extract_number("v2.3"), 2);
    assert_eq!(iface_extract_number("hello"), 0);
    assert_eq!(iface_extract_number(""), 0);
    assert_eq!(iface_extract_number("007"), 7);
}

// factorize / sequentialize: pop + push fsm_bimachine / fsm_sequentialize;
// size unchanged, refuses on empty.
// [spec:foma:sem:iface.iface-factorize-fn/test]
// [spec:foma:sem:foma.iface-factorize-fn/test]
// [spec:foma:sem:iface.iface-sequentialize-fn/test]
// [spec:foma:sem:foma.iface-sequentialize-fn/test]
#[test]
fn factorize_and_sequentialize_are_single_net_ops() {
    let mut session = Session::new();
    iface_factorize(&mut session); // empty: refusal
    iface_sequentialize(&mut session); // empty: refusal
    assert_eq!(session.stack_size(), 0);
    push(&mut session, "a b");
    iface_sequentialize(&mut session);
    assert_eq!(session.stack_size(), 1);
    // sequentialize of an acyclic acceptor preserves its language.
    assert!(top_is(&mut session, "a b"));
    push(&mut session, "a b c");
    iface_factorize(&mut session);
    assert_eq!(session.stack_size(), 1); // bimachine factorization is one net op
}

// label net: pop + push fsm_sigma_pairs_net(net); size unchanged.
// [spec:foma:sem:iface.iface-label-net-fn/test]
// [spec:foma:sem:foma.iface-label-net-fn/test]
#[test]
fn label_net_extracts_attested_pairs() {
    let mut session = Session::new();
    iface_label_net(&mut session); // empty: refusal
    assert_eq!(session.stack_size(), 0);
    push(&mut session, "a:b");
    iface_label_net(&mut session);
    assert_eq!(session.stack_size(), 1);
    // The label net accepts the single attested pair a:b.
    assert_eq!(
        down1(&session.stack_pop().unwrap(), "a"),
        Some("b".to_string())
    );
}

// pop stack: empty prints "Stack is empty." (no iface_stack_check) and
// leaves the stack empty; otherwise pops + fsm_destroy's the top.
// [spec:foma:sem:iface.iface-pop-fn/test]
// [spec:foma:sem:foma.iface-pop-fn/test]
#[test]
fn pop_removes_top_or_reports_empty() {
    let mut session = Session::new();
    iface_pop(&mut session); // empty: message only
    assert_eq!(session.stack_size(), 0);
    push(&mut session, "a");
    push(&mut session, "b");
    iface_pop(&mut session); // removes "b"
    assert_eq!(session.stack_size(), 1);
    assert!(top_is(&mut session, "a")); // "a" is now the top
}

// print lower-words: caches the apply handle, enumerates (prints), resets
// the enumerator, and does NOT consume the net; empty refuses.
// [spec:foma:sem:iface.iface-lower-words-fn/test]
// [spec:foma:sem:foma.iface-lower-words-fn/test]
#[test]
fn lower_words_prints_without_consuming() {
    let mut session = Session::new();
    iface_lower_words(&mut session, -1); // empty: refusal
    assert_eq!(session.stack_size(), 0);
    push(&mut session, "a:b | a:c");
    iface_lower_words(&mut session, -1); // g_list_limit; prints "b","c"
    iface_lower_words(&mut session, 1); // explicit small limit
    assert_eq!(session.stack_size(), 1); // net not consumed
}

// name net + print name: name net strncpy's <=40 bytes into the top net's
// name (stored in full; C truncated to a fixed 40-byte field) and calls print
// name; the net stays on the stack.
// [spec:foma:sem:iface.iface-name-net-fn+1/test]
// [spec:foma:sem:foma.iface-name-net-fn+1/test]
// [spec:foma:sem:iface.iface-print-name-fn/test]
// [spec:foma:sem:foma.iface-print-name-fn/test]
#[test]
fn name_net_sets_full_name_then_prints() {
    let mut session = Session::new();
    iface_name_net(&mut session, "nope"); // empty: refusal, no panic
    iface_print_name(&mut session); // empty: refusal
    assert_eq!(session.stack_size(), 0);
    push(&mut session, "a");
    iface_name_net(&mut session, "hello");
    assert_eq!(session.stack_size(), 1); // not popped
    let top = session.stack_find_top().unwrap();
    assert_eq!(session.stack_entry_fsm(top, |f| f.name.clone()), "hello");
    iface_print_name(&mut session); // prints "hello"
    // >= 40 bytes: stored in full (C truncated to a fixed 40-byte field).
    let long = "x".repeat(45);
    iface_name_net(&mut session, &long);
    let top = session.stack_find_top().unwrap();
    let name = session.stack_entry_fsm(top, |f| f.name.clone());
    assert_eq!(name, long);
}

// print dot: requires >=1; writes dot to stdout (None) or a file (Some);
// net not consumed; empty refuses.
// [spec:foma:sem:iface.iface-print-dot-fn/test]
// [spec:foma:sem:foma.iface-print-dot-fn/test]
#[test]
fn print_dot_writes_and_keeps_net() {
    let mut session = Session::new();
    iface_print_dot(&mut session, None); // empty: refusal
    assert_eq!(session.stack_size(), 0);
    push(&mut session, "a b");
    iface_print_dot(&mut session, None); // to stdout
    let dotpath = std::env::temp_dir().join("foma_s1_printdot.dot");
    iface_print_dot(&mut session, Some(dotpath.to_str().unwrap()));
    assert_eq!(session.stack_size(), 1); // net not consumed
    assert!(dotpath.exists());
}

// print net: named lookup (found / not found via g_defines) and the stack
// top path; nothing is consumed.
// [spec:foma:sem:iface.iface-print-net-fn/test]
// [spec:foma:sem:foma.iface-print-net-fn/test]
#[test]
fn print_net_named_and_top_paths() {
    let mut session = Session::new();
    // netname None + empty stack: refusal.
    iface_print_net(&mut session, None, None);
    assert_eq!(session.stack_size(), 0);
    // netname Some with an empty registry: "No defined network".
    iface_print_net(&mut session, Some("Foo"), None);
    // Populate the registry with Foo, then print it by name.
    let def = fsm_parse_regex(&session.opts, "x y", None, None).unwrap();
    add_defined(&mut session.defines, Some(def), "Foo");
    iface_print_net(&mut session, Some("Foo"), None); // found → prints
    // netname None + populated stack: prints the top net.
    push(&mut session, "a b");
    iface_print_net(&mut session, None, None);
    assert_eq!(session.stack_size(), 1); // stack untouched
}

// print cmatrix / export cmatrix: with no confusion matrix both print
// "No confusion matrix defined." and do not consume the net; empty refuses.
// The cmatrix-present + byte-exact output is covered by the spelling/io
// integration tests.
// [spec:foma:sem:iface.iface-print-cmatrix-fn/test]
// [spec:foma:sem:foma.iface-print-cmatrix-fn/test]
// [spec:foma:sem:iface.iface-print-cmatrix-att-fn+1/test]
// [spec:foma:sem:foma.iface-print-cmatrix-att-fn+1/test]
#[test]
fn print_cmatrix_reports_missing_matrix() {
    let mut session = Session::new();
    iface_print_cmatrix(&mut session); // empty: refusal
    iface_print_cmatrix_att(&mut session, None); // empty: refusal
    assert_eq!(session.stack_size(), 0);
    push(&mut session, "a b"); // no confusion matrix attached
    iface_print_cmatrix(&mut session);
    iface_print_cmatrix_att(&mut session, None);
    assert_eq!(session.stack_size(), 1); // net not consumed
}

// export cmatrix to an unwritable path: with a confusion matrix present, the
// fopen failure is reported and the command returns without crashing (C's
// unchecked fopen NULL-derefs). Net not consumed.
// [spec:foma:sem:iface.iface-print-cmatrix-att-fn+1/test]
// [spec:foma:sem:foma.iface-print-cmatrix-att-fn+1/test]
#[test]
fn print_cmatrix_att_unwritable_path_does_not_crash() {
    let mut session = Session::new();
    let mut net = fsm_parse_regex(&session.opts, "a b", None, None).unwrap();
    crate::spelling::cmatrix_init(&mut net); // attach a confusion matrix
    session.stack_add(net);
    iface_print_cmatrix_att(&mut session, Some("/foma_no_such_dir_xyz123/cm.att"));
    assert_eq!(session.stack_size(), 1); // net not consumed, no panic
}

// print defined: an empty registry prints nothing; a populated list prints
// each entry. Neither touches the stack. Wave 4 fix: the function header
// format dropped the stray ')' ("%s@%i\t").
// [spec:foma:sem:iface.iface-print-defined-fn+1/test]
// [spec:foma:sem:foma.iface-print-defined-fn+1/test]
#[test]
fn print_defined_handles_empty_and_populated() {
    let mut session = Session::new();
    iface_print_defined(&mut session); // empty registry: nothing printed
    let def = fsm_parse_regex(&session.opts, "x y", None, None).unwrap();
    add_defined(&mut session.defines, Some(def), "Foo");
    iface_print_defined(&mut session); // prints "Foo\t<stats>"
    assert_eq!(session.stack_size(), 0);
}

// load defined: round-trip through a temp file. The file is written with
// io::save_defined (the same primitive iface_save_defined delegates to;
// that iface wrapper is slice 2's), then iface_load_defined restores the
// definitions into a fresh g_defines. A missing file → load reports
// "File error" and adds nothing.
// [spec:foma:sem:iface.iface-load-defined-fn/test]
// [spec:foma:sem:foma.iface-load-defined-fn/test]
#[test]
fn load_defined_restores_saved_definitions() {
    let mut session = Session::new();
    let path = std::env::temp_dir().join("foma_s1_defined.gz");
    let p = path.to_str().unwrap();
    // Missing file: load reports "File error" and leaves the table empty.
    iface_load_defined(&mut session, "/no/such/foma/defined");
    assert!(find_defined(&mut session.defines, "Foo").is_none());

    // Define Foo = [x y] and write the file via the io primitive.
    let def = fsm_parse_regex(&session.opts, "x y", None, None).unwrap();
    add_defined(&mut session.defines, Some(def), "Foo");
    save_defined(&mut session.defines, p).expect("save definitions to scratch file");
    // Load the file back into a fresh session's registry.
    let mut session = Session::new();
    iface_load_defined(&mut session, p);
    // Foo is restored and equals [x y].
    let restored = find_defined(&mut session.defines, "Foo").map(fsm_copy);
    let restored = restored.expect("Foo should be restored");
    let opts = &session.opts.clone();
    let expected = fsm_parse_regex(opts, "x y", None, None).unwrap();
    assert!(fsm_equivalent(&session.opts, restored, expected));
}

// load stack: reads every net from a multi-net binary file and pushes them
// in file order, so the LAST net in the file ends up on top. A missing file
// is reported and the stack is left unchanged.
// [spec:foma:sem:iface.iface-load-stack-fn/test]
// [spec:foma:sem:foma.iface-load-stack-fn/test]
#[test]
fn load_stack_pushes_in_file_order_last_on_top() {
    let opts = &FomaOptions::default();
    let path = std::env::temp_dir().join("foma_s1_stack.gz");
    let p = path.to_str().unwrap();
    // Build a bottom->top save file (a, b, c) exactly as iface_save_stack
    // would: nets on the CLI stack have had fsm_count run by stack_add (so
    // linecount is current), then each is foma_net_print'd into one gzip
    // stream.
    {
        let file = File::create(p).unwrap();
        let mut gz = GzEncoder::new(file, Compression::default());
        for r in ["a", "b", "c"] {
            let mut net = fsm_parse_regex(opts, r, None, None).unwrap();
            fsm_count(&mut net);
            foma_net_print(&net, &mut gz).expect("writing net to scratch file");
        }
        gz.finish().unwrap();
    }
    let mut session = Session::new();
    // Missing file: reported, stack unchanged.
    iface_load_stack(&mut session, "/no/such/foma/stack");
    assert_eq!(session.stack_size(), 0);
    // Real load: a, b, c pushed in file order → top is "c", bottom is "a".
    iface_load_stack(&mut session, p);
    assert_eq!(session.stack_size(), 3);
    assert!(top_is(&mut session, "c")); // pops "c"
    assert!(top_is(&mut session, "b"));
    assert!(top_is(&mut session, "a"));
}

// ================================================================
// SLICE 2 tests (iface_quit .. end of iface.c, C-file order), plus
// the four static callees slice 1 had stubbed. Same method as slice
// 1: stack-effect assertions (size + top-net language via crate::apply
// / fsm_equivalent), temp-file round-trips for read/write/save, and
// return-value pins for the pure/test_* functions. Print-only paths
// assert return value + non-panic + stack preservation; byte-exact
// stdout is left to the sibling integration tests.
// ================================================================

// random lower/upper/words + iface_apply_random: each caches the apply
// handle, prints tallied random paths, resets the enumerator, and does NOT
// consume the net; empty stack refuses. Output is non-deterministic so only
// the stack effect is pinned here.
// [spec:foma:sem:iface.iface-random-lower-fn/test]
// [spec:foma:sem:foma.iface-random-lower-fn/test]
// [spec:foma:sem:iface.iface-random-upper-fn/test]
// [spec:foma:sem:foma.iface-random-upper-fn/test]
// [spec:foma:sem:iface.iface-random-words-fn/test]
// [spec:foma:sem:foma.iface-random-words-fn/test]
// [spec:foma:sem:iface.iface-apply-random-fn/test]
// [spec:foma:sem:foma.iface-apply-random-fn/test]
#[test]
fn random_family_prints_without_consuming() {
    let mut session = Session::new();
    iface_random_lower(&mut session, -1); // empty: refusal
    iface_random_upper(&mut session, -1);
    iface_random_words(&mut session, -1);
    iface_apply_random(&mut session, apply_random_words, 3);
    assert_eq!(session.stack_size(), 0);
    push(&mut session, "a:b | c:d");
    iface_random_lower(&mut session, -1); // g_list_random_limit
    iface_random_upper(&mut session, 3); // explicit limit
    iface_random_words(&mut session, 2);
    iface_apply_random(&mut session, apply_random_words, 4);
    assert_eq!(session.stack_size(), 1); // net not consumed
    assert!(top_is(&mut session, "a:b | c:d"));
}

// print sigma / print stats: both read the top net and preserve it; empty
// stack refuses. print_stats returns 0 and drives print_mem_size.
// [spec:foma:sem:iface.iface-print-sigma-fn/test]
// [spec:foma:sem:foma.iface-print-sigma-fn/test]
// [spec:foma:sem:iface.iface-print-stats-fn/test]
// [spec:foma:sem:foma.iface-print-stats-fn/test]
#[test]
fn print_sigma_and_stats_keep_net() {
    let mut session = Session::new();
    iface_print_sigma(&mut session); // empty: refusal
    iface_print_stats(&mut session);
    assert_eq!(session.stack_size(), 0);
    push(&mut session, "a b");
    iface_print_sigma(&mut session);
    iface_print_stats(&mut session);
    assert_eq!(session.stack_size(), 1); // net not consumed
    assert!(top_is(&mut session, "a b"));
}

// print shortest-string / -size: both branches (arity 1 acceptor and arity 2
// transducer) run without consuming the net; empty refuses. Byte-exact output
// is integration-tested.
// [spec:foma:sem:iface.iface-print-shortest-string-fn/test]
// [spec:foma:sem:foma.iface-print-shortest-string-fn/test]
// [spec:foma:sem:iface.iface-print-shortest-string-size-fn/test]
// [spec:foma:sem:foma.iface-print-shortest-string-size-fn/test]
#[test]
fn print_shortest_string_both_arities_keep_net() {
    let mut session = Session::new();
    iface_print_shortest_string(&mut session); // empty: refusal
    iface_print_shortest_string_size(&mut session);
    assert_eq!(session.stack_size(), 0);
    push(&mut session, "a b b"); // arity 1
    iface_print_shortest_string(&mut session);
    iface_print_shortest_string_size(&mut session);
    assert_eq!(session.stack_size(), 1); // net not consumed
    let _ = session.stack_pop();
    push(&mut session, "[a:b] | [a a:c]"); // arity 2 transducer
    iface_print_shortest_string(&mut session);
    iface_print_shortest_string_size(&mut session);
    assert_eq!(session.stack_size(), 1); // net not consumed
}

// read att / prolog: a print + stack_add wrapper over the io reader. Round-trip
// through the matching writer (net_print_att / foma_write_prolog); a bad path
// returns 1 and leaves the stack unchanged.
// [spec:foma:sem:iface.iface-read-att-fn/test]
// [spec:foma:sem:foma.iface-read-att-fn/test]
// [spec:foma:sem:iface.iface-read-prolog-fn/test]
// [spec:foma:sem:foma.iface-read-prolog-fn/test]
#[test]
fn read_att_and_prolog_roundtrip_and_error() {
    let opts = &FomaOptions::default();
    let dir = std::env::temp_dir();
    let attp = dir.join("foma_s2_read.att");
    let att = attp.to_str().unwrap();
    {
        let net = fsm_parse_regex(opts, "a b", None, None).unwrap();
        let mut f = File::create(att).unwrap();
        net_print_att(opts, &net, &mut f).expect("writing att to scratch file");
    }
    let plp = dir.join("foma_s2_read.prolog");
    let pl = plp.to_str().unwrap();
    {
        let mut net = fsm_parse_regex(opts, "a b", None, None).unwrap();
        foma_write_prolog(&mut net, Some(pl)).expect("prolog write to temp file");
    }
    let mut session = Session::new();
    assert!(!iface_read_att(&mut session, "/no/such/foma/att"));
    assert!(!iface_read_prolog(&mut session, "/no/such/foma/prolog"));
    assert_eq!(session.stack_size(), 0);
    assert!(iface_read_att(&mut session, att));
    assert_eq!(session.stack_size(), 1);
    assert!(top_is(&mut session, "a b"));
    assert!(iface_read_prolog(&mut session, pl));
    assert_eq!(session.stack_size(), 1);
    assert!(top_is(&mut session, "a b"));
}

// read text / spaced-text: compile a newline word list / space-separated
// symbol lines into an automaton (topsort(minimize)); pushed on the stack. A
// bad path returns false.
// [spec:foma:sem:iface.iface-read-text-fn/test]
// [spec:foma:sem:foma.iface-read-text-fn/test]
// [spec:foma:sem:iface.iface-read-spaced-text-fn/test]
// [spec:foma:sem:foma.iface-read-spaced-text-fn/test]
#[test]
fn read_text_and_spaced_text() {
    let dir = std::env::temp_dir();
    let rtp = dir.join("foma_s2_text.txt");
    let rt = rtp.to_str().unwrap();
    std::fs::write(rt, "cat\ndog\n").unwrap();
    let rstp = dir.join("foma_s2_spaced.txt");
    let rst = rstp.to_str().unwrap();
    std::fs::write(rst, "a b c\n").unwrap();

    let mut session = Session::new();
    assert!(!iface_read_text(&mut session, "/no/such/foma/text"));
    assert!(!iface_read_spaced_text(
        &mut session,
        "/no/such/foma/spaced"
    ));
    assert_eq!(session.stack_size(), 0);
    assert!(iface_read_text(&mut session, rt));
    assert_eq!(session.stack_size(), 1);
    let net = session.stack_pop().unwrap();
    assert!(accepts_down(&net, "cat"));
    assert!(accepts_down(&net, "dog"));
    assert!(!accepts_down(&net, "cow"));
    assert!(iface_read_spaced_text(&mut session, rst));
    assert_eq!(session.stack_size(), 1);
    assert!(accepts_down(&session.stack_pop().unwrap(), "abc"));
}

// stack check: returns 1 when >= size nets are present, else prints the
// "Not enough networks" message and returns 0.
// [spec:foma:sem:iface.iface-stack-check-fn/test]
// [spec:foma:sem:foma.iface-stack-check-fn/test]
#[test]
fn stack_check_counts_the_stack() {
    let mut session = Session::new();
    assert!(iface_stack_check(&mut session, 0)); // 0 >= 0 with an empty stack
    assert!(!(iface_stack_check(&mut session, 1)));
    push(&mut session, "a");
    push(&mut session, "b");
    assert!(iface_stack_check(&mut session, 2));
    assert!(!(iface_stack_check(&mut session, 3)));
}

// substitute symbol / defined: symbol dequotes both args and replaces the
// `original` symbol with `substitute`; defined replaces every arc labelled
// `original` with the named defined network. Both pop+push a single net;
// the "No defined network" and "does not occur" guards leave it untouched.
// [spec:foma:sem:iface.iface-substitute-symbol-fn/test]
// [spec:foma:sem:foma.iface-substitute-symbol-fn/test]
// [spec:foma:sem:iface.iface-substitute-defined-fn/test]
// [spec:foma:sem:foma.iface-substitute-defined-fn/test]
#[test]
fn substitute_symbol_and_defined() {
    let mut session = Session::new();
    iface_substitute_symbol(&mut session, "a", "x"); // empty: refusal
    iface_substitute_defined(&mut session, "a", "X");
    assert_eq!(session.stack_size(), 0);

    push(&mut session, "a b");
    iface_substitute_symbol(&mut session, "a", "x"); // replace symbol a with x
    assert_eq!(session.stack_size(), 1);
    assert!(top_is(&mut session, "x b"));

    // substitute defined: X = [p] replaces the arc labelled `a`.
    let def = fsm_parse_regex(&session.opts, "p", None, None).unwrap();
    add_defined(&mut session.defines, Some(def), "X");
    push(&mut session, "a b");
    iface_substitute_defined(&mut session, "a", "Nope"); // no such defined net → unchanged
    assert_eq!(session.stack_size(), 1);
    iface_substitute_defined(&mut session, "zzz", "X"); // symbol does not occur → unchanged
    assert_eq!(session.stack_size(), 1);
    iface_substitute_defined(&mut session, "a", "X"); // success → a replaced by [p]
    assert_eq!(session.stack_size(), 1);
    assert!(top_is(&mut session, "p b"));
}

// upper-words / words: cache the apply handle, enumerate up to `limit`
// (g_list_limit for -1), reset the enumerator, and preserve the net; empty
// refuses.
// [spec:foma:sem:iface.iface-upper-words-fn/test]
// [spec:foma:sem:foma.iface-upper-words-fn/test]
// [spec:foma:sem:iface.iface-words-fn/test]
// [spec:foma:sem:foma.iface-words-fn/test]
#[test]
fn upper_words_and_words_keep_net() {
    let mut session = Session::new();
    iface_upper_words(&mut session, -1); // empty: refusal
    iface_words(&mut session, -1);
    assert_eq!(session.stack_size(), 0);
    push(&mut session, "a:b | a:c");
    iface_upper_words(&mut session, -1);
    iface_words(&mut session, 1);
    iface_words(&mut session, -1);
    assert_eq!(session.stack_size(), 1); // net not consumed
}

// prune / reverse / zero-plus / upper-side: each pops + pushes a single-net
// op. Language checks pin the transformation.
// [spec:foma:sem:iface.iface-prune-fn/test]
// [spec:foma:sem:foma.iface-prune-fn/test]
// [spec:foma:sem:iface.iface-reverse-fn/test]
// [spec:foma:sem:foma.iface-reverse-fn/test]
// [spec:foma:sem:iface.iface-zero-plus-fn/test]
// [spec:foma:sem:foma.iface-zero-plus-fn/test]
// [spec:foma:sem:iface.iface-upper-side-fn/test]
// [spec:foma:sem:foma.iface-upper-side-fn/test]
#[test]
fn prune_reverse_zero_plus_upper_side() {
    let mut session = Session::new();
    iface_prune(&mut session); // empty: refusal
    iface_reverse(&mut session);
    iface_zero_plus(&mut session);
    iface_upper_side(&mut session);
    assert_eq!(session.stack_size(), 0);

    push(&mut session, "a b"); // coaccessible already → language unchanged
    iface_prune(&mut session);
    assert_eq!(session.stack_size(), 1);
    assert!(top_is(&mut session, "a b"));

    push(&mut session, "a b"); // reverse of "ab" accepts "ba"
    iface_reverse(&mut session);
    let net = session.stack_pop().unwrap();
    assert!(accepts_down(&net, "ba"));
    assert!(!accepts_down(&net, "ab"));

    push(&mut session, "a"); // Kleene star
    iface_zero_plus(&mut session);
    let net = session.stack_pop().unwrap();
    assert!(accepts_down(&net, ""));
    assert!(accepts_down(&net, "aa"));

    push(&mut session, "a:b"); // upper projection
    iface_upper_side(&mut session);
    assert_eq!(session.stack_size(), 1);
    assert!(top_is(&mut session, "a"));
}

// rotate / turn: both wire to stack_rotate (turn is the reproduced latent bug
// — it does NOT reverse the stack). rotate swaps the TOP and BOTTOM fsm fields
// only; single-net and empty are no-ops.
// [spec:foma:sem:iface.iface-rotate-fn/test]
// [spec:foma:sem:foma.iface-rotate-fn/test]
// [spec:foma:sem:iface.iface-turn-fn+1/test]
// [spec:foma:sem:foma.iface-turn-fn+1/test]
#[test]
fn rotate_swaps_ends_turn_reverses() {
    let mut session = Session::new();
    iface_rotate(&mut session); // empty: refusal (guard), no-op
    iface_turn(&mut session);
    assert_eq!(session.stack_size(), 0);

    // bottom->top: a, b, c. rotate swaps a<->c fsms → top a, mid b, bottom c.
    push(&mut session, "a");
    push(&mut session, "b");
    push(&mut session, "c");
    iface_rotate(&mut session);
    assert!(top_is(&mut session, "a")); // former bottom now on top
    assert!(top_is(&mut session, "b"));
    assert!(top_is(&mut session, "c"));

    // turn REVERSES the whole stack (not a top/bottom swap). Use 4 nets so the
    // two differ: reverse of a,b,c,d is d,c,b,a (pop order a,b,c,d), whereas a
    // swap-ends would give d,b,c,a (pop order a,c,b,d).
    push(&mut session, "a");
    push(&mut session, "b");
    push(&mut session, "c");
    push(&mut session, "d");
    iface_turn(&mut session);
    assert!(top_is(&mut session, "a"));
    assert!(top_is(&mut session, "b"));
    assert!(top_is(&mut session, "c"));
    assert!(top_is(&mut session, "d"));

    push(&mut session, "x"); // single net: unchanged
    iface_rotate(&mut session);
    assert!(top_is(&mut session, "x"));
}

// save defined / save stack: save_defined reports "No defined networks" when
// g_defines is empty, else round-trips through iface_load_defined; save_stack
// writes every net bottom→top and round-trips through iface_load_stack.
// [spec:foma:sem:iface.iface-save-defined-fn/test]
// [spec:foma:sem:foma.iface-save-defined-fn/test]
// [spec:foma:sem:iface.iface-save-stack-fn/test]
// [spec:foma:sem:foma.iface-save-stack-fn/test]
#[test]
fn save_defined_and_save_stack_roundtrip() {
    let mut session = Session::new();
    let dir = std::env::temp_dir();
    let dp = dir.join("foma_s2_saved.gz");
    let d = dp.to_str().unwrap();
    // Empty registry: the file is still created (an empty definitions gz) —
    // C's "No defined networks." guarded only the pre-init NULL registry.
    let _ = std::fs::remove_file(d);
    iface_save_defined(&mut session, d);
    assert!(std::path::Path::new(d).exists());
    // Populate, save, load back into a fresh session.
    let _ = std::fs::remove_file(d);
    let def = fsm_parse_regex(&session.opts, "x y", None, None).unwrap();
    add_defined(&mut session.defines, Some(def), "Foo");
    iface_save_defined(&mut session, d);
    let mut session = Session::new();
    iface_load_defined(&mut session, d);
    let restored = find_defined(&mut session.defines, "Foo").map(fsm_copy);
    let expected = fsm_parse_regex(&session.opts, "x y", None, None).unwrap();
    assert!(fsm_equivalent(
        &session.opts,
        restored.expect("Foo restored"),
        expected
    ));

    // save stack: writes bottom→top (a, b, c); load pushes in file order.
    let sp = dir.join("foma_s2_savestack.gz");
    let s = sp.to_str().unwrap();
    let mut session = Session::new();
    let _ = std::fs::remove_file(s);
    iface_save_stack(&mut session, s); // empty: refusal, nothing written
    assert!(!std::path::Path::new(s).exists());
    push(&mut session, "a");
    push(&mut session, "b");
    push(&mut session, "c");
    iface_save_stack(&mut session, s);
    assert_eq!(session.stack_size(), 3); // stack not consumed
    session = Session::new();
    iface_load_stack(&mut session, s);
    assert_eq!(session.stack_size(), 3);
    assert!(top_is(&mut session, "c"));
    assert!(top_is(&mut session, "b"));
    assert!(top_is(&mut session, "a"));
}

// set variable: BOOL accepts ON/OFF/1/0 (else "Invalid value"); the name match
// is strncmp over 8 bytes (documented latent bug: any name sharing a real
// variable's first 8 bytes hits it). INT uses strtol truncation and rejects
// no-digit / range / negative. STRING replaces the value verbatim. show
// variable(s) print without touching the stack or the values. Wave 4 fix:
// iface_show_variable now formats by declared type (INT as value, STRING as
// string, BOOL as ON/OFF) instead of ON/OFF for every type.
// [spec:foma:sem:iface.iface-set-variable-fn+1/test]
// [spec:foma:sem:foma.iface-set-variable-fn+1/test]
// [spec:foma:sem:iface.iface-show-variable-fn+2/test]
// [spec:foma:sem:foma.iface-show-variable-fn+2/test]
// [spec:foma:sem:iface.iface-show-variables-fn/test]
// [spec:foma:sem:foma.iface-show-variables-fn/test]
#[test]
fn set_and_show_variables() {
    let mut session = Session::new();
    // BOOL: ON/OFF/1/0 all recognised.
    iface_set_variable(&mut session, "minimal", "OFF");
    assert!(!session.opts.minimal);
    iface_set_variable(&mut session, "minimal", "ON");
    assert!(session.opts.minimal);
    iface_set_variable(&mut session, "minimal", "0");
    assert!(!session.opts.minimal);
    iface_set_variable(&mut session, "minimal", "1");
    assert!(session.opts.minimal);
    iface_set_variable(&mut session, "minimal", "bogus"); // invalid → unchanged
    assert!(session.opts.minimal);

    // Full-name match: "hopcroft-XYZ" shares an 8-char prefix with "hopcroft-min"
    // but is not it, so it must NOT touch the variable (C's strncmp-8 collided).
    iface_set_variable(&mut session, "hopcroft-min", "ON");
    assert!(session.opts.minimize_hopcroft);
    iface_set_variable(&mut session, "hopcroft-XYZ", "OFF"); // prefix-equal but different: no match
    assert!(session.opts.minimize_hopcroft); // unchanged

    // INT: strtol truncation; no-digit / negative are rejected.
    iface_set_variable(&mut session, "med-limit", "7");
    assert_eq!(session.opts.med_limit, 7);
    iface_set_variable(&mut session, "med-limit", "abc"); // no digits → unchanged
    assert_eq!(session.opts.med_limit, 7);
    iface_set_variable(&mut session, "med-limit", "-3"); // negative → unchanged
    assert_eq!(session.opts.med_limit, 7);

    // STRING: verbatim replace.
    iface_set_variable(&mut session, "att-epsilon", "@E@");
    assert_eq!(session.opts.att_epsilon, "@E@");

    // Unknown variable: message only, no panic.
    iface_set_variable(&mut session, "zzznope", "ON");

    // show variable / variables: non-panic, no stack effect.
    iface_show_variable(&mut session, "med-limit"); // Wave 4 fix: prints the value (7), not OFF
    iface_show_variable(&mut session, "att-epsilon");
    iface_show_variable(&mut session, "minimal");
    iface_show_variable(&mut session, "zzznope"); // "no global variable"
    iface_show_variables(&mut session);
    assert_eq!(session.stack_size(), 0);
}

// shuffle / union: both fold the entire stack (commutative); union minimizes
// without topsort. Refuse with < 2 nets.
// [spec:foma:sem:iface.iface-shuffle-fn/test]
// [spec:foma:sem:foma.iface-shuffle-fn/test]
// [spec:foma:sem:iface.iface-union-fn/test]
// [spec:foma:sem:foma.iface-union-fn/test]
#[test]
fn shuffle_and_union_fold_whole_stack() {
    let mut session = Session::new();
    push(&mut session, "a");
    iface_shuffle(&mut session); // < 2: refusal
    iface_union(&mut session);
    assert_eq!(session.stack_size(), 1);

    push(&mut session, "b"); // shuffle(a,b) = {ab, ba}
    iface_shuffle(&mut session);
    assert_eq!(session.stack_size(), 1);
    let net = session.stack_pop().unwrap();
    assert!(accepts_down(&net, "ab"));
    assert!(accepts_down(&net, "ba"));

    push(&mut session, "a");
    push(&mut session, "b");
    push(&mut session, "c"); // union folds to a|b|c
    iface_union(&mut session);
    assert_eq!(session.stack_size(), 1);
    assert!(top_is(&mut session, "a | b | c"));
}

// sigma net: pop + push fsm_sigma_net; accepts every single symbol of the
// alphabet.
// [spec:foma:sem:iface.iface-sigma-net-fn/test]
// [spec:foma:sem:foma.iface-sigma-net-fn/test]
#[test]
fn sigma_net_accepts_single_symbols() {
    let mut session = Session::new();
    iface_sigma_net(&mut session); // empty: refusal
    assert_eq!(session.stack_size(), 0);
    push(&mut session, "a b"); // sigma {a, b}
    iface_sigma_net(&mut session);
    assert_eq!(session.stack_size(), 1);
    assert!(top_is(&mut session, "a | b"));
}

// sort in / out: mutate the top net's arc order in place (no pop/push);
// sort net: sigma_sort + pop + push topsort. All preserve the language.
// [spec:foma:sem:iface.iface-sort-input-fn/test]
// [spec:foma:sem:foma.iface-sort-input-fn/test]
// [spec:foma:sem:iface.iface-sort-output-fn/test]
// [spec:foma:sem:foma.iface-sort-output-fn/test]
// [spec:foma:sem:iface.iface-sort-fn/test]
// [spec:foma:sem:foma.iface-sort-fn/test]
#[test]
fn sort_family_preserves_language() {
    let mut session = Session::new();
    iface_sort_input(&mut session); // empty: refusal
    iface_sort_output(&mut session);
    iface_sort(&mut session);
    assert_eq!(session.stack_size(), 0);
    push(&mut session, "a b | b a");
    iface_sort_input(&mut session);
    assert_eq!(session.stack_size(), 1);
    iface_sort_output(&mut session);
    assert_eq!(session.stack_size(), 1);
    iface_sort(&mut session);
    assert_eq!(session.stack_size(), 1);
    assert!(top_is(&mut session, "a b | b a"));
}

// test_* family: each reads the top net (equivalent reads the top two),
// prints the predicate via iface_print_bool, and preserves the stack. The
// printed value is pinned by calling the same predicate the iface fn uses.
// [spec:foma:sem:iface.iface-test-functional-fn/test]
// [spec:foma:sem:foma.iface-test-functional-fn/test]
// [spec:foma:sem:iface.iface-test-identity-fn/test]
// [spec:foma:sem:foma.iface-test-identity-fn/test]
// [spec:foma:sem:iface.iface-test-unambiguous-fn/test]
// [spec:foma:sem:foma.iface-test-unambiguous-fn/test]
// [spec:foma:sem:iface.iface-test-sequential-fn/test]
// [spec:foma:sem:foma.iface-test-sequential-fn/test]
// [spec:foma:sem:iface.iface-test-null-fn/test]
// [spec:foma:sem:foma.iface-test-null-fn/test]
// [spec:foma:sem:iface.iface-test-nonnull-fn/test]
// [spec:foma:sem:foma.iface-test-nonnull-fn/test]
// [spec:foma:sem:iface.iface-test-lower-universal-fn/test]
// [spec:foma:sem:foma.iface-test-lower-universal-fn/test]
// [spec:foma:sem:iface.iface-test-upper-universal-fn/test]
// [spec:foma:sem:foma.iface-test-upper-universal-fn/test]
// [spec:foma:sem:iface.iface-test-equivalent-fn/test]
// [spec:foma:sem:foma.iface-test-equivalent-fn/test]
#[test]
fn test_family_pins_predicate_and_preserves_stack() {
    let mut session = Session::new();
    // Every test_* refuses on an empty stack.
    iface_test_functional(&mut session);
    iface_test_identity(&mut session);
    iface_test_unambiguous(&mut session);
    iface_test_sequential(&mut session);
    iface_test_null(&mut session);
    iface_test_nonnull(&mut session);
    iface_test_lower_universal(&mut session);
    iface_test_upper_universal(&mut session);
    assert_eq!(session.stack_size(), 0);

    // functional: true for a:b, false for a:b|a:c.
    push(&mut session, "a:b");
    let t = session.stack_find_top().unwrap();
    assert!(session.stack_entry_fsm_with_opts(t, fsm_isfunctional));
    iface_test_functional(&mut session);
    assert_eq!(session.stack_size(), 1);
    let _ = session.stack_pop();
    push(&mut session, "[a:b] | [a:c]");
    let t = session.stack_find_top().unwrap();
    assert!(!(session.stack_entry_fsm_with_opts(t, fsm_isfunctional)));
    iface_test_functional(&mut session);
    let _ = session.stack_pop();

    // identity: true for a, false for a:b.
    push(&mut session, "a");
    let t = session.stack_find_top().unwrap();
    assert!(session.stack_entry_fsm_with_opts(t, fsm_isidentity));
    iface_test_identity(&mut session);
    let _ = session.stack_pop();
    push(&mut session, "a:b");
    let t = session.stack_find_top().unwrap();
    assert!(!(session.stack_entry_fsm_with_opts(t, fsm_isidentity)));
    iface_test_identity(&mut session);
    let _ = session.stack_pop();

    // unambiguous: true for a:b, false for a:b|a:c.
    push(&mut session, "a:b");
    let t = session.stack_find_top().unwrap();
    assert!(session.stack_entry_fsm_with_opts(t, fsm_isunambiguous));
    iface_test_unambiguous(&mut session);
    let _ = session.stack_pop();
    push(&mut session, "[a:b] | [a:c]");
    let t = session.stack_find_top().unwrap();
    assert!(!(session.stack_entry_fsm_with_opts(t, fsm_isunambiguous)));
    iface_test_unambiguous(&mut session);
    let _ = session.stack_pop();

    // sequential: true for the acyclic acceptor "a b".
    push(&mut session, "a b");
    let t = session.stack_find_top().unwrap();
    assert!(session.stack_entry_fsm(t, |f| fsm_issequential(f)));
    iface_test_sequential(&mut session);
    let _ = session.stack_pop();

    // null / nonnull: empty language vs. non-empty.
    push(&mut session, "[a] - [a]"); // empty
    let t = session.stack_find_top().unwrap();
    assert!(session.stack_entry_fsm_with_opts(t, fsm_isempty));
    iface_test_null(&mut session);
    iface_test_nonnull(&mut session);
    assert_eq!(session.stack_size(), 1);
    let _ = session.stack_pop();
    push(&mut session, "a"); // non-empty
    let t = session.stack_find_top().unwrap();
    assert!(!(session.stack_entry_fsm_with_opts(t, fsm_isempty)));
    iface_test_null(&mut session);
    iface_test_nonnull(&mut session);
    let _ = session.stack_pop();

    // lower-/upper-universal: compound (copy + complement + isempty), so only
    // preservation is pinned here. ?* is universal; "a" is not.
    push(&mut session, "?*");
    iface_test_lower_universal(&mut session);
    iface_test_upper_universal(&mut session);
    assert_eq!(session.stack_size(), 1);
    let _ = session.stack_pop();
    push(&mut session, "a");
    iface_test_lower_universal(&mut session);
    iface_test_upper_universal(&mut session);
    assert_eq!(session.stack_size(), 1);
    let _ = session.stack_pop();

    // equivalent: reads the top two, preserves both. (a|b)≡(b|a) true; a≢b.
    push(&mut session, "a | b");
    push(&mut session, "b | a");
    assert_eq!(session.stack_size(), 2);
    iface_test_equivalent(&mut session);
    assert_eq!(session.stack_size(), 2);
    let one = session.stack_entry_fsm(session.stack_find_top().unwrap(), fsm_copy);
    let two = session.stack_entry_fsm(session.stack_find_second().unwrap(), fsm_copy);
    assert!(fsm_equivalent(&session.opts, one, two));
    session = Session::new();
    push(&mut session, "a");
    push(&mut session, "b");
    iface_test_equivalent(&mut session);
    assert_eq!(session.stack_size(), 2);
}

// twosided flag-diacritics: pop + push flag_twosided; a flagless net keeps
// its language.
// [spec:foma:sem:iface.iface-twosided-flags-fn/test]
// [spec:foma:sem:foma.iface-twosided-flags-fn/test]
#[test]
fn twosided_flags_preserves_flagless_language() {
    let mut session = Session::new();
    iface_twosided_flags(&mut session); // empty: refusal
    assert_eq!(session.stack_size(), 0);
    push(&mut session, "a b");
    iface_twosided_flags(&mut session);
    assert_eq!(session.stack_size(), 1);
    assert!(top_is(&mut session, "a b"));
}

// view: guarded by iface_stack_check(1); with an empty stack it returns
// without calling view_net (which would spawn dot + a viewer). This guard-path
// test also stands in for view_net's facet — no external process is spawned.
// [spec:foma:sem:iface.iface-view-fn/test]
// [spec:foma:sem:foma.iface-view-fn/test]
// [spec:foma:sem:iface.view-net-fn/test]
// [spec:foma:sem:foma.view-net-fn/test]
#[test]
fn view_refuses_empty_stack_without_spawning() {
    let mut session = Session::new();
    iface_view(&mut session); // empty: refusal → view_net not reached, no viewer spawned
    assert_eq!(session.stack_size(), 0);
}

// words to file: type 0/1/2 select the word/upper/lower enumerator; a cyclic
// net prints the "can't write" message and writes nothing; an acyclic net
// writes each enumerated word. The net is preserved. Wave 4 fix: type 0 after a
// type-1/2 call re-selects apply_words (the C reused the sticky enumerator).
// [spec:foma:sem:iface.iface-words-file-fn+1/test]
// [spec:foma:sem:foma.iface-words-file-fn+1/test]
#[test]
fn words_file_writes_per_type_and_refuses_cyclic() {
    let dir = std::env::temp_dir();
    let p0 = dir.join("foma_s2_words0.txt");
    let p1 = dir.join("foma_s2_words1.txt");
    let p2 = dir.join("foma_s2_words2.txt");
    let pc = dir.join("foma_s2_wordsc.txt");
    let _ = std::fs::remove_file(&pc);

    let mut session = Session::new();
    iface_words_file(&mut session, p0.to_str().unwrap(), 0); // empty: refusal
    assert_eq!(session.stack_size(), 0);

    push(&mut session, "a b"); // acyclic acceptor → whole word "ab"
    iface_words_file(&mut session, p0.to_str().unwrap(), 0);
    assert_eq!(session.stack_size(), 1); // net not consumed
    assert_eq!(std::fs::read_to_string(&p0).unwrap(), "ab\n");
    let _ = session.stack_pop();

    push(&mut session, "a:b"); // transducer → upper "a", lower "b"
    iface_words_file(&mut session, p1.to_str().unwrap(), 1);
    assert_eq!(std::fs::read_to_string(&p1).unwrap(), "a\n");
    iface_words_file(&mut session, p2.to_str().unwrap(), 2);
    assert_eq!(std::fs::read_to_string(&p2).unwrap(), "b\n");
    // Wave 4 fix: a type-0 call right after type 1/2 uses the words enumerator,
    // not the stale upper ("a") / lower ("b") one from the previous call.
    let pmix = dir.join("foma_s2_wordsmix.txt");
    iface_words_file(&mut session, pmix.to_str().unwrap(), 0);
    let mix = std::fs::read_to_string(&pmix).unwrap();
    assert_ne!(mix, "a\n"); // not the stale upper-words enumerator
    assert_ne!(mix, "b\n"); // not the stale lower-words enumerator
    let _ = session.stack_pop();

    push_topsorted(&mut session, "a*"); // cyclic → refuses before opening the file
    iface_words_file(&mut session, pc.to_str().unwrap(), 0);
    assert_eq!(session.stack_size(), 1);
    assert!(!std::path::Path::new(&pc).exists());
}

// pairs / pairs_call / random_pairs: print input:output pairs (splitting the
// pair encoding), preserve the net, and reset the enumerator; empty refuses.
// random_pairs routes through pairs_call(limit, 1). Wave 4 fix: random_pairs now
// resolves -1 to g_list_random_limit (not g_list_limit) before delegating.
// [spec:foma:sem:iface.iface-pairs-fn/test]
// [spec:foma:sem:foma.iface-pairs-fn/test]
// [spec:foma:sem:iface.iface-pairs-call-fn/test]
// [spec:foma:sem:iface.iface-random-pairs-fn+1/test]
// [spec:foma:sem:foma.iface-random-pairs-fn+1/test]
#[test]
fn pairs_family_prints_without_consuming() {
    let mut session = Session::new();
    iface_pairs(&mut session, -1); // empty: refusal
    iface_pairs_call(&mut session, 2, 0);
    iface_random_pairs(&mut session, -1);
    assert_eq!(session.stack_size(), 0);
    push(&mut session, "a:b | c:d");
    iface_pairs(&mut session, -1);
    iface_pairs_call(&mut session, 2, 0);
    iface_pairs_call(&mut session, 2, 1); // random path
    iface_random_pairs(&mut session, -1);
    assert_eq!(session.stack_size(), 1); // net not consumed
    assert!(top_is(&mut session, "a:b | c:d"));
}

// pairs to file: cyclic refuses (message, no file); acyclic writes
// upper\tlower per pair. Net preserved.
// [spec:foma:sem:iface.iface-pairs-file-fn/test]
// [spec:foma:sem:foma.iface-pairs-file-fn/test]
#[test]
fn pairs_file_writes_pairs_and_refuses_cyclic() {
    let dir = std::env::temp_dir();
    let pf = dir.join("foma_s2_pairs.txt");
    let pc = dir.join("foma_s2_pairs_cyclic.txt");
    let _ = std::fs::remove_file(&pc);
    let mut session = Session::new();
    iface_pairs_file(&mut session, pf.to_str().unwrap()); // empty: refusal
    assert_eq!(session.stack_size(), 0);
    push(&mut session, "a:b");
    iface_pairs_file(&mut session, pf.to_str().unwrap());
    assert_eq!(session.stack_size(), 1);
    assert_eq!(std::fs::read_to_string(&pf).unwrap(), "a\tb\n");
    let _ = session.stack_pop();
    push_topsorted(&mut session, "[a:b]*"); // cyclic transducer → refuses, no file
    iface_pairs_file(&mut session, pc.to_str().unwrap());
    assert_eq!(session.stack_size(), 1);
    assert!(!std::path::Path::new(&pc).exists());
}

// write att / write prolog: watt returns 1 on an empty stack, else writes an
// AT&T/prolog file (net preserved). Round-trip through the matching io reader.
// [spec:foma:sem:iface.iface-write-att-fn/test]
// [spec:foma:sem:foma.iface-write-att-fn/test]
// [spec:foma:sem:iface.iface-write-prolog-fn/test]
// [spec:foma:sem:foma.iface-write-prolog-fn/test]
#[test]
fn write_att_and_prolog_roundtrip() {
    let dir = std::env::temp_dir();
    let attp = dir.join("foma_s2_write.att");
    let att = attp.to_str().unwrap();
    let plp = dir.join("foma_s2_write.prolog");
    let pl = plp.to_str().unwrap();

    let mut session = Session::new();
    assert!(!iface_write_att(&mut session, Some(att))); // empty: fails

    push(&mut session, "a b");
    assert!(iface_write_att(&mut session, Some(att)));
    assert_eq!(session.stack_size(), 1); // net not consumed
    let back = read_att(&session.opts, att).unwrap();
    let expected = fsm_parse_regex(&session.opts, "a b", None, None).unwrap();
    assert!(fsm_equivalent(&session.opts, back, expected));

    iface_write_prolog(&mut session, Some(pl));
    assert_eq!(session.stack_size(), 1);
    let back = fsm_read_prolog(pl).unwrap();
    let expected = fsm_parse_regex(&session.opts, "a b", None, None).unwrap();
    assert!(fsm_equivalent(&session.opts, back, expected));
}

// sigptr (static): the three reserved arc labels map to 0/?/@; a matched
// symbol is returned verbatim except the quoted "0"/"?" and escaped \n/\r
// special cases; an unknown number yields NONE(n).
// [spec:foma:sem:iface.sigptr-fn/test]
#[test]
fn sigptr_maps_reserved_and_special_symbols() {
    let sig = vec![
        Sigma {
            number: 3,
            symbol: "0".into(),
        },
        Sigma {
            number: 4,
            symbol: "?".into(),
        },
        Sigma {
            number: 5,
            symbol: "\n".into(),
        },
        Sigma {
            number: 6,
            symbol: "\r".into(),
        },
        Sigma {
            number: 7,
            symbol: "hello".into(),
        },
    ];
    assert_eq!(sigptr(&sig, EPSILON), "0");
    assert_eq!(sigptr(&sig, UNKNOWN), "?");
    assert_eq!(sigptr(&sig, IDENTITY), "@");
    assert_eq!(sigptr(&sig, 3), "\"0\"");
    assert_eq!(sigptr(&sig, 4), "\"?\"");
    assert_eq!(sigptr(&sig, 5), "\\n");
    assert_eq!(sigptr(&sig, 6), "\\r");
    assert_eq!(sigptr(&sig, 7), "hello");
    assert_eq!(sigptr(&sig, 99), "NONE(99)");
}

// print_sigma (static): writes "Sigma:" + the >2 symbols + "@"/"?" then a
// "Size:" line. Byte-exact on the sigma of "a b".
// [spec:foma:sem:iface.print-sigma-fn+1/test]
#[test]
fn print_sigma_static_formats_alphabet() {
    let opts = &FomaOptions::default();
    let net = fsm_parse_regex(opts, "a b", None, None).unwrap();
    let mut buf: Vec<u8> = Vec::new();
    print_sigma(&net.sigma, &mut buf);
    let s = String::from_utf8(buf).unwrap();
    assert!(s.starts_with("Sigma:"), "got {:?}", s);
    assert!(s.contains(" a"));
    assert!(s.contains(" b"));
    assert!(s.trim_end().ends_with("Size: 2."), "got {:?}", s);
}

// print_stats (static) + print_mem_size (static): print_stats drives
// print_mem_size (side-effect only).
// [spec:foma:sem:iface.print-stats-fn+1/test]
// [spec:foma:sem:foma.print-stats-fn+1/test]
// [spec:foma:sem:iface.print-mem-size-fn/test]
#[test]
fn print_stats_static_prints_size_line() {
    let opts = &FomaOptions::default();
    let mut net = fsm_parse_regex(opts, "a b | c", None, None).unwrap();
    fsm_count(&mut net);
    print_stats(&net);
    // exercise print_mem_size directly as well (side-effect only).
    print_mem_size(&net);
}

// print_net (static): writes the Sigma/Net/Flags/Arity/arc dump to stdout
// (None) or a file (Some) and does not touch the stack.
// [spec:foma:sem:iface.print-net-fn+1/test]
#[test]
fn print_net_static_writes_dump() {
    let opts = &FomaOptions::default();
    let mut net = fsm_parse_regex(opts, "a b", None, None).unwrap();
    print_net(&mut net, None); // to stdout
    let p = std::env::temp_dir().join("foma_s2_printnet.txt");
    let mut net = fsm_parse_regex(opts, "a b", None, None).unwrap();
    print_net(&mut net, Some(p.to_str().unwrap()));
    let s = std::fs::read_to_string(&p).unwrap();
    assert!(s.contains("Sigma:"));
    assert!(s.contains("Net:"));
    assert!(s.contains("Arity:"));
}

// print_dot (static): writes a Graphviz digraph.
// [spec:foma:sem:iface.print-dot-fn+2/test]
#[test]
fn print_dot_static_writes_digraph() {
    let opts = &FomaOptions::default();
    let mut net = fsm_parse_regex(opts, "a b", None, None).unwrap();
    print_dot(&mut net, None); // to stdout
    let p = std::env::temp_dir().join("foma_s2_printdot.dot");
    let mut net = fsm_parse_regex(opts, "a b", None, None).unwrap();
    print_dot(&mut net, Some(p.to_str().unwrap()));
    let s = std::fs::read_to_string(&p).unwrap();
    assert!(
        s.starts_with("digraph A {"),
        "got {:?}",
        &s[..s.len().min(20)]
    );
    assert!(s.trim_end().ends_with("}"));
    // Unwritable path (a file under a non-existent directory): report the error
    // and return instead of crashing.
    let mut net = fsm_parse_regex(opts, "a b", None, None).unwrap();
    print_dot(&mut net, Some("/foma_no_such_dir_xyz123/out.dot"));
}

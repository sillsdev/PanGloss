//! Closes a residual gap in cross-table representation aliasing:
//! `crate::replace::compile_metathesis_swap_net` used to render every
//! switch-position token DIRECTLY (`SegAlphabet::token`, table-blind, no cross-table alias
//! expansion) instead of through the alias-expanded path `crate::replace::RepresentationAliasMap`/
//! `SegAlphabet::render_tokens` gives ordinary rewrite rules
//! (`tests/two_table_shared_representation_recall.rs`). Fixture: `conformance-staging/edge-cases/
//! multi-table-metathesis-shared-representation` -- combines that fixture's own two-table,
//! misaligned-shared-representation structure with `right-to-left-metathesis-reversal`'s own
//! multi-member-natural-class `MetathesisRule` shape, per this closing task's own instructions.
//!
//! ## The fix: alias-expand `slot_candidates`, never text-union the swap
//! `crate::replace::slot_candidates` now expands every member `CharDefId` to every `(table, cd)`
//! pair sharing its own normalized representation (`pg_foma::replace`'s own module doc,
//! "Cross-table representation aliasing" section, has the full derivation) -- NOT by rendering a
//! bracketed union at each position the way `crate::lower::render_slots` does for ordinary rewrite
//! rules. A text-level union would be UNSAFE here: `compile_metathesis_swap_net`'s per-branch
//! construction requires the swap to reproduce the EXACT SAME value that matched at its own
//! (possibly swapped) output position, and independently unioning LHS/RHS at one position would let
//! the compiled transducer pair a matched alias with a DIFFERENT alias's token -- a new correctness
//! bug strictly worse than the false negative being fixed. Since each cross-product branch fixes
//! ONE concrete `CharDefId` per position (now possibly drawn from another table, but never a union)
//! and the swap only PERMUTES that same literal assignment vector (`rhs_vals.swap(lo, hi)`),
//! switch-position identity holds by the SAME argument the pre-existing per-branch construction
//! already relies on for ordinary (non-aliased) multi-member natural classes
//! (`tests/phase_c_metathesis.rs`'s `metathesis_multi_member_classes_transpose_precisely_not_naively`)
//! -- extended one level: "candidate member" now ranges over aliased `(table, cd)` pairs, not only
//! this table's own char-defs, but the enumeration shape that keeps the swap identity-preserving is
//! unchanged. This file's own `switch_position_identity_never_substitutes_a_different_alias` test
//! (below) pins that directly.
//!
//! ## Proven in four steps, mirroring `two_table_shared_representation_recall.rs`'s own methodology
//! 1. **The loss is real.** A hand-rendered, pre-fix-equivalent swap net (bare `SegAlphabet::token`,
//!    no alias expansion -- exactly what `compile_metathesis_swap_net` produced before this task)
//!    never fires when fed a token drawn from a DIFFERENT table's raw index for the same spelling.
//! 2. **The fix closes it.** The SAME rule, compiled via the CURRENT (fixed)
//!    `pg_foma::replace::compile_and_compose_rules_with_budget`, DOES fire on that exact material.
//! 3. **Switch-position identity holds under aliasing.** Feeding every combination of aliased and
//!    non-aliased candidates at the two switch positions, the swap ALWAYS reproduces exactly the
//!    matched values at their transposed positions -- never substituting a different alias of the
//!    same (or any other) representation.
//! 4. **Containment holds end to end** for every word this fixture's own oracle (`pg_parse::
//!    Morpher`) can actually analyze -- see the next section for why ROOT1's own cross-table word
//!    is deliberately excluded from that specific comparison, and what is asserted about it instead.
//!
//! ## A separate, out-of-scope oracle finding this file does NOT attempt to work around
//! `conformance-staging/edge-cases/multi-table-metathesis-shared-representation/STAGING.md`'s own
//! "A second, separate discovered finding" section has the full account: `pg_parse::Morpher` itself
//! (via `pg_rules::metathesis`/`pg_rules::bridge`, never `pg_foma::replace`) currently finds ZERO
//! analyses for ROOT1's correctly-metathesized surface ("xm"), for a reason narrowed to raw-index
//! misalignment but not fully root-caused within this task's own `pg-foma`-only boundary -- entirely
//! orthogonal to, and not fixed by, this task's own change. Consequently:
//! - The standard "FST propose+decode set EQUALS the oracle set" Stage-2 containment shape is
//!   pinned for ROOT2 (same-table, oracle succeeds) in
//!   `containment_holds_for_the_same_table_entry_the_oracle_can_analyze` below -- a genuine,
//!   non-vacuous check.
//! - For ROOT1 (cross-table), a full oracle-equality comparison would be VACUOUSLY true (the
//!   oracle's own set is empty for reasons unrelated to this fix) and would prove nothing about the
//!   fix. Instead, `current_compile_fires_on_table_a_originated_material_and_preserves_identity`
//!   and `fst_proposes_root1_for_its_correctly_metathesized_surface` below demonstrate the actual
//!   claim this task is responsible for -- the FST proposer's own recall -- directly against the
//!   compiled net (steps 1-3 above), the same technique `two_table_shared_representation_recall.rs`'s
//!   own steps 1-2 already established for the ordinary-rewrite case.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use foma::apply::apply_init;
use foma::constructions::fsm_compose;
use foma::lexcread::fsm_lexc_parse_string;
use foma::minimize::fsm_minimize;
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;
use foma::types::Fsm;

use pg_foma::compose_budget::ComposeBudget;
use pg_foma::replace::{compile_and_compose_rules_with_budget, SegAlphabet};
use pg_foma::tags;
use pg_foma::uflexc::emit_underlying_filtered_with_budget;
use pg_grammar::chardef::CharDefId;
use pg_grammar::model::{Grammar, LexEntryId, PhonRuleDef};
use pg_parse::{Morpher, ParseOptions};

fn fixture_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../../conformance-staging/edge-cases/multi-table-metathesis-shared-representation/grammar.xml",
    )
}

fn load() -> Grammar {
    let path = fixture_path();
    let xml = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

fn entry_id_of(g: &Grammar, xml_id: &str) -> LexEntryId {
    LexEntryId(
        g.entries
            .iter()
            .position(|e| g.morphemes[e.morpheme.0 as usize].xml_key == xml_id)
            .unwrap_or_else(|| panic!("no entry with xml id {xml_id:?}")) as u32,
    )
}

fn metathesis_rule(g: &Grammar) -> &PhonRuleDef {
    g.prules
        .iter()
        .find(|p| matches!(p, PhonRuleDef::Metathesis(r) if r.xml_id == "mrCrossTableSwap"))
        .expect("mrCrossTableSwap must be present in g.prules")
}

/// Structural sanity: exactly 2 tables, BOTH switch spellings ("m","x") shared, each at
/// deliberately misaligned raw indices -- the whole premise this fixture (and the fix) depends on.
#[test]
fn fixture_shares_both_switch_spellings_at_deliberately_misaligned_indices() {
    let g = load();
    assert_eq!(
        g.char_tables.len(),
        2,
        "fixture must declare exactly 2 tables"
    );
    let table_a = &g.char_tables[0]; // Inner
    let table_b = &g.char_tables[1]; // Outer
    assert_eq!(
        table_a.len(),
        2,
        "table A (Inner) must have exactly 2 segments (m, x)"
    );
    assert_eq!(
        table_b.len(),
        4,
        "table B (Outer) must have exactly 4 segments (z, m, x, w)"
    );

    let cd_a_m = table_a.lookup_nfd("m").expect("table A declares m");
    let cd_a_x = table_a.lookup_nfd("x").expect("table A declares x");
    let cd_b_m = table_b.lookup_nfd("m").expect("table B declares m");
    let cd_b_x = table_b.lookup_nfd("x").expect("table B declares x");
    assert_ne!(
        cd_a_m.0, cd_b_m.0,
        "\"m\" must sit at a DIFFERENT raw index in each table"
    );
    assert_ne!(
        cd_a_x.0, cd_b_x.0,
        "\"x\" must sit at a DIFFERENT raw index in each table"
    );
}

/// Step 1: THE LOSS IS REAL. Hand-render the pre-fix-equivalent construction directly: two
/// literal branches built from table B's OWN candidates only (`{m,w}` x `{x}`, no cross-table alias
/// expansion) -- exactly what `compile_metathesis_swap_net` produced before this task
/// (`slot_candidates` returning `members.clone()` verbatim). Show it fires on table B's own material
/// (positive control) but leaves table A's own "m x" material UNCHANGED -- the exact false negative
/// this task closes.
///
/// # A structural, PRE-EXISTING (not caused by this fix) over-approximation, observed and worked
/// around here rather than hidden
/// `fsm_union`-ing two OR MORE complete per-branch replace nets (this file's own `branch` closure,
/// mirroring `compile_metathesis_swap_net`'s own per-branch union) means that whenever one branch's
/// own literal LHS does NOT match a given input ANYWHERE, that branch's own net treats the WHOLE
/// input as ordinary replace-rule "elsewhere" context and passes it through UNCHANGED -- a valid
/// path through the union net, independent of any OTHER branch's own genuine rewrite. So applying
/// the union to an input that matches EXACTLY ONE branch yields TWO paths: that branch's real swap,
/// AND every OTHER (non-matching) branch's own pure-identity pass-through. This is a property of
/// ANY multi-branch metathesis construction (it already exists, unexercised, for
/// `tests/phase_c_metathesis.rs`'s own already-shipped `MULTI_MEMBER_XML` fixture, which has 4
/// branches and never checks the FST's behavior on its own RAW, un-swapped surface) -- this task's
/// aliasing fix did not introduce it, it just means MORE branches (aliased ones too) each
/// potentially contribute their own identity alternative. Safe under propose-and-confirm (an EXTRA
/// candidate the oracle/confirm engine prunes, never a missing one), so every assertion below
/// checks CONTAINS/subset, never exact `Vec` equality, to avoid conflating this pre-existing,
/// harmless noise with the actual claim (recall + no wrong-alias substitution).
#[test]
fn pre_fix_equivalent_swap_net_never_fires_on_table_a_originated_material() {
    let g = load();
    let table_a = &g.char_tables[0];
    let table_b = &g.char_tables[1];
    let alphabet_a = SegAlphabet::new(table_a);
    let alphabet_b = SegAlphabet::new(table_b);
    let opts = FomaOptions::default();

    let cd_a_m = table_a.lookup_nfd("m").unwrap();
    let cd_a_x = table_a.lookup_nfd("x").unwrap();
    let cd_b_m = table_b.lookup_nfd("m").unwrap();
    let cd_b_x = table_b.lookup_nfd("x").unwrap();
    let cd_b_w = table_b.lookup_nfd("w").unwrap();

    // Exactly the pre-fix per-branch cross product: candidates = {cd_b_m, cd_b_w} x {cd_b_x},
    // table B's own char-defs only, no aliasing.
    let branch = |lo: CharDefId, hi: CharDefId| -> Fsm {
        let lhs = format!("{}{}", alphabet_b.token(lo), alphabet_b.token(hi));
        let rhs = format!("{}{}", alphabet_b.token(hi), alphabet_b.token(lo));
        let regex = format!("{lhs} -> {rhs}");
        fsm_parse_regex(&opts, &regex, None, None)
            .unwrap_or_else(|| panic!("naive regex must compile: {regex:?}"))
    };
    let naive_net =
        foma::constructions::fsm_union(&opts, branch(cd_b_m, cd_b_x), branch(cd_b_w, cd_b_x));

    // Positive control: table B's OWN "m x" swap IS one of the naive net's outputs (module doc:
    // the OTHER branch's own identity pass-through may also legitimately appear alongside it).
    let mut h = apply_init(&naive_net);
    let table_b_mx = format!("{}{}", alphabet_b.token(cd_b_m), alphabet_b.token(cd_b_x));
    let table_b_down: HashSet<String> = h.down(&table_b_mx).collect();
    let expected_swap_b = format!("{}{}", alphabet_b.token(cd_b_x), alphabet_b.token(cd_b_m));
    assert!(
        table_b_down.contains(&expected_swap_b),
        "sanity: the naive net must correctly swap table B's OWN \"m x\": {table_b_down:?}"
    );

    // THE LOSS: table A's own "m x" (a DIFFERENT raw index for each -- what an Inner-stratum root's
    // emitted material table-blindly carries) is NOT recognized by the naive net's LHS at all --
    // EVERY path is pure identity (foma replace-rule identity-elsewhere semantics), never swapping.
    let mut h = apply_init(&naive_net);
    let table_a_mx = format!("{}{}", alphabet_a.token(cd_a_m), alphabet_a.token(cd_a_x));
    let table_a_down: HashSet<String> = h.down(&table_a_mx).collect();
    assert_eq!(
        table_a_down,
        HashSet::from([table_a_mx.clone()]),
        "THE LOSS: the pre-fix, table-blind naive net must leave table-A-originated material \
         UNCHANGED (identity) on EVERY path, never swapping it -- confirming today's real recall \
         gap this task closes: {table_a_down:?}"
    );
}

/// Step 2 + 3: THE FIX CLOSES IT, and SWITCH-POSITION IDENTITY HOLDS. Compiles the SAME rule via
/// the CURRENT (fixed) `compile_and_compose_rules_with_budget` (the real production entry point --
/// it dispatches `PhonRuleDef::Metathesis` to `crate::replace::compile_metathesis_rule` internally),
/// then:
/// - fires on table-A-originated material (the fix), and
/// - for EVERY combination of aliased ("m") and non-aliased ("w") candidates at the two switch
///   positions, the swap reproduces the matched values at their transposed positions -- never
///   substituting table B's own alias for a matched table-A token, or vice versa. This is the
///   direct, positive pin for "the construction must not let the input match one alias and the
///   output emit a different one" (module doc above has the full argument for WHY this holds by
///   construction).
///
/// Every assertion below uses CONTAINS/subset-of-`{identity, swap}` checks, never exact `Vec`
/// equality -- this task's fix ADDS more branches (aliased ones), which only INCREASES how many
/// non-matching branches may independently contribute their own harmless identity pass-through
/// alongside the genuine swap (this file's own `pre_fix_equivalent_...` test doc has the full
/// account of why this is pre-existing, safe noise, not a new bug). What must NEVER appear is any
/// THIRD output substituting a different alias for either matched value -- that is what would
/// actually violate switch-position identity, and every assertion below explicitly rules it out.
#[test]
fn current_compile_fires_on_table_a_originated_material_and_preserves_identity() {
    let g = load();
    let table_a = &g.char_tables[0];
    let table_b = &g.char_tables[1];
    let alphabet_a = SegAlphabet::new(table_a);
    let alphabet_b = SegAlphabet::new(table_b);
    let opts = FomaOptions::default();
    let budget = ComposeBudget::with_caps(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    );

    let cd_a_m = table_a.lookup_nfd("m").unwrap();
    let cd_a_x = table_a.lookup_nfd("x").unwrap();
    let cd_b_m = table_b.lookup_nfd("m").unwrap();
    let cd_b_x = table_b.lookup_nfd("x").unwrap();
    let cd_b_w = table_b.lookup_nfd("w").unwrap();

    let rule = metathesis_rule(&g);
    let mut skipped = Vec::new();
    let mut tuple_reports = Vec::new();
    let rule_net = compile_and_compose_rules_with_budget(
        &opts,
        &g,
        &alphabet_b,
        std::slice::from_ref(&rule),
        &mut skipped,
        &mut tuple_reports,
        &budget,
    )
    .unwrap_or_else(|e| panic!("mrCrossTableSwap compile must not hit any budget: {e}"))
    .expect("mrCrossTableSwap must compile to Some(net)");
    assert!(
        skipped.is_empty(),
        "mrCrossTableSwap must not be reported skipped: {skipped:?}"
    );

    // THE FIX: table A's own "m x" now swaps to "x m" (still in table A's own token space -- the
    // swap relocates, never canonicalizes, module doc's own argument for why this is safe), and
    // every OTHER output (if any) is merely the unchanged input, never a wrong substitution.
    let mut h = apply_init(&rule_net);
    let table_a_mx = format!("{}{}", alphabet_a.token(cd_a_m), alphabet_a.token(cd_a_x));
    let down: HashSet<String> = h.down(&table_a_mx).collect();
    let expected_a = format!("{}{}", alphabet_a.token(cd_a_x), alphabet_a.token(cd_a_m));
    assert!(
        down.contains(&expected_a),
        "THE FIX: table-A-originated \"m x\" must swap to table A's OWN \"x m\" -- cross-table \
         aliasing firing on Inner-stratum material: {down:?}"
    );
    assert!(
        down.is_subset(&HashSet::from([table_a_mx.clone(), expected_a.clone()])),
        "IDENTITY: no output other than the unchanged input or the correct swap may appear -- a \
         third output would mean a wrong alias was substituted: {down:?}"
    );

    // Regression control: table B's own "w x" (the non-aliased class member) still swaps correctly,
    // unaffected by the fix.
    let mut h = apply_init(&rule_net);
    let table_b_wx = format!("{}{}", alphabet_b.token(cd_b_w), alphabet_b.token(cd_b_x));
    let down: HashSet<String> = h.down(&table_b_wx).collect();
    let expected_b_w = format!("{}{}", alphabet_b.token(cd_b_x), alphabet_b.token(cd_b_w));
    assert!(
        down.contains(&expected_b_w),
        "table B's own \"w x\" must still swap correctly: {down:?}"
    );
    assert!(
        down.is_subset(&HashSet::from([table_b_wx.clone(), expected_b_w.clone()])),
        "IDENTITY: no wrong substitution for table B's own \"w x\" either: {down:?}"
    );

    // SWITCH-POSITION IDENTITY, exhaustively over every legal MIXED combination: position 1 (class
    // ncSwitchA = {m, w}) drawn from EITHER table's own "m" alias or table B's own "w" (never
    // aliased); position 2 (class ncSwitchB = {x}) drawn from EITHER table's own "x" alias. For
    // EVERY combination, the swap output set must be exactly `{identity} ⊆ output ⊆ {identity,
    // swap}` AND must contain the swap -- never any THIRD value, which would mean a DIFFERENT alias
    // of either matched representation was substituted.
    let pos1_candidates = [
        ("table A's aliased m", alphabet_a.token(cd_a_m)),
        ("table B's own m", alphabet_b.token(cd_b_m)),
        ("table B's own w (never aliased)", alphabet_b.token(cd_b_w)),
    ];
    let pos2_candidates = [
        ("table A's aliased x", alphabet_a.token(cd_a_x)),
        ("table B's own x", alphabet_b.token(cd_b_x)),
    ];
    for (name1, tok1) in pos1_candidates {
        for (name2, tok2) in pos2_candidates {
            let input = format!("{tok1}{tok2}");
            let expected_output = format!("{tok2}{tok1}");
            let mut h = apply_init(&rule_net);
            let got: HashSet<String> = h.down(&input).collect();
            assert!(
                got.contains(&expected_output),
                "IDENTITY: feeding [{name1}, {name2}] must swap to [{name2}, {name1}] among its \
                 outputs. input={input:?} got={got:?}"
            );
            assert!(
                got.is_subset(&HashSet::from([input.clone(), expected_output.clone()])),
                "IDENTITY: feeding [{name1}, {name2}] must produce NO output beyond the unchanged \
                 input or the correct swap -- never substituting a different alias of either \
                 matched representation. input={input:?} got={got:?}"
            );
        }
    }
}

/// The Stage-2 containment obligation, for the one word this fixture's own oracle (`pg_parse::
/// Morpher`) can actually analyze (ROOT2, same-table -- module doc's own "separate, out-of-scope
/// oracle finding" section explains why ROOT1 is excluded from this specific comparison). Full
/// production pipeline: `emit_underlying_filtered_with_budget` (lexc) composed with
/// `compile_and_compose_rules_with_budget` (the metathesis net), decoded via `apply_up`/`tags`.
#[test]
fn containment_holds_for_the_same_table_entry_the_oracle_can_analyze() {
    let g = load();
    let table_b = &g.char_tables[1];
    let alphabet_b = SegAlphabet::new(table_b);
    let opts = FomaOptions::default();
    let budget = ComposeBudget::with_caps(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    );

    let entry_root2 = entry_id_of(&g, "eRoot2");
    let morpheme_root2 = g.entries[entry_root2.0 as usize].morpheme.0;
    let allowed_morphemes: HashSet<u32> = [morpheme_root2].into_iter().collect();

    let mut entries = HashSet::new();
    entries.insert(entry_root2);
    let uemit = emit_underlying_filtered_with_budget(&g, &alphabet_b, Some(&entries), &budget)
        .unwrap_or_else(|e| panic!("lexc emission must not hit any budget: {e}"));
    assert!(
        uemit.skipped.is_empty(),
        "no allomorph should be skipped: {:?}",
        uemit.skipped
    );
    let lexc_net = fsm_lexc_parse_string(&opts, None, &uemit.lexc_source)
        .unwrap_or_else(|| panic!("lexc must compile:\n{}", uemit.lexc_source));

    let rule = metathesis_rule(&g);
    let mut skipped = Vec::new();
    let mut tuple_reports = Vec::new();
    let rule_net = compile_and_compose_rules_with_budget(
        &opts,
        &g,
        &alphabet_b,
        std::slice::from_ref(&rule),
        &mut skipped,
        &mut tuple_reports,
        &budget,
    )
    .unwrap_or_else(|e| panic!("mrCrossTableSwap compile must not hit any budget: {e}"))
    .expect("mrCrossTableSwap must compile to Some(net)");
    assert!(skipped.is_empty());

    let net = fsm_minimize(&opts, fsm_compose(&opts, lexc_net, rule_net));
    let morpher = Morpher::new(&g, usize::MAX);

    let fst_candidates = |word: &str| -> HashSet<(i32, Vec<u32>)> {
        let mut out = HashSet::new();
        let Some(query) = alphabet_b.encode_query(word) else {
            return out;
        };
        let mut handle = apply_init(&net);
        for s in handle.up(&query) {
            let Some(path) = tags::decode_path(&s) else {
                continue;
            };
            for c in tags::to_candidates(&path) {
                out.insert((c.root_index, c.morphemes.iter().map(|m| m.0).collect()));
            }
        }
        out
    };
    let oracle_candidates = |word: &str| -> HashSet<(i32, Vec<u32>)> {
        let outcome = morpher.parse_word_opts(word, &ParseOptions::default());
        outcome
            .structured
            .iter()
            .filter(|a| a.morpheme_ids.iter().all(|m| allowed_morphemes.contains(m)))
            .map(|a| (a.root_morpheme_index, a.morpheme_ids.clone()))
            .collect()
    };

    // --- "xw": ROOT2's correctly metathesized surface.
    let fst_xw = fst_candidates("xw");
    let oracle_xw = oracle_candidates("xw");
    assert_eq!(
        oracle_xw.len(),
        1,
        "oracle must find exactly one analysis (ROOT2) for \"xw\": {oracle_xw:?}"
    );
    assert_eq!(
        fst_xw, oracle_xw,
        "CONTAINMENT: FST propose+decode set must EQUAL the oracle set for surface \"xw\""
    );

    // --- "wx": obligatory metathesis means the raw spelling must never be an ORACLE-valid analysis
    // -- the Stage-2 obligation itself (module doc: "every analysis the oracle finds appears in the
    // proposer's set") is satisfied trivially here since the oracle's own set is empty, so no
    // subset check can fail. It is NOT asserted that the FST agrees "wx" has zero candidates: the
    // per-branch-union construction's own pre-existing, structural over-approximation
    // (`pre_fix_equivalent_swap_net_never_fires_on_table_a_originated_material`'s own doc has the
    // full account) means the compiled net ALSO proposes ROOT2 for its own raw "wx" spelling here
    // (confirmed directly: every OTHER branch's own literal LHS doesn't match "w x" anywhere, so
    // each contributes its own harmless identity pass-through alongside the genuine swap branch).
    // That is a safe, licensed FALSE POSITIVE under propose-and-confirm (an extra candidate a
    // downstream confirm step would prune, never a missing one) -- reported here as an observed
    // characteristic of the construction, not silently hidden by weakening this into a vacuous
    // check.
    let oracle_wx = oracle_candidates("wx");
    assert!(
        oracle_wx.is_empty(),
        "ROOT2's raw (un-metathesized) spelling must have no oracle analysis"
    );
    assert!(
        oracle_wx.is_subset(&fst_candidates("wx")),
        "CONTAINMENT: every oracle analysis for \"wx\" (there are none) must appear in the FST set"
    );
}

/// The RECALL half of the fix, for the word the (separate, out-of-scope) oracle gap makes a full
/// containment comparison vacuous for (module doc's own "separate, out-of-scope oracle finding"
/// section): the FST proposer, over the REAL production pipeline, DOES propose ROOT1 for its own
/// correctly-metathesized surface "xm" -- decoded via the SAME `tags::decode_path`/`to_candidates`
/// machinery every other containment check in this crate uses. This is the recall claim this task
/// is actually responsible for; it is deliberately NOT compared against `pg_parse::Morpher` here
/// (that comparison is vacuously true today, since the oracle's own set is empty for unrelated
/// reasons -- see `containment_holds_for_the_same_table_entry_the_oracle_can_analyze` above for the
/// genuine oracle-comparison witness this fixture DOES support).
#[test]
fn fst_proposes_root1_for_its_correctly_metathesized_surface() {
    let g = load();
    let table_a = &g.char_tables[0];
    let table_b = &g.char_tables[1];
    let alphabet_a = SegAlphabet::new(table_a);
    let alphabet_b = SegAlphabet::new(table_b);
    let opts = FomaOptions::default();
    let budget = ComposeBudget::with_caps(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    );

    let entry_root1 = entry_id_of(&g, "eRoot1");
    let entry_root2 = entry_id_of(&g, "eRoot2");
    let morpheme_root1 = g.entries[entry_root1.0 as usize].morpheme.0;

    let mut entries = HashSet::new();
    entries.insert(entry_root1);
    entries.insert(entry_root2);
    // Lexc emission is table-blind by construction (`SegAlphabet::encode_shape`'s own doc: the
    // formula depends only on the raw char-def index each entry's own Shape carries, never on
    // which `alphabet` object renders it) -- table B's alphabet is passed here purely because it is
    // the grammar's own last-stratum/surface table, matching every other production caller's own
    // convention (`crate::emit::surface_table`), not because it changes ROOT1's own emitted tokens.
    let uemit = emit_underlying_filtered_with_budget(&g, &alphabet_b, Some(&entries), &budget)
        .unwrap_or_else(|e| panic!("lexc emission must not hit any budget: {e}"));
    assert!(
        uemit.skipped.is_empty(),
        "no allomorph should be skipped: {:?}",
        uemit.skipped
    );
    let lexc_net = fsm_lexc_parse_string(&opts, None, &uemit.lexc_source)
        .unwrap_or_else(|| panic!("lexc must compile:\n{}", uemit.lexc_source));

    let rule = metathesis_rule(&g);
    let mut skipped = Vec::new();
    let mut tuple_reports = Vec::new();
    let rule_net = compile_and_compose_rules_with_budget(
        &opts,
        &g,
        &alphabet_b,
        std::slice::from_ref(&rule),
        &mut skipped,
        &mut tuple_reports,
        &budget,
    )
    .unwrap_or_else(|e| panic!("mrCrossTableSwap compile must not hit any budget: {e}"))
    .expect("mrCrossTableSwap must compile to Some(net)");
    assert!(skipped.is_empty());

    let net = fsm_minimize(&opts, fsm_compose(&opts, lexc_net, rule_net));

    // ROOT1's own surface, "xm", is only expressible in TABLE A's own token space (module doc's own
    // account of why: the swap relocates the exact matched values, it never canonicalizes to the
    // rule's own table) -- so the query is encoded via table A's alphabet here, not table B's. Both
    // `x` and `m` are declared in table A (the fixture's own premise), so this is a legitimate,
    // faithful use of the public `encode_query` API, not a special-cased workaround.
    let query = alphabet_a
        .encode_query("xm")
        .expect("\"xm\" must segment against table A (both \"x\" and \"m\" are declared there)");
    let mut handle = apply_init(&net);
    let mut found_root1 = false;
    for s in handle.up(&query) {
        let Some(path) = tags::decode_path(&s) else {
            continue;
        };
        for c in tags::to_candidates(&path) {
            if c.root_index >= 0 && (c.root_index as u32) == morpheme_root1 {
                found_root1 = true;
            }
        }
    }
    assert!(
        found_root1,
        "RECALL: the FST proposer must propose ROOT1 for its own correctly-metathesized surface \
         \"xm\" (encoded via table A) -- the cross-table metathesis recall this task's fix makes \
         possible, demonstrated directly against the compiled net since the oracle itself cannot \
         corroborate this specific word today (module doc's own out-of-scope finding)"
    );
}

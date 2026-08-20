//! Closes a residual gap in cross-table representation aliasing: `compile_metathesis_swap_net` used to render every switch-position token table-blind instead of through the alias-expanded path ordinary rewrite rules get, proven in four steps against the `multi-table-metathesis-shared-representation` fixture -- but never against ROOT1's cross-table word, whose oracle gap is a separate, out-of-scope finding.
//! See `docs/research/pg-foma-multi-table-metathesis-shared-representation.md` for the fix, the four-step proof, and the out-of-scope oracle finding in full.

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

/// Structural sanity: exactly 2 tables, BOTH switch spellings ("m","x") shared, each at deliberately misaligned raw indices -- the whole premise this fixture (and the fix) depends on.
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

/// Step 1: THE LOSS IS REAL. Hand-renders the pre-fix-equivalent construction (two literal branches from table B's OWN candidates only, no cross-table alias expansion): fires on table B's own material but leaves table A's "m x" material UNCHANGED -- the exact false negative this task closes.
/// See `docs/research/pg-foma-multi-table-metathesis-shared-representation.md` for why every assertion below checks CONTAINS/subset rather than exact equality.
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

    // Exactly the pre-fix per-branch cross product: candidates = {cd_b_m, cd_b_w} x {cd_b_x}, table B's own char-defs only, no aliasing.
    let branch = |lo: CharDefId, hi: CharDefId| -> Fsm {
        let lhs = format!("{}{}", alphabet_b.token(lo), alphabet_b.token(hi));
        let rhs = format!("{}{}", alphabet_b.token(hi), alphabet_b.token(lo));
        let regex = format!("{lhs} -> {rhs}");
        fsm_parse_regex(&opts, &regex, None, None)
            .unwrap_or_else(|| panic!("naive regex must compile: {regex:?}"))
    };
    let naive_net =
        foma::constructions::fsm_union(&opts, branch(cd_b_m, cd_b_x), branch(cd_b_w, cd_b_x));

    // Positive control: table B's OWN "m x" swap IS one of the naive net's outputs (the other branch's identity pass-through may also legitimately appear alongside it).
    let mut h = apply_init(&naive_net);
    let table_b_mx = format!("{}{}", alphabet_b.token(cd_b_m), alphabet_b.token(cd_b_x));
    let table_b_down: HashSet<String> = h.down(&table_b_mx).collect();
    let expected_swap_b = format!("{}{}", alphabet_b.token(cd_b_x), alphabet_b.token(cd_b_m));
    assert!(
        table_b_down.contains(&expected_swap_b),
        "sanity: the naive net must correctly swap table B's OWN \"m x\": {table_b_down:?}"
    );

    // THE LOSS: table A's own "m x" is NOT recognized by the naive net's LHS at all -- EVERY path is pure identity, never swapping.
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

/// Steps 2+3: THE FIX CLOSES IT, and SWITCH-POSITION IDENTITY HOLDS. Compiles the SAME rule via the CURRENT `compile_and_compose_rules_with_budget`, then checks it fires on table-A-originated material and never substitutes a different alias at either switch position, using CONTAINS/subset checks rather than exact equality since the fix adds branches that may harmlessly pass identity through alongside the genuine swap.
/// See `docs/research/pg-foma-multi-table-metathesis-shared-representation.md` for why this construction is safe by construction.
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

    // THE FIX: table A's own "m x" now swaps to "x m" (still in table A's own token space -- the swap relocates, never canonicalizes), and every OTHER output is merely the unchanged input.
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

    // Regression control: table B's own "w x" (the non-aliased class member) still swaps correctly, unaffected by the fix.
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

    // SWITCH-POSITION IDENTITY, exhaustively over every legal MIXED combination at both positions: the output set must contain the swap and never any THIRD value substituting a different alias.
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

/// The Stage-2 containment obligation, for the one word this fixture's oracle can actually analyze (ROOT2, same-table; ROOT1 is excluded, see this file's top doc). Full production pipeline: lexc composed with the metathesis net, decoded via `apply_up`/`tags`.
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

    // "wx": obligatory metathesis means the raw spelling must never be ORACLE-valid; the subset check below is trivially satisfied since the oracle's own set is empty -- it is NOT asserted the FST agrees, since the construction's pre-existing over-approximation also proposes ROOT2 there (a safe, licensed false positive under propose-and-confirm).
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

/// The RECALL half of the fix, for the word the out-of-scope oracle gap makes a full containment comparison vacuous for: the FST proposer, over the REAL production pipeline, DOES propose ROOT1 for its correctly-metathesized surface "xm" -- deliberately not compared against `pg_parse::Morpher` here since that comparison is vacuously true today.
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
    // Lexc emission is table-blind by construction; table B's alphabet is passed only because it's the grammar's last-stratum/surface table, matching every production caller's convention, not because it changes ROOT1's emitted tokens.
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

    // ROOT1's surface "xm" is only expressible in TABLE A's token space, since the swap relocates the matched values rather than canonicalizing them, so the query is encoded via table A's alphabet, not table B's.
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

#[test]
fn templated_backend_proposes_root1_in_the_final_query_token_space() {
    let g = load();
    let entry_root1 = entry_id_of(&g, "eRoot1");
    let morpheme_root1 = g.entries[entry_root1.0 as usize].morpheme.0;
    let mut output = pg_foma::templated_compile::compile_templated_morphotactics(&g)
        .expect("templated-underlying-tokens compile must not fail");
    let candidates = output.proposer.propose("xm");
    assert!(candidates.iter().any(|candidate| candidate.morphemes.iter().any(|m| m.0 == morpheme_root1)), "templated-underlying-tokens must bridge ROOT1's origin-table tokens into the final query token space for \"xm\": {candidates:?}");
}

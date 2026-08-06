//! The recall-side counterpart to `tests/cover_bistratal_overlapping_segment_representation.rs` (the refusal/verdict-side pin): proves cross-table representation aliasing over the real compile path.
//! See docs/research/pg-foma-two-table-shared-representation-recall-notes.md for the three-step argument and an out-of-scope pg_parse/pg_rules finding surfaced while authoring this fixture.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use foma::apply::apply_init;
use foma::constructions::fsm_compose;
use foma::lexcread::fsm_lexc_parse_string;
use foma::minimize::fsm_minimize;
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;

use pg_foma::compose_budget::ComposeBudget;
use pg_foma::replace::{compile_and_compose_rules_with_budget, SegAlphabet};
use pg_foma::tags;
use pg_foma::uflexc::emit_underlying_filtered_with_budget;
use pg_grammar::model::{Grammar, LexEntryId, PhonRuleDef};
use pg_parse::{Morpher, ParseOptions};

fn fixture_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../../conformance-staging/edge-cases/two-table-shared-representation-recall/grammar.xml",
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

fn devoice_prule(g: &Grammar) -> &PhonRuleDef {
    g.prules
        .iter()
        .find(|p| matches!(p, PhonRuleDef::Rewrite(r) if r.xml_id == "prXtoY"))
        .expect("prXtoY must be present in g.prules")
}

/// Structural sanity: exactly 2 tables, "x" shared, at DIFFERENT raw indices (the whole premise).
#[test]
fn fixture_shares_a_representation_at_deliberately_misaligned_indices() {
    let g = load();
    assert_eq!(
        g.char_tables.len(),
        2,
        "fixture must declare exactly 2 tables"
    );
    let table_a = &g.char_tables[0];
    let table_b = &g.char_tables[1];
    assert_eq!(
        table_a.len(),
        1,
        "table A (Inner) must have exactly 1 segment"
    );
    assert_eq!(
        table_b.len(),
        4,
        "table B (Outer) must have exactly 4 segments"
    );
    let cd_a_x = table_a.lookup_nfd("x").expect("table A must declare \"x\"");
    let cd_b_x = table_b
        .lookup_nfd("x")
        .expect("table B must also declare \"x\"");
    assert_ne!(
        cd_a_x.0, cd_b_x.0,
        "\"x\" must sit at a DIFFERENT raw index in each table -- the deliberate misalignment \
         this fixture's whole premise depends on"
    );
}

/// Step 1: the loss is real. Hand-rendering the pre-fix, table-blind rule shows it does not transform table A's own "x" token, though a positive control proves it correctly compiles "x -> y".
#[test]
fn pre_fix_equivalent_rule_never_fires_on_table_a_originated_material() {
    let g = load();
    let table_a = &g.char_tables[0];
    let table_b = &g.char_tables[1];
    let alphabet_a = SegAlphabet::new(table_a);
    let alphabet_b = SegAlphabet::new(table_b);
    let opts = FomaOptions::default();

    let cd_a_x = table_a.lookup_nfd("x").unwrap();
    let cd_b_x = table_b.lookup_nfd("x").unwrap();
    let cd_b_y = table_b.lookup_nfd("y").unwrap();

    // Exactly what pre-fix rendering produced for an environment-free "ncBx -> ncBy" rule: `SegAlphabet::token`, no aliasing.
    let naive_regex = format!(
        "{} -> {}",
        alphabet_b.token(cd_b_x),
        alphabet_b.token(cd_b_y)
    );
    let naive_net = fsm_parse_regex(&opts, &naive_regex, None, None)
        .unwrap_or_else(|| panic!("naive regex must compile: {naive_regex:?}"));

    // Positive control: table B's own "x" token does get rewritten, proving the naive net is not vacuously broken.
    let mut h = apply_init(&naive_net);
    let table_b_x_text = alphabet_b.token(cd_b_x).to_string();
    let table_b_down: Vec<String> = h.down(&table_b_x_text).collect();
    assert_eq!(
        table_b_down,
        vec![alphabet_b.token(cd_b_y).to_string()],
        "sanity: the naive net must correctly rewrite table B's OWN \"x\" token to \"y\""
    );

    // The loss: table A's own "x" token (a different raw index) is not recognized at all and passes through unchanged instead of becoming "y".
    let mut h = apply_init(&naive_net);
    let table_a_x_text = alphabet_a.token(cd_a_x).to_string();
    let table_a_down: Vec<String> = h.down(&table_a_x_text).collect();
    assert_eq!(
        table_a_down,
        vec![table_a_x_text.clone()],
        "THE LOSS: the pre-fix, table-blind naive net must leave table-A-originated material \
         UNCHANGED (identity), never rewriting it to \"y\" -- confirming today's real recall gap \
         this task closes: {table_a_down:?}"
    );
    assert_ne!(
        table_a_down,
        vec![alphabet_b.token(cd_b_y).to_string()],
        "the naive net must NOT produce table B's \"y\" token from table-A material"
    );
}

/// Step 2: the fix closes it. The same rule, compiled via the current, fixed compiler, fires on the exact table-A-originated material step 1 showed the naive net missing.
#[test]
fn current_compile_fires_on_table_a_originated_material() {
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

    let cd_a_x = table_a.lookup_nfd("x").unwrap();
    let cd_b_y = table_b.lookup_nfd("y").unwrap();

    let rule = devoice_prule(&g);
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
    .unwrap_or_else(|e| panic!("prXtoY compile must not hit any budget: {e}"))
    .expect("prXtoY must compile to Some(net)");
    assert!(
        skipped.is_empty(),
        "prXtoY must not be reported skipped: {skipped:?}"
    );

    let mut h = apply_init(&rule_net);
    let table_a_x_text = alphabet_a.token(cd_a_x).to_string();
    let down: Vec<String> = h.down(&table_a_x_text).collect();
    assert_eq!(
        down,
        vec![alphabet_b.token(cd_b_y).to_string()],
        "THE FIX: the CURRENT compile must rewrite table-A-originated \"x\" material to table B's \
         own \"y\" token -- cross-table aliasing firing on Inner-stratum material: {down:?}"
    );
}

/// Step 3: containment holds end to end over the real lexc-emission + rule-compile pipeline, comparing morpheme-level `(root_index, morpheme ids)` sets, never `signature()`'s surface half.
#[test]
fn fst_propose_confirm_matches_oracle_across_the_table_boundary() {
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

    let entry_root1 = entry_id_of(&g, "eRoot1");
    let entry_root2 = entry_id_of(&g, "eRoot2");
    let morpheme_root1 = g.entries[entry_root1.0 as usize].morpheme.0;
    let morpheme_root2 = g.entries[entry_root2.0 as usize].morpheme.0;
    let allowed_morphemes: HashSet<u32> = [morpheme_root1, morpheme_root2].into_iter().collect();

    let mut entries = HashSet::new();
    entries.insert(entry_root1);
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

    let rule = devoice_prule(&g);
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
    .unwrap_or_else(|e| panic!("prXtoY compile must not hit any budget: {e}"))
    .expect("prXtoY must compile to Some(net)");
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

    // "y": ROOT1 (table A) devoiced through the Outer stratum's rule; proposer and oracle sets must be equal and nonempty.
    let fst_y = fst_candidates("y");
    let oracle_y = oracle_candidates("y");
    assert_eq!(
        oracle_y.len(),
        1,
        "oracle must find exactly one analysis (ROOT1) for surface \"y\": {oracle_y:?}"
    );
    assert_eq!(
        fst_y, oracle_y,
        "CONTAINMENT: FST propose+decode set must EQUAL the oracle set for surface \"y\" -- the \
         cross-table recall this task's aliasing fix makes possible"
    );

    // "x": the rule is obligatory, so ROOT1's raw, undevoiced spelling must never be a valid surface form for either the oracle or the FST.
    let oracle_x = oracle_candidates("x");
    assert!(
        oracle_x.is_empty(),
        "ROOT1's raw (undevoiced) spelling must have no oracle analysis"
    );
    let fst_x = fst_candidates("x");
    assert_eq!(
        fst_x, oracle_x,
        "CONTAINMENT: FST and oracle must agree \"x\" has no analysis"
    );

    // --- "z": table B's own decoy segment, unattached to anything -- a plain negative control.
    let oracle_z = oracle_candidates("z");
    assert!(oracle_z.is_empty(), "\"z\" must have no oracle analysis");
    let fst_z = fst_candidates("z");
    assert_eq!(
        fst_z, oracle_z,
        "CONTAINMENT: FST and oracle must agree \"z\" has no analysis"
    );

    // "q": ROOT2 (table B), same-table and unaffected by the rule; a positive control that ordinary same-table recall is untouched by the fix.
    let fst_q = fst_candidates("q");
    let oracle_q = oracle_candidates("q");
    assert_eq!(
        oracle_q.len(),
        1,
        "oracle must find exactly one analysis (ROOT2) for \"q\""
    );
    assert_eq!(
        fst_q, oracle_q,
        "CONTAINMENT: FST and oracle must agree on \"q\""
    );
}

/// `encode_query` must always encode a concrete query word to exactly one token per segment, deterministically, never a bracketed union, regardless of any shared cross-table representation.
#[test]
fn encode_query_stays_single_token_never_ambiguous() {
    let g = load();
    let table_b = &g.char_tables[1];
    let alphabet_b = SegAlphabet::new(table_b);

    for word in ["x", "y", "z", "q"] {
        let encoded = alphabet_b
            .encode_query(word)
            .unwrap_or_else(|| panic!("{word:?} must segment against table B"));
        assert_eq!(
            encoded.chars().count(),
            1,
            "a single-segment query must encode to exactly one token, never a bracketed union: \
             {word:?} -> {encoded:?}"
        );
    }
}

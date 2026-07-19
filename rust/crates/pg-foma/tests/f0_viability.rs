//! Phase P0 viability spike (docs/fst-plan/foma-fst-plan.md §P0, gate F0).
//!
//! Proves the pure-Rust `foma` crate (crates.io v0.1.1, github.com/divvun/foma-rs) supports
//! everything the emitter design (D2/D3) needs: lexc compilation with multichar tag symbols
//! and all-paths `apply_up` enumeration (F0.1), regex-compiled replace rules composed with a
//! lexicon and applied up through the composition (F0.2), flag diacritics gating paths under
//! `apply_up` (F0.3), and non-ASCII round-tripping (F0.4, folded into the lexc section). A
//! separate module proves binary save/load round-trips both via file and via the from-memory
//! loader (`fsm_read_binary_mem`), which the browser-loading and `.bin`-cache designs (D5) will
//! depend on later.
//!
//! Every network in this file is compiled in **synthesis direction**: upper tape carries the
//! analysis (tags + underlying segments), lower tape carries the surface form — matching D2/D3.
//! We always apply **up** (surface -> analysis), matching the runtime direction in §1 of the
//! plan.

use foma::apply::apply_init;
use foma::constructions::fsm_compose;
use foma::io::{fsm_read_binary_file, fsm_read_binary_mem, fsm_write_binary, fsm_write_binary_file};
use foma::lexcread::fsm_lexc_parse_string;
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;
use foma::types::Fsm;
use std::collections::BTreeSet;

fn opts() -> FomaOptions {
    FomaOptions::default()
}

/// Collect the *full* set of `apply_up` results for `word` — exhausts the resume protocol via
/// the iterator sugar, matching how the eventual `pg-foma` decoder must enumerate every path
/// (D2: "multiple `<R:...>` in one path... split into one candidate per root" requires seeing
/// every path, not just the first).
fn up_all(net: &Fsm, word: &str) -> BTreeSet<String> {
    let mut h = apply_init(net);
    h.up(word).collect()
}

// ---------------------------------------------------------------------------------------------
// Tag escaping helpers -- a genuine gate finding, not incidental plumbing. The logic (and its
// full rationale: `%<`/`%:`/`%>` escaping for the lexc dialects, `%0` for every zero digit
// because a bare `0` is lexc's alignment-epsilon and silently collapses tag symbols) now lives
// in `pg_foma::tags` (P1 stage 1) -- these are 4-digit-width wrappers so the P0 test bodies stay
// byte-identical to what the gate originally verified, while exercising the REAL production
// codec rather than a private copy that could drift.
fn lexc_tag(prefix: &str, n: u32) -> String {
    pg_foma::tags::lexc_tag(prefix, n, 4)
}

fn tag_text(prefix: &str, n: u32) -> String {
    pg_foma::tags::tag_text(prefix, n, 4)
}

// ---------------------------------------------------------------------------------------------
// F0.1 — LEXC: Multichar_Symbols tags, two continuation classes, all-paths enumeration,
// including a word with two valid analyses (ambiguous root sharing one surface spelling), and
// a non-ASCII entry (F0.4, folded in here since it is a one-line lexc addition).
// ---------------------------------------------------------------------------------------------

fn toy_lexc() -> String {
    format!(
        r#"
Multichar_Symbols {r1} {r2} {r3} {r4} {m10} {m11}

LEXICON Root
kat{r1}:kat Suffixes ;
kat{r2}:kat Suffixes ;
dog{r3}:dog Suffixes ;
kəŋ{r4}:kəŋ Suffixes ;

LEXICON Suffixes
s{m10}:s # ;
{m11}:0 # ;
"#,
        r1 = lexc_tag("R", 1),
        r2 = lexc_tag("R", 2),
        r3 = lexc_tag("R", 3),
        r4 = lexc_tag("R", 4),
        m10 = lexc_tag("M", 10),
        m11 = lexc_tag("M", 11),
    )
}

fn compile_toy_lexc() -> Fsm {
    let src = toy_lexc();
    fsm_lexc_parse_string(&opts(), None, &src).unwrap_or_else(|| panic!("toy lexc failed to compile:\n{src}"))
}

#[test]
fn lexc_all_paths_ambiguous_word() {
    let net = compile_toy_lexc();

    // "kats" is surface-ambiguous: two Root entries (<R:0001>, <R:0002>) share the spelling
    // "kat", so apply_up must return BOTH analyses — proving all-paths enumeration, not just
    // first-match.
    let got = up_all(&net, "kats");
    let expected: BTreeSet<String> = [
        format!("kat{}s{}", tag_text("R", 1), tag_text("M", 10)),
        format!("kat{}s{}", tag_text("R", 2), tag_text("M", 10)),
    ]
    .into_iter()
    .collect();
    assert_eq!(got, expected, "expected exact set equality, all paths");
}

#[test]
fn lexc_all_paths_unambiguous_word() {
    let net = compile_toy_lexc();

    let got = up_all(&net, "dogs");
    let expected: BTreeSet<String> = [format!("dog{}s{}", tag_text("R", 3), tag_text("M", 10))]
        .into_iter()
        .collect();
    assert_eq!(got, expected);
}

#[test]
fn lexc_zero_suffix_continuation() {
    let net = compile_toy_lexc();

    // The epsilon-surface suffix (<M:0011>:0) must also enumerate both ambiguous analyses.
    let got = up_all(&net, "kat");
    let expected: BTreeSet<String> = [
        format!("kat{}{}", tag_text("R", 1), tag_text("M", 11)),
        format!("kat{}{}", tag_text("R", 2), tag_text("M", 11)),
    ]
    .into_iter()
    .collect();
    assert_eq!(got, expected);
}

#[test]
fn lexc_no_spurious_analyses() {
    let net = compile_toy_lexc();

    // A surface form with no lexicon path must come back empty, not panic or over-generate.
    assert!(up_all(&net, "xyz").is_empty());
}

#[test]
fn lexc_tags_do_not_collide_on_leading_zeros() {
    // Regression guard for the exact footgun documented above: an <R:0001> vs <R:0010>-shaped
    // pair must stay distinct once correctly escaped (they collapse to the same symbol if the
    // '0' escaping is dropped -- this test would fail loudly if that regressed).
    let src = format!(
        r#"
Multichar_Symbols {r1} {r10}

LEXICON Root
a{r1}:a # ;
b{r10}:b # ;
"#,
        r1 = lexc_tag("R", 1),
        r10 = lexc_tag("R", 10),
    );
    let net = fsm_lexc_parse_string(&opts(), None, &src).unwrap_or_else(|| panic!("lexc failed:\n{src}"));
    assert_eq!(up_all(&net, "a"), [format!("a{}", tag_text("R", 1))].into_iter().collect());
    assert_eq!(up_all(&net, "b"), [format!("b{}", tag_text("R", 10))].into_iter().collect());
}

// ---------------------------------------------------------------------------------------------
// F0.4 — UNICODE: a lexc entry with non-ASCII segments (ŋ, ə) round-trips through apply_up.
// Uses the same toy lexicon (<R:0004> root "kəŋ").
// ---------------------------------------------------------------------------------------------

#[test]
fn lexc_unicode_entry_round_trips() {
    let net = compile_toy_lexc();

    let got = up_all(&net, "kəŋ");
    let expected: BTreeSet<String> = [format!("kəŋ{}{}", tag_text("R", 4), tag_text("M", 11))]
        .into_iter()
        .collect();
    assert_eq!(got, expected);

    let got_plural = up_all(&net, "kəŋs");
    let expected_plural: BTreeSet<String> = [format!("kəŋ{}s{}", tag_text("R", 4), tag_text("M", 10))]
        .into_iter()
        .collect();
    assert_eq!(got_plural, expected_plural);
}

// ---------------------------------------------------------------------------------------------
// F0.2 — REGEX + COMPOSE: a lexicon compiled in synthesis direction (upper = tags + underlying
// segments, lower = underlying surface with an archiphonemic nasal `N`), composed with a regex
// replace rule `N -> m || _ [p|b]` (nasal place assimilation before a labial stop). The rule's
// upper side is the pre-rule (lexicon-lower) form and its lower side is the true surface, so
// `lexicon .o. rule` yields a net whose lower tape is the fully-assimilated surface and whose
// upper tape is still the untouched analysis tags. Applying up the assimilated surface must
// recover the analysis — proving composition direction and apply-up both work together.
// ---------------------------------------------------------------------------------------------

fn rule_lexc() -> String {
    format!(
        r#"
Multichar_Symbols {r} {m}

LEXICON Root
kaN{r}:kaN Suffixes ;

LEXICON Suffixes
pa{m}:pa # ;
"#,
        r = lexc_tag("R", 3001),
        m = lexc_tag("M", 4001),
    )
}

fn compile_rule_composition() -> Fsm {
    let o = opts();
    let src = rule_lexc();
    let lexicon =
        fsm_lexc_parse_string(&o, None, &src).unwrap_or_else(|| panic!("rule-composition lexc failed to compile:\n{src}"));
    let rule = fsm_parse_regex(&o, "N -> m || _ [p|b]", None, None).expect("replace rule failed to compile");
    fsm_compose(&o, lexicon, rule)
}

#[test]
fn regex_compose_recovers_underlying_form() {
    let net = compile_rule_composition();

    // The underlying concatenation "kaN" + "pa" = "kaNpa"; the rule assimilates N to m before
    // p, so the true surface is "kampa". Applying up the assimilated surface must recover the
    // tags (and, incidentally, the un-assimilated underlying "kaN" spelling on the upper tape).
    let got = up_all(&net, "kampa");
    let expected: BTreeSet<String> = [format!("kaN{}pa{}", tag_text("R", 3001), tag_text("M", 4001))]
        .into_iter()
        .collect();
    assert_eq!(got, expected, "composition must recover the underlying analysis");

    // The un-assimilated surface must NOT be accepted post-composition -- proves the rule was
    // actually applied (composition changed the surface-side requirement), not a no-op.
    assert!(
        up_all(&net, "kaNpa").is_empty(),
        "pre-assimilation surface should be rejected once the rule is composed in"
    );
}

// ---------------------------------------------------------------------------------------------
// F0.3 — FLAG DIACRITICS: a `@U.F.x@` / `@R.F.x@` unify/require pair gates one path and lets
// the other through under apply_up; flag symbols are absent from output by default
// (`show_flags` off), and are visible when explicitly enabled -- both directions of behaviour
// spot-checked, mirroring the port's own `flag_diacritics_end_to_end` test (crates/foma/src/
// apply.rs) but driven through `apply_up` (our runtime direction) instead of `apply_down`.
// ---------------------------------------------------------------------------------------------

#[test]
fn flags_gate_paths_under_apply_up() {
    let o = opts();
    // a-branch sets F to 1 or 2 (@U.F.n@ = unify: set-if-unset, else unify); c/d-branch requires
    // F to equal 1 or 2 respectively (@R.F.n@ = require exact value). "ac" is a consistent path
    // (F=1 both times); "ad" is inconsistent (F set to 1, then required to be 2) and must be
    // pruned under the default obey_flags=true.
    let net = fsm_parse_regex(
        &o,
        r#"[a "@U.F.1@" | b "@U.F.2@"] [c "@R.F.1@" | d "@R.F.2@"]"#,
        None,
        None,
    )
    .expect("flag regex failed to compile");

    let mut h = apply_init(&net);
    assert_eq!(
        h.up("ac").collect::<Vec<_>>(),
        vec!["ac".to_string()],
        "consistent flag path must survive apply_up"
    );

    let mut h2 = apply_init(&net);
    assert!(
        h2.up("ad").collect::<Vec<_>>().is_empty(),
        "flag-inconsistent path must be pruned by apply_up"
    );

    // "bd" is also consistent (F=2 both times).
    let mut h3 = apply_init(&net);
    assert_eq!(h3.up("bd").collect::<Vec<_>>(), vec!["bd".to_string()]);

    // "bc" is inconsistent (F=2, then required 1).
    let mut h4 = apply_init(&net);
    assert!(h4.up("bc").collect::<Vec<_>>().is_empty());
}

#[test]
fn flags_hidden_by_default_shown_when_enabled() {
    let o = opts();
    let net = fsm_parse_regex(&o, r#"[a "@U.F.1@"] [c "@R.F.1@"]"#, None, None).expect("flag regex failed to compile");

    // Default: show_flags = false -> flag symbols do not appear in the output string.
    let mut h = apply_init(&net);
    assert_eq!(h.up("ac").collect::<Vec<_>>(), vec!["ac".to_string()]);

    // Explicitly enabled: flag symbols render literally.
    let mut h2 = apply_init(&net);
    foma::apply::apply_set_show_flags(&mut h2, 1);
    assert_eq!(h2.up("ac").collect::<Vec<_>>(), vec!["a@U.F.1@c@R.F.1@".to_string()]);
}

#[test]
fn flags_obey_off_lets_inconsistent_path_through() {
    let o = opts();
    let net = fsm_parse_regex(
        &o,
        r#"[a "@U.F.1@" | b "@U.F.2@"] [c "@R.F.1@" | d "@R.F.2@"]"#,
        None,
        None,
    )
    .expect("flag regex failed to compile");

    let mut h = apply_init(&net);
    foma::apply::apply_set_obey_flags(&mut h, 0);
    assert_eq!(h.up("ad").collect::<Vec<_>>(), vec!["ad".to_string()]);
}

// ---------------------------------------------------------------------------------------------
// Binary save/load round-trip (plan §P0 step 7): both the file-based loader and the
// from-memory loader (`fsm_read_binary_mem`) must reproduce identical apply_up results, since
// D5's optional `.bin` cache and the browser-loading path both depend on this.
// ---------------------------------------------------------------------------------------------

#[test]
fn binary_round_trip_via_file() {
    let net = compile_toy_lexc();
    let before = up_all(&net, "kats");
    assert!(!before.is_empty());

    let path = std::env::temp_dir().join("pg_foma_f0_roundtrip_file.bin");
    fsm_write_binary_file(&net, path.to_str().unwrap()).expect("write binary file");
    let reloaded = fsm_read_binary_file(path.to_str().unwrap()).expect("read binary file");
    let _ = std::fs::remove_file(&path);

    let after = up_all(&reloaded, "kats");
    assert_eq!(before, after, "file round-trip must reproduce identical apply_up results");
}

#[test]
fn binary_round_trip_via_memory() {
    let net = compile_toy_lexc();
    let before = up_all(&net, "kats");
    assert!(!before.is_empty());

    let mut buf: Vec<u8> = Vec::new();
    fsm_write_binary(&net, &mut buf).expect("write binary to memory buffer");
    let reloaded = fsm_read_binary_mem(&buf).expect("read binary from memory buffer");

    let after = up_all(&reloaded, "kats");
    assert_eq!(before, after, "in-memory round-trip must reproduce identical apply_up results");

    // Also verify the ambiguous-word full set survives, not just non-emptiness.
    let expected: BTreeSet<String> = [
        format!("kat{}s{}", tag_text("R", 1), tag_text("M", 10)),
        format!("kat{}s{}", tag_text("R", 2), tag_text("M", 10)),
    ]
    .into_iter()
    .collect();
    assert_eq!(after, expected);
}

// ---------------------------------------------------------------------------------------------
// Gate F0 step 6 (C-foma fidelity oracle): these two `#[ignore]`d tests dump the *exact* lexc
// source strings this file compiles internally (byte-for-byte, via the same `toy_lexc()` /
// `rule_lexc()` builders the real tests use) so they can be fed to the official C foma v0.10.0
// Windows CLI (github.com/mhulden/foma releases) for a side-by-side comparison against
// foma-rs's `apply_up` output. Not part of the normal `cargo test -p pg-foma` run (no network
// dependency, no binary checked into the repo, per the plan's constraints) -- run manually with
// `cargo test -p pg-foma --test f0_viability -- --ignored --nocapture <name>` and redirect the
// dumped source into a `.lexc` file next to the C foma binaries. Results of doing this are
// recorded in the scratchpad `foma-oracle/README.md` and the P0 report: C foma's flookup
// (default direction = surface->analysis, matching our apply_up) reproduced the exact same
// analysis sets as foma-rs for every word in `toy_lexc()`, and the composed rule network
// (`rule_lexc() .o. "N -> m || _ [p|b]"`) reproduced the same result once the CLI's `compose
// net` stack-operand order was accounted for (a CLI stack-ordering detail, not a semantic
// divergence between the two implementations -- see the report).
#[test]
#[ignore]
fn print_toy_lexc_for_oracle() {
    println!("{}", toy_lexc());
}

#[test]
#[ignore]
fn print_rule_lexc_for_oracle() {
    println!("{}", rule_lexc());
}

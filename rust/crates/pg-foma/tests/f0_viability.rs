//! Viability spike for the pure-Rust `foma` crate: lexc compilation, all-paths `apply_up`, composed replace rules, flag diacritics, and Unicode/binary round-tripping.

use foma::apply::apply_init;
use foma::constructions::fsm_compose;
use foma::io::{
    fsm_read_binary_file, fsm_read_binary_mem, fsm_write_binary, fsm_write_binary_file,
};
use foma::lexcread::fsm_lexc_parse_string;
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;
use foma::types::Fsm;
use std::collections::BTreeSet;

fn opts() -> FomaOptions {
    FomaOptions::default()
}

/// Collects the full set of `apply_up` results for `word` by exhausting the resume protocol via the iterator sugar.
fn up_all(net: &Fsm, word: &str) -> BTreeSet<String> {
    let mut h = apply_init(net);
    h.up(word).collect()
}

// Thin 4-digit-width wrappers over `pg_foma::tags`'s lexc-escaping (a bare `0` is lexc's alignment-epsilon and silently collapses tag symbols), so tests exercise the real production codec.
fn lexc_tag(prefix: &str, n: u32) -> String {
    pg_foma::tags::lexc_tag(prefix, n, 4)
}

fn tag_text(prefix: &str, n: u32) -> String {
    pg_foma::tags::tag_text(prefix, n, 4)
}

// LEXC: Multichar_Symbols tags, two continuation classes, all-paths enumeration, an ambiguous word, and a non-ASCII entry.

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
    fsm_lexc_parse_string(&opts(), None, &src)
        .unwrap_or_else(|| panic!("toy lexc failed to compile:\n{src}"))
}

#[test]
fn lexc_all_paths_ambiguous_word() {
    let net = compile_toy_lexc();

    // "kats" is surface-ambiguous (two Root entries share the spelling "kat"), so apply_up must return both analyses.
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
    // Regression guard: `<R:0001>` vs `<R:0010>` must stay distinct once escaped -- they collapse to the same symbol if the leading-zero escaping is dropped.
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
    let net =
        fsm_lexc_parse_string(&opts(), None, &src).unwrap_or_else(|| panic!("lexc failed:\n{src}"));
    assert_eq!(
        up_all(&net, "a"),
        [format!("a{}", tag_text("R", 1))].into_iter().collect()
    );
    assert_eq!(
        up_all(&net, "b"),
        [format!("b{}", tag_text("R", 10))].into_iter().collect()
    );
}

// UNICODE: a lexc entry with non-ASCII segments round-trips through apply_up.

#[test]
fn lexc_unicode_entry_round_trips() {
    let net = compile_toy_lexc();

    let got = up_all(&net, "kəŋ");
    let expected: BTreeSet<String> = [format!("kəŋ{}{}", tag_text("R", 4), tag_text("M", 11))]
        .into_iter()
        .collect();
    assert_eq!(got, expected);

    let got_plural = up_all(&net, "kəŋs");
    let expected_plural: BTreeSet<String> =
        [format!("kəŋ{}s{}", tag_text("R", 4), tag_text("M", 10))]
            .into_iter()
            .collect();
    assert_eq!(got_plural, expected_plural);
}

// REGEX + COMPOSE: `lexicon .o. rule` composes a synthesis-direction lexicon (upper=tags+underlying, lower=underlying with archiphonemic `N`) with a surface-assimilation replace rule, so the result's lower tape is the true surface and upper tape is still the untouched analysis tags.

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
    let lexicon = fsm_lexc_parse_string(&o, None, &src)
        .unwrap_or_else(|| panic!("rule-composition lexc failed to compile:\n{src}"));
    let rule = fsm_parse_regex(&o, "N -> m || _ [p|b]", None, None)
        .expect("replace rule failed to compile");
    fsm_compose(&o, lexicon, rule)
}

#[test]
fn regex_compose_recovers_underlying_form() {
    let net = compile_rule_composition();

    // "kaN"+"pa"="kaNpa" underlying; the rule assimilates N to m before p, giving surface "kampa", whose apply_up must recover the tags.
    let got = up_all(&net, "kampa");
    let expected: BTreeSet<String> = [format!(
        "kaN{}pa{}",
        tag_text("R", 3001),
        tag_text("M", 4001)
    )]
    .into_iter()
    .collect();
    assert_eq!(
        got, expected,
        "composition must recover the underlying analysis"
    );

    // The un-assimilated surface must not be accepted post-composition, proving the rule was actually applied.
    assert!(
        up_all(&net, "kaNpa").is_empty(),
        "pre-assimilation surface should be rejected once the rule is composed in"
    );
}

// FLAG DIACRITICS: a `@U.F.x@`/`@R.F.x@` unify/require pair gates paths under `apply_up`, mirroring the `foma` crate's own flag-diacritics test via `apply_up` instead of `apply_down`.

#[test]
fn flags_gate_paths_under_apply_up() {
    let o = opts();
    // `@U.F.n@` unifies (set-if-unset else unify) F; `@R.F.n@` requires F equal n. "ac"/"bd" are consistent (pass); "ad"/"bc" are inconsistent (pruned) under the default `obey_flags=true`.
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
    let net = fsm_parse_regex(&o, r#"[a "@U.F.1@"] [c "@R.F.1@"]"#, None, None)
        .expect("flag regex failed to compile");

    // Default: show_flags = false -> flag symbols do not appear in the output string.
    let mut h = apply_init(&net);
    assert_eq!(h.up("ac").collect::<Vec<_>>(), vec!["ac".to_string()]);

    // Explicitly enabled: flag symbols render literally.
    let mut h2 = apply_init(&net);
    foma::apply::apply_set_show_flags(&mut h2, 1);
    assert_eq!(
        h2.up("ac").collect::<Vec<_>>(),
        vec!["a@U.F.1@c@R.F.1@".to_string()]
    );
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

// Binary round-trip: both the file and from-memory loaders must reproduce identical apply_up results.

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
    assert_eq!(
        before, after,
        "file round-trip must reproduce identical apply_up results"
    );
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
    assert_eq!(
        before, after,
        "in-memory round-trip must reproduce identical apply_up results"
    );

    // Also verify the ambiguous-word full set survives, not just non-emptiness.
    let expected: BTreeSet<String> = [
        format!("kat{}s{}", tag_text("R", 1), tag_text("M", 10)),
        format!("kat{}s{}", tag_text("R", 2), tag_text("M", 10)),
    ]
    .into_iter()
    .collect();
    assert_eq!(after, expected);
}

// C-foma fidelity oracle (manual, `--ignored`): dumps lexc source for comparison against the
// official C foma CLI; see `docs/research/pg-foma-f0-viability-oracle.md`.
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

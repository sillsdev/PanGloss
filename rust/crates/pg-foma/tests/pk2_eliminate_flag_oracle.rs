//! PK2 (docs/superpowers/specs/2026-07-15-fst-precision-knob-design.md §8 sequencing item (2)):
//! the C-foma oracle gate for `eliminate flag` fidelity. Spec §5's load-bearing risk: foma-rs's
//! `flag_eliminate` (the `foma` crate, pinned `=0.1.1`, `src/flags.rs`) is "the least-tested corner
//! of foma (upstream bugs where flags interact with `_eq`, github.com/mhulden/foma issue #60)".
//! Before the tuner's `Eliminate` arm (spec §1 position 1) is ever enabled, per-attribute
//! elimination must be equivalence-tested against the real C foma oracle. On any mismatch the
//! design must degrade to `AllFlags` (spec §5: "never to wrong").
//!
//! ## Method
//!
//! Every network below is a single Rust `&str` regex source, shared VERBATIM between:
//!   - **foma-rs** (`foma::regex::fsm_parse_regex` + `foma::flags::flag_eliminate` +
//!     `foma::apply::apply_init`/`ApplyHandle::up`), and
//!   - **real C foma 0.10.0alpha** running under WSL (`wsl foma -q -f script.foma` to compile
//!     +`eliminate flag ATTR` + `save stack net.fst`, then `wsl flookup net.fst` to batch-apply;
//!     `flookup`'s DEFAULT direction is apply-UP, matching `ApplyHandle::up` -- verified empirically
//!     below, see `win_to_wsl_path`'s callers).
//!
//! For each (network, attribute) pair we compute four "legs" over the same fixed word list and
//! assert they agree as SETS per word (a word with no legal analysis maps to the empty set; C-foma
//! prints `+?` for this, filtered out in `parse_flookup_output`):
//!   1. **foma-rs baseline**: flags left in the network; `apply_up` obeys them (`ApplyHandle`'s
//!      `obey_flags` defaults to 1 in `foma-0.1.1/src/apply.rs::apply_init` -- verified empirically
//!      by `rs_flags_obeyed_by_default_baseline` below, since v1's live pipeline (`FomaProposer`)
//!      currently only ever STRIPS flags before this point and so has never exercised this path).
//!   2. **foma-rs eliminated**: `flag_eliminate(opts, net, Some(attr))`, then `apply_up`.
//!   3. **C-foma baseline**: same source, `save stack` with no elimination, `flookup` (apply-up).
//!   4. **C-foma eliminated**: same source + `eliminate flag ATTR`, `save stack`, `flookup`.
//!
//! Legs 3-4 (anything that shells to `wsl`) SKIP GRACEFULLY (`eprintln!` + early return) when
//! `wsl foma`/`wsl flookup` are unavailable, matching this crate's tolerance for missing external
//! tooling on other machines/CI. Legs 1-2 (foma-rs-internal) always run.
//!
//! A mismatch found by this file is a SUCCESSFUL gate finding, not something to hide: where real
//! divergence exists (see the E-flag-type test at the bottom) the test asserts the divergence
//! itself and documents it, rather than papering over it.
//!
//! ## Battery (task item 2, a-e)
//! - `battery_a_unify_agreement_across_stem_boundary` -- (a) Beesley & Karttunen separated
//!   dependency: determiner/noun NUM agreement via `@U@`.
//! - `battery_b_positive_require_and_disallow_combos` -- (b) `@P@`+`@R@` and `@P@`+`@D@`.
//! - `battery_c_three_independent_attributes_chained_elimination` -- (c) three flags (one pair
//!   with a prefix-colliding name, `NUM`/`NUMBER`, to stress `flag_purge`'s name-boundary guard)
//!   eliminated one at a time, checked at every checkpoint (Karttunen-style chain).
//! - `battery_d_flags_coexist_with_multichar_tags` -- (d) `<R:0001>`-shaped tag symbols alongside
//!   flags; asserts elimination never touches the tag.
//! - `battery_e_reduplication_shaped_flags_and_affix_issue60_risk` -- (e) the closest reproducible
//!   analog of issue #60's crash shape (flag diacritics + a reduplication-shaped stem + affixation);
//!   true generative reduplication is not a regular-language operation foma-rs/C-foma's regex
//!   parser exposes (and out of pg-foma's FST scope per the design doc's "reduplication stays the
//!   peel"), so this uses finite PRE-COPIED stems (`catcat`, `dogdog`) standing in for a
//!   reduplicated shape -- documented here, not hidden. `_eq` in issue #60 turned out to be the
//!   REPORTER's own xfst function name, not a foma builtin (confirmed via the issue text) -- there
//!   is no `_eq(...)` construct in foma-rs's regex parser to substitute for.
//!
//! Plus:
//! - `rs_flags_obeyed_by_default_baseline` -- the load-bearing assumption every leg above depends
//!   on, checked first.
//! - `e_flag_type_elimination_not_equivalence_preserving` -- `@E@` (FLAG_EQUAL) is a DIFFERENT,
//!   separately-discovered divergence (not issue #60): `foma-0.1.1/src/flags.rs`'s `flag_build`
//!   row table (a literal bug-for-bug port of the real C table) has NO rows with the eliminated
//!   flag's type == `FLAG_EQUAL`, so eliminating an E-attribute never builds a filter -- it silently
//!   degrades to Strip (illegal paths become reachable) while still calling itself "eliminated".
//!   **This is the headline finding of this file, not a footnote**: spec §5's gate criterion
//!   ("apply_up set-equality between foma-rs and C-foma") is NECESSARY BUT NOT SUFFICIENT. E
//!   PASSES that oracle check (both engines agree: eliminated = `{a,b}`, since foma is a
//!   bug-for-bug port) while VIOLATING spec §1's equivalence-preservation invariant (eliminated
//!   `{a,b}` != keepflag/baseline `{}`). A tuner that only ran the §5 oracle check would wrongly
//!   enable Eliminate for an E-tester. The real per-attribute gate must ALSO assert
//!   `eliminated == baseline` WITHIN one engine (this file already computes both sides of that
//!   check for every battery). Which direction is "wrong" here (does legit `@E.F.1@` semantics
//!   make `a`/`b` legal or illegal?) is NOT resolved by this investigation -- so no arm is
//!   asserted safe for E; only that Eliminate is unsafe for it. Structurally this generalizes:
//!   `flag_build`'s table only has rows for eliminated-type U/R/D, so ANY eliminated type absent
//!   from those rows (E confirmed; N/C/P are structurally identical - no rows either) silently
//!   strips instead of eliminating. The positive verdict below is scoped to U/R/D accordingly.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use foma::apply::apply_init;
use foma::flags::{flag_check, flag_eliminate};
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;
use foma::types::Fsm;

/* ------------------------------------------------------------------------------------------- */
/* foma-rs side                                                                                 */
/* ------------------------------------------------------------------------------------------- */

/// Compile `source`, `apply_up` every word in `words`, with no elimination (flags obeyed at
/// runtime -- this crate's "baseline"/"flagged" leg).
fn rs_baseline_up(source: &str, words: &[&str]) -> BTreeMap<String, BTreeSet<String>> {
    let opts = FomaOptions::default();
    let net = fsm_parse_regex(&opts, source, None, None)
        .unwrap_or_else(|| panic!("foma-rs failed to compile baseline source: {source}"));
    apply_up_all(&net, words)
}

/// Compile `source`, eliminate each attribute in `attrs` IN ORDER (chained -- each call sees the
/// previous elimination's output network, matching C-foma's `eliminate flag` REPL semantics of
/// successive commands against the same stack), then `apply_up` every word.
fn rs_eliminated_up(
    source: &str,
    attrs: &[&str],
    words: &[&str],
) -> BTreeMap<String, BTreeSet<String>> {
    let opts = FomaOptions::default();
    let mut net = fsm_parse_regex(&opts, source, None, None)
        .unwrap_or_else(|| panic!("foma-rs failed to compile source for elimination: {source}"));
    for attr in attrs {
        net = flag_eliminate(&opts, net, Some(attr));
    }
    apply_up_all(&net, words)
}

fn apply_up_all(net: &Fsm, words: &[&str]) -> BTreeMap<String, BTreeSet<String>> {
    let mut handle = apply_init(net);
    let mut out = BTreeMap::new();
    for &w in words {
        let set: BTreeSet<String> = handle.up(w).collect();
        out.insert(w.to_string(), set);
    }
    out
}

/* ------------------------------------------------------------------------------------------- */
/* C-foma (WSL) side                                                                            */
/* ------------------------------------------------------------------------------------------- */

/// Whether `wsl foma` and `wsl flookup` are both callable on this machine. Cached: spawning WSL is
/// slow (~1-2s) and every C-foma leg checks this first.
fn wsl_available() -> bool {
    static CACHE: OnceLock<bool> = OnceLock::new();
    *CACHE.get_or_init(|| {
        let check = Command::new("wsl")
            .args([
                "-e",
                "bash",
                "-lc",
                "command -v foma >/dev/null 2>&1 && command -v flookup >/dev/null 2>&1",
            ])
            .output();
        matches!(check, Ok(o) if o.status.success())
    })
}

/// Scratch directory for this test binary's generated `.foma` scripts / `.fst` binaries.
/// `CARGO_TARGET_TMPDIR` is cargo's own per-test-target tmp dir (stable across runs) -- this file
/// must be self-contained on any machine, not tied to any one session's scratch path.
fn scratch_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("pk2_oracle");
    std::fs::create_dir_all(&dir).expect("create pk2_oracle scratch dir");
    dir
}

/// Convert a Windows path (`C:\Users\...`) to WSL's `/mnt/c/...` form.
fn win_to_wsl_path(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let bytes = s.as_bytes();
    assert!(
        bytes.len() > 2 && bytes[1] == b':',
        "expected a drive-letter path, got {s:?}"
    );
    let drive = (bytes[0] as char).to_ascii_lowercase();
    format!("/mnt/{drive}{}", &s[2..])
}

/// Outcome of driving a C-foma script through `wsl foma -q -f`. `Failed` is itself a gate finding
/// (spec §5's crash risk, github.com/mhulden/foma issue #60) -- callers must handle it and report
/// it, never let it propagate as a Rust panic that would take down the whole test binary.
enum CFomaOutcome {
    Ok {
        fst_path: PathBuf,
    },
    Failed {
        status: String,
        stdout: String,
        stderr: String,
    },
}

/// Write `regex source; [eliminate flag ATTR ...]; save stack TAG.fst` to `TAG.foma` under the
/// scratch dir, run it under `wsl foma -q -f`, and report whether `TAG.fst` was produced.
fn run_c_foma_script(tag: &str, regex_source: &str, eliminate_attrs: &[&str]) -> CFomaOutcome {
    let dir = scratch_dir();
    let script_path = dir.join(format!("{tag}.foma"));
    let fst_path = dir.join(format!("{tag}.fst"));
    let wsl_fst = win_to_wsl_path(&fst_path);

    let mut script = format!("regex {regex_source};\n");
    for attr in eliminate_attrs {
        script.push_str(&format!("eliminate flag {attr}\n"));
    }
    script.push_str(&format!("save stack {wsl_fst}\n"));
    std::fs::write(&script_path, &script).expect("write foma script");
    let wsl_script = win_to_wsl_path(&script_path);

    // Remove any stale .fst from a previous run so a "Failed" outcome can't be masked by an old file.
    let _ = std::fs::remove_file(&fst_path);

    let output = match Command::new("wsl")
        .args(["foma", "-q", "-f", &wsl_script])
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            return CFomaOutcome::Failed {
                status: format!("spawn error: {e}"),
                stdout: String::new(),
                stderr: String::new(),
            };
        }
    };
    if output.status.success() && fst_path.exists() {
        CFomaOutcome::Ok { fst_path }
    } else {
        CFomaOutcome::Failed {
            status: format!("{:?}", output.status),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }
}

/// Batch-apply `words` against a saved binary net via `wsl flookup`. `flookup`'s DEFAULT direction
/// (no `-i`) is apply-UP (empirically verified: on `regex a:x | a:y;`, `flookup` with no flag fed
/// "x" returns "a", and `flookup -i` fed "a" returns {"x","y"} -- i.e. unflagged = up, `-i` = down),
/// matching `ApplyHandle::up`. Returns word -> set of upper-side results (`+?` => empty set, i.e.
/// no analysis).
fn flookup_up(fst_path: &Path, words: &[&str]) -> BTreeMap<String, BTreeSet<String>> {
    let wsl_fst = win_to_wsl_path(fst_path);
    let mut child = Command::new("wsl")
        .args(["flookup", &wsl_fst])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn wsl flookup");
    {
        let stdin = child.stdin.as_mut().expect("flookup stdin");
        for w in words {
            writeln!(stdin, "{w}").expect("write to flookup stdin");
        }
    }
    let output = child.wait_with_output().expect("wait for wsl flookup");
    let mut out: BTreeMap<String, BTreeSet<String>> = words
        .iter()
        .map(|&w| (w.to_string(), BTreeSet::new()))
        .collect();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        let Some((word, result)) = line.split_once('\t') else {
            continue;
        };
        if result != "+?" {
            out.entry(word.to_string())
                .or_default()
                .insert(result.to_string());
        }
    }
    out
}

/// Full C-foma leg: compile (+ optional chained eliminations), save, flookup. Returns `None` (with
/// an `eprintln!` explaining why) when WSL is unavailable OR when foma itself failed/crashed on
/// this script -- both are graceful-skip conditions for callers, but the crash case is additionally
/// a reportable finding, which callers should log.
fn c_foma_leg(
    tag: &str,
    regex_source: &str,
    eliminate_attrs: &[&str],
    words: &[&str],
) -> Option<BTreeMap<String, BTreeSet<String>>> {
    if !wsl_available() {
        eprintln!("SKIP {tag}: `wsl foma`/`wsl flookup` not available on this machine");
        return None;
    }
    match run_c_foma_script(tag, regex_source, eliminate_attrs) {
        CFomaOutcome::Ok { fst_path } => Some(flookup_up(&fst_path, words)),
        CFomaOutcome::Failed {
            status,
            stdout,
            stderr,
        } => {
            eprintln!(
                "FINDING {tag}: C-foma script FAILED (status={status}) -- this is itself a gate \
                 result (spec §5 / issue #60 crash risk), not a harness bug:\nstdout:\n{stdout}\nstderr:\n{stderr}"
            );
            None
        }
    }
}

/* ------------------------------------------------------------------------------------------- */
/* Shared assertion helper                                                                      */
/* ------------------------------------------------------------------------------------------- */

/// Print a word -> legal-analyses table for `label`, for both eyeballing and the report.
fn print_table(label: &str, map: &BTreeMap<String, BTreeSet<String>>) {
    println!("-- {label} --");
    for (word, set) in map {
        let rendered = if set.is_empty() {
            "FAIL".to_string()
        } else {
            format!("{:?}", set.iter().collect::<Vec<_>>())
        };
        println!("  {word:?} -> {rendered}");
    }
}

/* ------------------------------------------------------------------------------------------- */
/* Shared network sources                                                                       */
/* ------------------------------------------------------------------------------------------- */

/// The exact network from `foma-0.1.1/src/flags.rs`'s own `flag_eliminate_end_to_end` unit test,
/// covering U/R/D flags across three independently-named attributes F, G, H (task item 2b:
/// `@P.F.x@`+`@U.F.x@`(unify), `@P.G.x@`+`@R.G@`(require), `@P.H.x@`+`@D.H@`(disallow)).
/// Verified against real C foma via `wsl foma -q -f` / `apply up` during investigation: legal set
/// = {a, c, e}.
const FLAGTEST_SRC: &str = r#"["@P.F.1@" a "@U.F.1@"] | ["@P.F.2@" b "@U.F.1@"] | ["@P.G.1@" c "@R.G@"] | [d "@R.G@"] | [e "@D.H@"] | ["@P.H.1@" f "@D.H@"]"#;
const FLAGTEST_WORDS: [&str; 6] = ["a", "b", "c", "d", "e", "f"];

/* ------------------------------------------------------------------------------------------- */
/* Load-bearing assumption: foma-rs obeys flags by default                                      */
/* ------------------------------------------------------------------------------------------- */

#[test]
fn rs_flags_obeyed_by_default_baseline() {
    // Every "baseline" leg in this file depends on ApplyHandle obeying flag diacritics by
    // default (foma-0.1.1/src/apply.rs::apply_init sets `obey_flags = 1`). v1's live pipeline
    // (FomaProposer) currently only ever STRIPS flags before this point (spec §1 position 3),
    // so this has never actually been exercised on a still-flagged network before -- verify it
    // directly rather than trusting the source read.
    let got = rs_baseline_up(FLAGTEST_SRC, &FLAGTEST_WORDS);
    print_table("rs baseline (flags obeyed?)", &got);
    let legal: BTreeSet<String> = got
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, _)| k.clone())
        .collect();
    let expected: BTreeSet<String> = ["a", "c", "e"].iter().map(|s| s.to_string()).collect();
    assert_eq!(
        legal, expected,
        "foma-rs must obey flag diacritics by default on an un-eliminated network \
         (if this fails, every other test's 'baseline' leg is measuring the wrong thing)"
    );
}

/* ------------------------------------------------------------------------------------------- */
/* (a) Unify agreement across a stem boundary (Beesley & Karttunen separated dependency)        */
/* ------------------------------------------------------------------------------------------- */

#[test]
fn battery_a_unify_agreement_across_stem_boundary() {
    // Determiner/noun NUM agreement: two positions, not locally adjacent in the grammar sense
    // (separated by the flag mechanism, not string adjacency), must agree via the SAME @U@
    // attribute. Classic Beesley 1998 / B&K 2003 ch.7 "separated dependency" pattern.
    let src = r#"[["@U.NUM.sg@" t h e] | ["@U.NUM.pl@" t h e s e]] [["@U.NUM.sg@" c a t] | ["@U.NUM.pl@" c a t s]]"#;
    let words = ["thecat", "thesecats", "thecats", "thesecat"];
    let legal_expected: BTreeSet<String> = ["thecat", "thesecats"]
        .iter()
        .map(|s| s.to_string())
        .collect();

    let baseline = rs_baseline_up(src, &words);
    print_table("battery_a rs baseline", &baseline);
    let baseline_legal: BTreeSet<String> = baseline
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, _)| k.clone())
        .collect();
    assert_eq!(
        baseline_legal, legal_expected,
        "baseline: only matching-NUM combos should be legal"
    );

    let eliminated = rs_eliminated_up(src, &["NUM"], &words);
    print_table("battery_a rs eliminated(NUM)", &eliminated);
    assert_eq!(
        baseline, eliminated,
        "battery_a: foma-rs baseline vs NUM-eliminated must agree as sets per word"
    );

    let Some(c_baseline) = c_foma_leg("battery_a_baseline", src, &[], &words) else {
        return;
    };
    print_table("battery_a C-foma baseline", &c_baseline);
    assert_eq!(
        baseline, c_baseline,
        "battery_a: foma-rs vs C-foma baseline must agree"
    );

    let Some(c_eliminated) = c_foma_leg("battery_a_elim_num", src, &["NUM"], &words) else {
        return;
    };
    print_table("battery_a C-foma eliminated(NUM)", &c_eliminated);
    assert_eq!(
        eliminated, c_eliminated,
        "battery_a: foma-rs vs C-foma eliminated(NUM) must agree"
    );
}

/* ------------------------------------------------------------------------------------------- */
/* (b) @P@+@R@ and @P@+@D@ combos (plus @P@+@U@)                                                */
/* ------------------------------------------------------------------------------------------- */

#[test]
fn battery_b_positive_require_and_disallow_combos() {
    let src = FLAGTEST_SRC;
    let words = FLAGTEST_WORDS;
    let baseline = rs_baseline_up(src, &words);
    print_table("battery_b rs baseline", &baseline);

    for attr in ["F", "G", "H"] {
        let eliminated = rs_eliminated_up(src, &[attr], &words);
        print_table(&format!("battery_b rs eliminated({attr})"), &eliminated);
        assert_eq!(
            baseline, eliminated,
            "battery_b: foma-rs baseline vs {attr}-eliminated must agree as sets per word"
        );

        let tag = format!("battery_b_elim_{attr}");
        let Some(c_eliminated) = c_foma_leg(&tag, src, &[attr], &words) else {
            continue;
        };
        print_table(
            &format!("battery_b C-foma eliminated({attr})"),
            &c_eliminated,
        );
        assert_eq!(
            eliminated, c_eliminated,
            "battery_b: foma-rs vs C-foma eliminated({attr}) must agree"
        );
    }

    let Some(c_baseline) = c_foma_leg("battery_b_baseline", src, &[], &words) else {
        return;
    };
    print_table("battery_b C-foma baseline", &c_baseline);
    assert_eq!(
        baseline, c_baseline,
        "battery_b: foma-rs vs C-foma baseline must agree"
    );

    // Task item 2b explicitly asks for `@P.F.x@` + `@R.F.x@` -- i.e. REQUIRE *with a value*, a
    // distinct set of rows in flag_build's decision table ("R flag, with value", keyed on
    // `null_req = Some(false)`) from the valueless `@R.G@` already covered by FLAGTEST_SRC above.
    // K set to 1 then required to equal 1 -> legal; K set to 2 then required to equal 1 -> illegal.
    let rvalue_src = r#"["@P.K.1@" g "@R.K.1@"] | ["@P.K.2@" h "@R.K.1@"]"#;
    let rvalue_words = ["g", "h"];
    let rvalue_legal: BTreeSet<String> = ["g"].iter().map(|s| s.to_string()).collect();

    let rv_baseline = rs_baseline_up(rvalue_src, &rvalue_words);
    print_table("battery_b (R-with-value) rs baseline", &rv_baseline);
    let rv_baseline_legal: BTreeSet<String> = rv_baseline
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, _)| k.clone())
        .collect();
    assert_eq!(
        rv_baseline_legal, rvalue_legal,
        "battery_b (R-with-value): baseline sanity"
    );

    let rv_eliminated = rs_eliminated_up(rvalue_src, &["K"], &rvalue_words);
    print_table("battery_b (R-with-value) rs eliminated(K)", &rv_eliminated);
    assert_eq!(
        rv_baseline, rv_eliminated,
        "battery_b (R-with-value): foma-rs baseline vs K-eliminated must agree as sets per word"
    );

    let Some(rv_c_baseline) =
        c_foma_leg("battery_b_rvalue_baseline", rvalue_src, &[], &rvalue_words)
    else {
        return;
    };
    print_table("battery_b (R-with-value) C-foma baseline", &rv_c_baseline);
    assert_eq!(
        rv_baseline, rv_c_baseline,
        "battery_b (R-with-value): foma-rs vs C-foma baseline must agree"
    );

    let Some(rv_c_eliminated) =
        c_foma_leg("battery_b_rvalue_elim_k", rvalue_src, &["K"], &rvalue_words)
    else {
        return;
    };
    print_table(
        "battery_b (R-with-value) C-foma eliminated(K)",
        &rv_c_eliminated,
    );
    assert_eq!(
        rv_eliminated, rv_c_eliminated,
        "battery_b (R-with-value): foma-rs vs C-foma eliminated(K) must agree"
    );
}

/* ------------------------------------------------------------------------------------------- */
/* (c) Three independent attributes, chained elimination (Karttunen-style), incl. a             */
/*     prefix-colliding attribute-name pair to stress flag_purge's name-boundary guard           */
/* ------------------------------------------------------------------------------------------- */

#[test]
fn battery_c_three_independent_attributes_chained_elimination() {
    // pos1/pos2: NUM agreement (must match, like battery_a). pos3: NUMBER -- a name that is a
    // PREFIX of "NUM" reversed / NUM is a prefix of NUMBER -- stresses flag_purge's name-boundary
    // check (foma-0.1.1/src/flags.rs::flag_purge: `csym.starts_with(name_b) && csym.len() >
    // name_b.len() && (csym[name_b.len()] == b'.' || == b'@')`), independent of NUM. pos4: CASE,
    // independent of both.
    let src = r#"[["@U.NUM.sg@" a] | ["@U.NUM.pl@" p]] [["@U.NUM.sg@" b] | ["@U.NUM.pl@" q]] [["@U.NUMBER.x@" c] | ["@U.NUMBER.y@" d]] [["@U.CASE.nom@" e] | ["@U.CASE.acc@" f]]"#;
    let words = [
        "abce", "abcf", "abde", "abdf", "pqce", "pqcf", "pqde", "pqdf", // legal: NUM agrees
        "aqce", "pbce", "aqde", "pbdf", // illegal: NUM mismatch (pos1 vs pos2)
    ];
    let legal_expected: BTreeSet<String> = [
        "abce", "abcf", "abde", "abdf", "pqce", "pqcf", "pqde", "pqdf",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    let baseline = rs_baseline_up(src, &words);
    print_table("battery_c rs baseline", &baseline);
    let baseline_legal: BTreeSet<String> = baseline
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, _)| k.clone())
        .collect();
    assert_eq!(
        baseline_legal, legal_expected,
        "battery_c: baseline legal set sanity check"
    );

    // Checkpoint after each incremental elimination in the chain NUM -> NUMBER -> CASE.
    let chain: [&[&str]; 3] = [&["NUM"], &["NUM", "NUMBER"], &["NUM", "NUMBER", "CASE"]];
    for attrs in chain {
        let eliminated = rs_eliminated_up(src, attrs, &words);
        print_table(&format!("battery_c rs eliminated({attrs:?})"), &eliminated);
        assert_eq!(
            baseline, eliminated,
            "battery_c: baseline vs eliminated({attrs:?}) must agree at every checkpoint \
             (regression here would mean chained elimination corrupts an unrelated attribute, \
             e.g. NUM vs NUMBER name-prefix collision)"
        );

        let tag = format!("battery_c_elim_{}", attrs.join("_"));
        let Some(c_eliminated) = c_foma_leg(&tag, src, attrs, &words) else {
            continue;
        };
        print_table(
            &format!("battery_c C-foma eliminated({attrs:?})"),
            &c_eliminated,
        );
        assert_eq!(
            eliminated, c_eliminated,
            "battery_c: foma-rs vs C-foma eliminated({attrs:?}) must agree"
        );
    }

    let Some(c_baseline) = c_foma_leg("battery_c_baseline", src, &[], &words) else {
        return;
    };
    print_table("battery_c C-foma baseline", &c_baseline);
    assert_eq!(
        baseline, c_baseline,
        "battery_c: foma-rs vs C-foma baseline must agree"
    );
}

/* ------------------------------------------------------------------------------------------- */
/* (d) Flags coexisting with multichar tag symbols (must never leak into/corrupt tags)          */
/* ------------------------------------------------------------------------------------------- */

#[test]
fn battery_d_flags_coexist_with_multichar_tags() {
    // `<R:0001>`-shaped multichar symbols are pg-foma's own tag alphabet (see src/tags.rs). This
    // network puts a real tag symbol immediately after a flag-gated stem to make sure elimination
    // never touches it.
    let src = r#"[["@U.NUM.sg@" c a t] | ["@U.NUM.pl@" c a t s]] "<R:0001>""#;
    let words = ["cat<R:0001>", "cats<R:0001>", "cat", "cats"]; // last two: missing the tag, must fail

    // Sanity: the tag shape is never mistaken for a flag by foma-rs's own flag_check DFA (it
    // requires the "@X.a@" shape; "<R:0001>" starts with '<', not '@').
    assert!(
        !flag_check("<R:0001>"),
        "a pg-foma tag symbol must never be classified as a flag diacritic"
    );

    let baseline = rs_baseline_up(src, &words);
    print_table("battery_d rs baseline", &baseline);
    let legal_expected: BTreeSet<String> = ["cat<R:0001>", "cats<R:0001>"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let baseline_legal: BTreeSet<String> = baseline
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, _)| k.clone())
        .collect();
    assert_eq!(
        baseline_legal, legal_expected,
        "battery_d: baseline sanity (tag required)"
    );

    // foma-rs internal: eliminate NUM, then check (1) apply_up sets still agree, (2) the tag
    // symbol is still present in sigma post-elimination, (3) no NUM flag symbols remain.
    let opts = FomaOptions::default();
    let net = fsm_parse_regex(&opts, src, None, None).expect("compiles");
    let eliminated_net = flag_eliminate(&opts, net, Some("NUM"));
    let sigma_syms: BTreeSet<String> = eliminated_net
        .sigma
        .iter()
        .map(|s| s.symbol.to_string())
        .collect();
    assert!(
        sigma_syms.contains("<R:0001>"),
        "the tag symbol must survive NUM elimination intact: sigma = {sigma_syms:?}"
    );
    assert!(
        !sigma_syms.iter().any(|s| flag_check(s)),
        "no flag symbols of any kind should remain after eliminating the only attribute present: {sigma_syms:?}"
    );

    let eliminated = apply_up_all(&eliminated_net, &words);
    print_table("battery_d rs eliminated(NUM)", &eliminated);
    assert_eq!(
        baseline, eliminated,
        "battery_d: baseline vs eliminated(NUM) must agree as sets"
    );

    let Some(c_baseline) = c_foma_leg("battery_d_baseline", src, &[], &words) else {
        return;
    };
    print_table("battery_d C-foma baseline", &c_baseline);
    assert_eq!(
        baseline, c_baseline,
        "battery_d: foma-rs vs C-foma baseline must agree"
    );

    let Some(c_eliminated) = c_foma_leg("battery_d_elim_num", src, &["NUM"], &words) else {
        return;
    };
    print_table("battery_d C-foma eliminated(NUM)", &c_eliminated);
    assert_eq!(
        eliminated, c_eliminated,
        "battery_d: foma-rs vs C-foma eliminated(NUM) must agree"
    );
}

/* ------------------------------------------------------------------------------------------- */
/* (e) Reduplication-shaped stem + flag + affix: the closest reproducible analog of the         */
/*     issue-#60 crash risk (flag diacritics interacting badly with reduplication+affixation)    */
/* ------------------------------------------------------------------------------------------- */

#[test]
fn battery_e_reduplication_shaped_flags_and_affix_issue60_risk() {
    // github.com/mhulden/foma issue #60: eliminating flag diacritics while reduplicating a stem
    // and adding a prefix/suffix crashes real C foma ("double free corruption" / "error core
    // dumped"). `_eq` in that report turned out (confirmed via the issue text) to be the
    // REPORTER's OWN xfst function name, not a foma builtin -- there is no `_eq(...)` regex
    // construct in foma-rs to substitute for.
    //
    // True generative reduplication (copying an unbounded stem) is not expressible as a finite
    // regular-language regex in either compiler's parser, and is explicitly out of pg-foma's FST
    // scope (design doc: "Reduplication: unchanged (proposer-agnostic peel)"). The closest
    // reproducible construct: finite PRE-COPIED stems ("catcat", "dogdog") standing in for the
    // OUTPUT SHAPE of a reduplicated root, combined with a flag-gated suffix (affixation) and a
    // multichar tag -- reduplication-shaped, flagged, and affixed, all three risk ingredients from
    // the issue, without requiring a copy operator neither compiler's regex language has.
    let src = r#"[["@P.NUM.sg@" c a t c a t] | ["@P.NUM.pl@" d o g d o g]] [["@U.NUM.pl@" s] | ["@U.NUM.sg@"]] "<R:0002>""#;
    let words = [
        "catcat<R:0002>",
        "dogdogs<R:0002>",
        "catcats<R:0002>",
        "dogdog<R:0002>",
    ];
    let legal_expected: BTreeSet<String> = ["catcat<R:0002>", "dogdogs<R:0002>"]
        .iter()
        .map(|s| s.to_string())
        .collect();

    let baseline = rs_baseline_up(src, &words);
    print_table("battery_e rs baseline", &baseline);
    let baseline_legal: BTreeSet<String> = baseline
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, _)| k.clone())
        .collect();
    assert_eq!(baseline_legal, legal_expected, "battery_e: baseline sanity");

    let eliminated = rs_eliminated_up(src, &["NUM"], &words);
    print_table("battery_e rs eliminated(NUM)", &eliminated);
    assert_eq!(
        baseline, eliminated,
        "battery_e: foma-rs handles this reduplication-shaped+flag+affix combo WITHOUT crashing \
         or losing equivalence (the Rust port leaks memory instead of double-freeing per \
         flags.rs's own doc comments, so a C-style crash is not expected here, but correctness is \
         still asserted)"
    );

    if !wsl_available() {
        eprintln!("SKIP battery_e C-foma legs: wsl foma/flookup not available on this machine");
        return;
    }

    match run_c_foma_script("battery_e_baseline", src, &[]) {
        CFomaOutcome::Ok { fst_path } => {
            let c_baseline = flookup_up(&fst_path, &words);
            print_table("battery_e C-foma baseline", &c_baseline);
            assert_eq!(
                baseline, c_baseline,
                "battery_e: foma-rs vs C-foma baseline (no elimination) must agree"
            );
        }
        CFomaOutcome::Failed {
            status,
            stdout,
            stderr,
        } => {
            eprintln!(
                "FINDING battery_e_baseline: C-foma FAILED even without elimination \
                 (status={status}):\nstdout:\n{stdout}\nstderr:\n{stderr}"
            );
        }
    }

    match run_c_foma_script("battery_e_elim_num", src, &["NUM"]) {
        CFomaOutcome::Ok { fst_path } => {
            let c_eliminated = flookup_up(&fst_path, &words);
            print_table("battery_e C-foma eliminated(NUM)", &c_eliminated);
            assert_eq!(
                eliminated, c_eliminated,
                "battery_e: foma-rs vs C-foma eliminated(NUM) must agree -- a mismatch or crash \
                 here is the issue-#60 risk shape materializing"
            );
        }
        CFomaOutcome::Failed {
            status,
            stdout,
            stderr,
        } => {
            eprintln!(
                "FINDING battery_e_elim_num: C-foma CRASHED/FAILED eliminating flag NUM on a \
                 reduplication-shaped+affixed network (status={status}) -- this IS the issue-#60 \
                 risk shape materializing, a successful gate finding, not a harness bug:\n\
                 stdout:\n{stdout}\nstderr:\n{stderr}\n\
                 foma-rs's own result for comparison (it did not crash): {eliminated:?}"
            );
        }
    }
}

/* ------------------------------------------------------------------------------------------- */
/* @E@ (FLAG_EQUAL) divergence: NOT issue #60, a separately-discovered non-equivalence           */
/* ------------------------------------------------------------------------------------------- */

#[test]
fn e_flag_type_elimination_not_equivalence_preserving() {
    // Discovered during investigation (NOT the issue-#60 shape -- a distinct divergence in the
    // SAME risk area). foma-0.1.1/src/flags.rs::flag_build's 25-row decision table (a literal
    // bug-for-bug port of the real C table -- see that file's row comments) has NO row whose
    // eliminated-flag type is FLAG_EQUAL ("E"). So when `flag_eliminate` is asked to eliminate an
    // E-typed attribute, the per-instance `flag_build` comparison against every other flag
    // instance always returns NONE, `flag` never becomes nonzero, and NO FILTER is ever built for
    // that instance -- yet `flag_purge` (which purges by NAME match unconditionally, not gated on
    // whether a filter fired) still strips the "@E.F.1@" symbols from the network regardless.
    // Net effect: "eliminating" an E-attribute silently degrades to STRIP (spec §1 position 3:
    // illegal paths become reachable) while still being invoked as if it were the exact Eliminate
    // arm (position 1). This was verified empirically against real C foma too (see eqtest3/4
    // during investigation): baseline (flags obeyed) already fails BOTH "a" and "b" for this
    // network -- i.e. real C foma's runtime apply doesn't even honor `@E@` as a passing condition
    // once the attribute has been SET by a prior @P@, an orthogonal quirk in the *runtime* apply
    // path, separate from the *elimination* non-equivalence asserted below.
    let src = r#"["@P.F.1@" a "@E.F.1@"] | ["@P.F.2@" b "@E.F.1@"]"#;
    let words = ["a", "b"];

    let baseline = rs_baseline_up(src, &words);
    print_table("e_flag rs baseline (flags obeyed)", &baseline);
    let eliminated = rs_eliminated_up(src, &["F"], &words);
    print_table("e_flag rs eliminated(F, an E-typed attribute)", &eliminated);

    // This IS the per-attribute gate that spec §5's oracle criterion alone would miss: §5 only
    // asks for foma-rs/C-foma agreement (an E-tester passes that -- see below, both engines land
    // on {a,b}), but spec §1's actual safety property is `Eliminate == KeepFlag` (baseline, flags
    // obeyed) WITHIN one engine. That fails here: eliminated {a,b} != baseline {}. Note the
    // direction is NOT resolved by this investigation -- {} could be the runtime under-generating
    // (a recall bug, the dangerous direction) rather than {a,b} over-generating (the safe,
    // HC-confirm-prunable direction), since `@E.F.1@`'s true intended semantics were never pinned
    // down here. So this test asserts ONLY the non-equivalence, not that KeepFlag/Strip is "the"
    // safe arm -- do not assign Eliminate to an E-typed constraint (if pg-foma's emitter ever
    // produces one; it cannot be confirmed from this crate alone that it does), and treat E's
    // correct runtime semantics as an open question rather than assuming any arm is safe for it.
    assert_ne!(
        baseline, eliminated,
        "FINDING (headline of this file): eliminating an @E@-typed attribute in foma-rs is NOT \
         equivalence-preserving (flag_build has no rows for FLAG_EQUAL, so no filter is built, but \
         flag_purge strips the symbol anyway -- this degrades silently to Strip, i.e. spec §1 \
         position 1 silently behaves like position 3). The spec §5 oracle check (foma-rs vs \
         C-foma agreement) does NOT catch this by itself -- see the cross-oracle check below, \
         where both engines agree on the (wrong) eliminated result. The real per-attribute gate \
         must ALSO require eliminated == baseline within one engine, which is exactly this assertion."
    );

    if !wsl_available() {
        eprintln!("SKIP e_flag C-foma legs: wsl foma/flookup not available on this machine");
        return;
    }
    let Some(c_baseline) = c_foma_leg("e_flag_baseline", src, &[], &words) else {
        return;
    };
    print_table("e_flag C-foma baseline", &c_baseline);
    // Informational only (not asserted as a hard gate condition): does C-foma's *baseline*
    // runtime-apply behavior on this E-flagged network match foma-rs's baseline? If not, the
    // divergence starts even before elimination enters the picture.
    if baseline != c_baseline {
        eprintln!(
            "FINDING: foma-rs and C-foma already disagree on the UN-eliminated E-flagged \
             baseline (before elimination is even involved): foma-rs={baseline:?} \
             c-foma={c_baseline:?}"
        );
    }

    let Some(c_eliminated) = c_foma_leg("e_flag_elim_f", src, &["F"], &words) else {
        return;
    };
    print_table("e_flag C-foma eliminated(F)", &c_eliminated);
    if eliminated == c_eliminated {
        eprintln!(
            "FINDING: foma-rs and C-foma AGREE on eliminated(F) ({eliminated:?}) despite both \
             diverging from their own un-eliminated baseline -- consistent with a bug-for-bug \
             port: both engines share the same E-flag elimination non-equivalence, not just \
             foma-rs alone."
        );
    } else {
        eprintln!(
            "FINDING: foma-rs and C-foma DISAGREE on eliminated(F): foma-rs={eliminated:?} \
             c-foma={c_eliminated:?} -- a genuine cross-oracle mismatch on top of the \
             non-equivalence already found."
        );
    }
}

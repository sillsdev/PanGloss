//! The C-foma oracle gate for `eliminate flag` fidelity: per-attribute elimination equivalence-tested against the real C foma oracle before any tuner enables an `Eliminate` arm; on any mismatch the design must degrade to `AllFlags`, never to wrong.
//! Method, battery coverage, and the headline E-flag finding: docs/research/pk2-eliminate-flag-oracle-findings.md.

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

// foma-rs side.

/// Compile `source`, `apply_up` every word in `words`, with no elimination (this crate's "baseline"/"flagged" leg).
fn rs_baseline_up(source: &str, words: &[&str]) -> BTreeMap<String, BTreeSet<String>> {
    let opts = FomaOptions::default();
    let net = fsm_parse_regex(&opts, source, None, None)
        .unwrap_or_else(|| panic!("foma-rs failed to compile baseline source: {source}"));
    apply_up_all(&net, words)
}

/// Compile `source`, eliminate each attribute in `attrs` in order, chained so each call sees the previous output network, matching C-foma's REPL semantics.
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

// C-foma (WSL) side.

/// Whether `wsl foma` and `wsl flookup` are both callable on this machine. Cached: spawning WSL is slow and every C-foma leg checks this first.
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

/// Scratch directory for this test binary's generated `.foma` scripts / `.fst` binaries, under cargo's stable per-test-target tmp dir.
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

/// Outcome of driving a C-foma script through `wsl foma -q -f`; `Failed` is itself a gate finding callers must report, never a Rust panic.
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

/// Write the compile+eliminate+save script to `TAG.foma`, run it under `wsl foma -q -f`, and report whether `TAG.fst` was produced.
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

/// Batch-apply `words` against a saved binary net via `wsl flookup`; its default direction (no `-i`) is apply-up, matching `ApplyHandle::up` (empirically verified).
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

/// Full C-foma leg: compile, save, flookup; returns `None` when WSL is unavailable or when foma itself failed/crashed (a reportable finding, logged by the caller).
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
                 result (issue #60 crash risk), not a harness bug:\nstdout:\n{stdout}\nstderr:\n{stderr}"
            );
            None
        }
    }
}

// Shared assertion helper.

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

// Shared network sources.

/// The exact network from `foma-0.1.1/src/flags.rs`'s own `flag_eliminate_end_to_end` unit test, covering U/R/D flags across three independently-named attributes; legal set verified against real C foma = {a, c, e}.
const FLAGTEST_SRC: &str = r#"["@P.F.1@" a "@U.F.1@"] | ["@P.F.2@" b "@U.F.1@"] | ["@P.G.1@" c "@R.G@"] | [d "@R.G@"] | [e "@D.H@"] | ["@P.H.1@" f "@D.H@"]"#;
const FLAGTEST_WORDS: [&str; 6] = ["a", "b", "c", "d", "e", "f"];

// Load-bearing assumption: foma-rs obeys flags by default.

#[test]
fn rs_flags_obeyed_by_default_baseline() {
    // Every "baseline" leg here depends on ApplyHandle obeying flag diacritics by default; verify directly rather than trusting the source read, since the live pipeline only ever strips flags before this point.
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

// (a) Unify agreement across a stem boundary (Beesley & Karttunen separated dependency).

#[test]
fn battery_a_unify_agreement_across_stem_boundary() {
    // Determiner/noun NUM agreement, separated by the flag mechanism rather than string adjacency, must agree via the same @U@ attribute (Beesley 1998 / B&K 2003 ch.7).
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

// (b) @P@+@R@ and @P@+@D@ combos (plus @P@+@U@).

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

    // REQUIRE *with a value* is a distinct set of rows in flag_build's decision table from the valueless `@R.G@` already covered by FLAGTEST_SRC above.
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

// (c) Three independent attributes, chained elimination (Karttunen-style), incl. a prefix-colliding attribute-name pair to stress flag_purge's name-boundary guard.

#[test]
fn battery_c_three_independent_attributes_chained_elimination() {
    // pos1/pos2: NUM agreement. pos3: NUMBER, a name that is a prefix of NUM, stressing flag_purge's name-boundary check. pos4: CASE, independent of both.
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

// (d) Flags coexisting with multichar tag symbols (must never leak into/corrupt tags).

#[test]
fn battery_d_flags_coexist_with_multichar_tags() {
    // Puts a real `<R:0001>`-shaped tag symbol immediately after a flag-gated stem to make sure elimination never touches it.
    let src = r#"[["@U.NUM.sg@" c a t] | ["@U.NUM.pl@" c a t s]] "<R:0001>""#;
    let words = ["cat<R:0001>", "cats<R:0001>", "cat", "cats"]; // last two: missing the tag, must fail

    // Sanity: the tag shape is never mistaken for a flag (flag_check requires the "@X.a@" shape).
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

    // Checks apply_up sets still agree, the tag symbol survives in sigma, and no NUM flag symbols remain.
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

// (e) Reduplication-shaped stem + flag + affix: the closest reproducible analog of the issue-#60 crash risk.

#[test]
fn battery_e_reduplication_shaped_flags_and_affix_issue60_risk() {
    // True generative reduplication is not expressible as a finite regex in either compiler's parser, so this uses finite pre-copied stems ("catcat", "dogdog") standing in for the output shape, combined with a flag-gated suffix and a multichar tag.
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
        "battery_e: foma-rs must handle this reduplication-shaped+flag+affix combo without losing \
         equivalence"
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

// `@E@` (FLAG_EQUAL) divergence: not issue #60, a separately-discovered non-equivalence; full argument in docs/research/pk2-eliminate-flag-oracle-findings.md.

#[test]
fn e_flag_type_elimination_not_equivalence_preserving() {
    // flag_build's decision table has no row for FLAG_EQUAL, so eliminating an E-typed attribute never builds a filter, yet flag_purge still strips the symbol, degrading silently to Strip.
    let src = r#"["@P.F.1@" a "@E.F.1@"] | ["@P.F.2@" b "@E.F.1@"]"#;
    let words = ["a", "b"];

    let baseline = rs_baseline_up(src, &words);
    print_table("e_flag rs baseline (flags obeyed)", &baseline);
    let eliminated = rs_eliminated_up(src, &["F"], &words);
    print_table("e_flag rs eliminated(F, an E-typed attribute)", &eliminated);

    // This assertion is only the non-equivalence; which side is "wrong" is an open question (see the linked doc), so no arm is asserted safe for E.
    assert_ne!(
        baseline, eliminated,
        "FINDING (headline of this file): eliminating an @E@-typed attribute in foma-rs is NOT \
         equivalence-preserving; a foma-rs/C-foma agreement check alone does not catch this -- see \
         docs/research/pk2-eliminate-flag-oracle-findings.md"
    );

    if !wsl_available() {
        eprintln!("SKIP e_flag C-foma legs: wsl foma/flookup not available on this machine");
        return;
    }
    let Some(c_baseline) = c_foma_leg("e_flag_baseline", src, &[], &words) else {
        return;
    };
    print_table("e_flag C-foma baseline", &c_baseline);
    // Informational only: if C-foma's own baseline already disagrees, the divergence starts before elimination enters the picture.
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

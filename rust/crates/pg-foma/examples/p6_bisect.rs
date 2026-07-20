use foma::apply::apply_init;
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;
use pg_foma::replace::{compile_rewrite_rule, SegAlphabet};
use pg_grammar::model::PhonRuleDef;
use std::path::{Path, PathBuf};

fn sample_path(name: &str) -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../../../samples/data").join(name)
}

fn try_it(opts: &FomaOptions, label: &str, s: &str) {
    let ok = fsm_parse_regex(opts, s, None, None).is_some();
    println!("{label}: {} :: {:?}", if ok { "OK" } else { "FAIL" }, s);
}

fn main() {
    let opts = FomaOptions::default();
    let a = '\u{e000}';
    let b = '\u{e007}';
    let c = '\u{e01c}';
    let d = '\u{e00b}';
    let e = '\u{e01d}';
    let f = '\u{e002}';
    let g = '\u{e00a}';

    try_it(&opts, "1 literal", &format!("{a}"));
    try_it(&opts, "2 union", &format!("[{a}|{b}]"));
    try_it(&opts, "3 concat multi", &format!("{e}{f}"));
    try_it(&opts, "4 basic replace", &format!("{c} -> {d} || {a} _ {e}{f}"));
    try_it(&opts, "5 replace w/ union left", &format!("{c} -> {d} || [{a}|{b}] _ {e}{f}"));
    try_it(&opts, "6 replace w/ union left no space", &format!("{c}->{d}||[{a}|{b}]_{e}{f}"));
    try_it(
        &opts,
        "7 two comma branches",
        &format!("{c} -> {d} || [{a}|{b}] _ {e}{f}, {c} -> {d} || [{a}|{b}] _ {e}{g}"),
    );
    try_it(&opts, "8 deletion 0 rhs", &format!("{c} -> 0 || {a} _ {e}"));
    try_it(&opts, "9 ascii diff lhs, both context", "a -> b || c _ d, e -> f || g _ h");
    try_it(&opts, "10 ascii same lhs, both context", "a -> b || c _ d, a -> b || c _ e");
    try_it(&opts, "11 ascii same lhs no context", "a -> b, a -> b");
    try_it(&opts, "12 ascii env-comma same rule", "a -> b || c _ d, c _ e");
    try_it(&opts, "13 pua env-comma same rule", &format!("{c} -> {d} || {a} _ {e}{f}, {a} _ {e}{g}"));
    try_it(
        &opts,
        "14 pua env-comma with union",
        &format!("{c} -> {d} || [{a}|{b}] _ {e}{f}, [{a}|{b}] _ {e}{g}"),
    );
    // Hypothesis: comma separates "RHS || env" clauses sharing ONE lhs, each clause with its OWN
    // rhs (no repeated "lhs ->").
    try_it(&opts, "15 shared-lhs diff-rhs-per-env", "a -> b || c _ d, e || f _ g");
    try_it(
        &opts,
        "16 pua shared-lhs diff-rhs-per-env",
        &format!("{c} -> {d} || {a} _ {e}{f}, {g} || {a} _ {e}{b}"),
    );
    // Hypothesis: parallel rule SET syntax uses a different separator for concurrently-applied
    // distinct-context rules, e.g. sequential composition via .o. instead of comma; verify union
    // of two SEPARATE FULL replace-rule nets at the FSM level round-trips correctly for a case
    // with disjoint contexts (this is the actual mechanism the real driver uses; bisect confirms
    // whether the "spurious identity leak" is inherent to fsm_union of complete transducers).

    // ---- isolate prule5 alone (voiceless obstruent deletion) ----
    let xml = std::fs::read_to_string(sample_path("indonesian-hc.xml")).unwrap();
    let g = pg_grammar::load(&xml).unwrap();
    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let prule5 = g
        .strata
        .iter()
        .flat_map(|s| &s.prules)
        .map(|&id| &g.prules[id.0 as usize])
        .find(|pr| matches!(pr, PhonRuleDef::Rewrite(r) if r.xml_id == "prule5"))
        .expect("prule5 present");
    let PhonRuleDef::Rewrite(r5) = prule5 else { unreachable!() };
    let (net5, reports5) = compile_rewrite_rule(&opts, &g, &alphabet, r5).expect("compose budget ok").expect("prule5 compiles");
    println!("\nprule5 alone: reports={reports5:?}");
    let mut h5 = apply_init(&net5);
    let e = table.lookup_nfd("e").unwrap();
    let n = table.lookup_nfd("n").unwrap();
    let bound = table.lookup_nfd("+").unwrap();
    let mut u = String::new();
    u.push(alphabet.token(e));
    u.push(alphabet.token(n));
    u.push(alphabet.token(bound));
    u.push_str(&alphabet.encode_query("tulis").unwrap());
    let results: Vec<String> = h5.down(&u).collect();
    println!("prule5 alone apply_down(e+n+%2B+tulis): {} result(s)", results.len());
    for r in &results {
        let hex: Vec<String> = r.chars().map(|c| format!("{:04x}", c as u32)).collect();
        println!("    [{}]", hex.join(" "));
    }

    // ---- prule5 ALONE but with a leading 'm' (matching net4's actual output length/shape) ----
    {
        let (net5_solo2, _) = compile_rewrite_rule(&opts, &g, &alphabet, r5).expect("compose budget ok").expect("prule5 compiles");
        let mut h5b = apply_init(&net5_solo2);
        let m0 = table.lookup_nfd("m").unwrap();
        let mut u5b = String::new();
        u5b.push(alphabet.token(m0));
        u5b.push(alphabet.token(e));
        u5b.push(alphabet.token(n));
        u5b.push(alphabet.token(bound));
        u5b.push_str(&alphabet.encode_query("tulis").unwrap());
        let r5b: Vec<String> = h5b.down(&u5b).collect();
        println!("prule5 ALONE apply_down(m+e+n+%2B+tulis): {} result(s)", r5b.len());
        for r in &r5b {
            let hex: Vec<String> = r.chars().map(|c| format!("{:04x}", c as u32)).collect();
            println!("    [{}]", hex.join(" "));
        }
    }

    // ---- prule4 .o. prule5 composed, on the RAW placeholder input ----
    let prule4 = g
        .strata
        .iter()
        .flat_map(|s| &s.prules)
        .map(|&id| &g.prules[id.0 as usize])
        .find(|pr| matches!(pr, PhonRuleDef::Rewrite(r) if r.xml_id == "prule4"))
        .expect("prule4 present");
    let PhonRuleDef::Rewrite(r4) = prule4 else { unreachable!() };
    let (net4, _) = compile_rewrite_rule(&opts, &g, &alphabet, r4).expect("compose budget ok").expect("prule4 compiles");
    let (net5b, _) = compile_rewrite_rule(&opts, &g, &alphabet, r5).expect("compose budget ok").expect("prule5 compiles");
    let m = table.lookup_nfd("m").unwrap();
    let placeholder = table.lookup_nfd("\u{207f}").unwrap();
    // prule4 ALONE on the exact same input, to compare byte-for-byte against the composed result.
    {
        let (net4_solo, _) = compile_rewrite_rule(&opts, &g, &alphabet, r4).expect("compose budget ok").expect("prule4 compiles");
        let mut h4 = apply_init(&net4_solo);
        let mut u4 = String::new();
        u4.push(alphabet.token(m));
        u4.push(alphabet.token(e));
        u4.push(alphabet.token(placeholder));
        u4.push(alphabet.token(bound));
        u4.push_str(&alphabet.encode_query("tulis").unwrap());
        let r4results: Vec<String> = h4.down(&u4).collect();
        println!("prule4 ALONE apply_down(me<ph>+tulis): {} result(s)", r4results.len());
        for r in &r4results {
            let hex: Vec<String> = r.chars().map(|c| format!("{:04x}", c as u32)).collect();
            println!("    [{}]", hex.join(" "));
        }
    }
    let composed45 = foma::constructions::fsm_compose(&opts, net4, net5b);
    let mut h45 = apply_init(&composed45);
    let mut u2 = String::new();
    u2.push(alphabet.token(m));
    u2.push(alphabet.token(e));
    u2.push(alphabet.token(placeholder));
    u2.push(alphabet.token(bound));
    u2.push_str(&alphabet.encode_query("tulis").unwrap());
    let results2: Vec<String> = h45.down(&u2).collect();
    println!("prule4.o.prule5 apply_down(me<ph>+tulis): {} result(s)", results2.len());
    for r in &results2 {
        let hex: Vec<String> = r.chars().map(|c| format!("{:04x}", c as u32)).collect();
        println!("    [{}]", hex.join(" "));
    }

    // Try REVERSED composition order too (sanity-check the tape-direction assumption).
    let (net4c, _) = compile_rewrite_rule(&opts, &g, &alphabet, r4).expect("compose budget ok").expect("prule4 compiles");
    let (net5c, _) = compile_rewrite_rule(&opts, &g, &alphabet, r5).expect("compose budget ok").expect("prule5 compiles");
    let composed54 = foma::constructions::fsm_compose(&opts, net5c, net4c);
    let mut h54 = apply_init(&composed54);
    let results3: Vec<String> = h54.down(&u2).collect();
    println!(
        "prule5.o.prule4 (REVERSED order) apply_down(me<ph>+tulis): {} result(s)",
        results3.len()
    );
    for r in &results3 {
        let hex: Vec<String> = r.chars().map(|c| format!("{:04x}", c as u32)).collect();
        println!("    [{}]", hex.join(" "));
    }

    // ---- hypothesis: fsm_compose (Rust-level, separately-parsed nets) mishandles internal `.#.`
    // bookkeeping across context-restricted replace rules; a SINGLE regex string using foma's
    // OWN `.o.` infix operator (one fsm_parse_regex call) should behave differently if so.
    let placeholder2 = table.lookup_nfd("\u{207f}").unwrap();
    let t_id = table.lookup_nfd("t").unwrap();
    let e_id = e; // char1
    let n_id = n; // char12
    let bound_id = bound; // char30
    let u_vowel = table.lookup_nfd("u").unwrap();
    let single_regex = format!(
        "{ph} -> {nn} || {ee} _ {bb} {tt} .o. {tt} -> 0 || {ee} {nn} {bb} _ {uu}",
        ph = alphabet.token(placeholder2),
        nn = alphabet.token(n_id),
        ee = alphabet.token(e_id),
        bb = alphabet.token(bound_id),
        tt = alphabet.token(t_id),
        uu = alphabet.token(u_vowel),
    );
    println!("\nsingle-string .o. test regex: {single_regex:?}");
    match fsm_parse_regex(&opts, &single_regex, None, None) {
        Some(net) => {
            let mut h = apply_init(&net);
            let mut u3 = String::new();
            u3.push(alphabet.token(m));
            u3.push(alphabet.token(e_id));
            u3.push(alphabet.token(placeholder2));
            u3.push(alphabet.token(bound_id));
            u3.push_str(&alphabet.encode_query("tulis").unwrap());
            let r3: Vec<String> = h.down(&u3).collect();
            println!("single-string .o. apply_down(me<ph>+tulis): {} result(s)", r3.len());
            for r in &r3 {
                let hex: Vec<String> = r.chars().map(|c| format!("{:04x}", c as u32)).collect();
                println!("    [{}]", hex.join(" "));
            }
        }
        None => println!("single-string .o. regex FAILED to compile"),
    }

    // ---- the SECOND rule ALONE, fully isolated, simplified literal form ----
    let second_alone = format!(
        "{tt} -> 0 || {ee} {nn} {bb} _ {uu}",
        ee = alphabet.token(e_id),
        nn = alphabet.token(n_id),
        bb = alphabet.token(bound_id),
        tt = alphabet.token(t_id),
        uu = alphabet.token(u_vowel),
    );
    println!("\nsecond-alone regex: {second_alone:?}");
    let net_2nd = fsm_parse_regex(&opts, &second_alone, None, None).expect("compiles");
    let mut h2nd = apply_init(&net_2nd);
    let mut u4b = String::new();
    u4b.push(alphabet.token(m));
    u4b.push(alphabet.token(e_id));
    u4b.push(alphabet.token(n_id));
    u4b.push(alphabet.token(bound_id));
    u4b.push_str(&alphabet.encode_query("tulis").unwrap());
    let r2nd: Vec<String> = h2nd.down(&u4b).collect();
    println!("second-alone apply_down(m e n + tulis): {} result(s)", r2nd.len());
    for r in &r2nd {
        let hex: Vec<String> = r.chars().map(|c| format!("{:04x}", c as u32)).collect();
        println!("    [{}]", hex.join(" "));
    }
}

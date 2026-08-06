//! Property test: `CharDefTable::unifiable_cds`'s build-time closure memo must agree exactly with a gate-free lane scan (`flat_unifiable`, computed fresh) on random `(edge cd, query cd)` pairs. Self-skips when the untracked sample grammar is absent, and is unconditionally `#[ignore]`d so the default test run stays fast.

use std::path::{Path, PathBuf};

use pg_featstruct::flat_unifiable;
use pg_grammar::chardef::{CharDefId, CharDefKind};

fn sample_path(name: &str) -> Option<PathBuf> {
    // CARGO_MANIFEST_DIR = .../rust/crates/pg-grammar ; samples live at repo_root/samples/data.
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
}

/// Tiny deterministic xorshift64 PRNG, no external `rand` dependency, fixed seed so a failure is always reproducible.
fn xorshift(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/amharic-hc.xml); run with --include-ignored"]
fn amharic_closure_matches_gate_free_lane_scan() {
    let Some(path) = sample_path("amharic-hc.xml") else {
        eprintln!("amharic-hc.xml not present locally; skipping (untracked sample corpus)");
        return;
    };
    let xml = std::fs::read_to_string(&path).expect("read amharic-hc.xml");
    let g = pg_grammar::load(&xml).expect("amharic grammar loads");
    assert!(
        !g.phon_features.is_empty(),
        "sanity: Amharic authors phonological features"
    );

    let table = &g.char_tables[0];
    let n = table.len() as u32;
    assert!(n > 0, "sanity: Amharic table is non-empty");

    let mut state: u64 = 0x9E3779B97F4A7C15; // fixed seed: deterministic, reproducible on failure
    let trials = 20_000;
    let mut checked_pairs = 0usize;
    for _ in 0..trials {
        let i = (xorshift(&mut state) % n as u64) as u32;
        let j = (xorshift(&mut state) % n as u64) as u32;
        let cd_i = CharDefId(i);
        let cd_j = CharDefId(j);
        // The closure only covers Segment×Segment pairs; boundary rows are deliberately out of scope, so restrict the property to segment pairs.
        if table.get(cd_i).kind() != CharDefKind::Segment
            || table.get(cd_j).kind() != CharDefKind::Segment
        {
            continue;
        }
        checked_pairs += 1;

        // Design A: build-time closure membership (`CharDefTable::unifiable_cds`).
        let design_a = table
            .unifiable_cds(cd_i)
            .is_some_and(|closure| closure.contains(j));

        // Design C: gate-free lane scan, computed fresh right here — no closure consulted at all.
        let design_c = flat_unifiable(
            table.get(cd_i).feature_lanes(),
            table.get(cd_j).feature_lanes(),
        );

        assert_eq!(
            design_a,
            design_c,
            "Design A/C disagree for cd {i} ({:?}) vs cd {j} ({:?})",
            table.get(cd_i).xml_id(),
            table.get(cd_j).xml_id(),
        );
    }
    assert!(
        checked_pairs > 1000,
        "sanity: expected many segment-pair trials out of {trials}, got {checked_pairs}"
    );
}

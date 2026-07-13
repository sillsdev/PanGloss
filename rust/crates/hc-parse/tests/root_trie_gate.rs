//! Structural gate for the root-allomorph trie (M5a): build the per-stratum tries over the real
//! Sena (1371 lex entries) and Indonesian (66) grammars, and confirm that searching a real root's
//! own surface returns that root's allomorph id.
//!
//! The sample grammars are untracked local corpus files (per `rust-conversion.md` §8); each test
//! self-skips when they are absent (fresh clone / CI).

use std::path::{Path, PathBuf};

use hc_grammar::load;
use hc_grammar::model::{AllomorphId, Grammar, LexEntryId, StratumId};
use hc_parse::RootAllomorphIndex;
use hc_shape::{NodeFlags, NodeKind, Shape, ShapeBuilder};

/// The `char_def` id sequence of a shape's `Segment` nodes (the trie's discriminator).
fn seg_char_defs(shape: &Shape) -> Vec<u32> {
    (0..shape.len())
        .filter(|&i| shape.kind(i) == NodeKind::Segment)
        .map(|i| shape.char_def(i))
        .collect()
}

/// Find a returned allomorph's shape by `(entry, allomorph id)`.
fn allomorph_shape(g: &Grammar, e: LexEntryId, a: AllomorphId) -> &Shape {
    &g.entries[e.0 as usize]
        .allomorphs
        .iter()
        .find(|x| x.id == a)
        .expect("returned allomorph belongs to its entry")
        .shape
        .shape
}

fn sample_path(name: &str) -> Option<PathBuf> {
    // CARGO_MANIFEST_DIR = .../rust/crates/hc-parse ; samples live at repo_root/samples/data.
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
}

fn load_grammar(name: &str) -> Option<Grammar> {
    let path = sample_path(name)?;
    let xml = std::fs::read_to_string(path).expect("read sample grammar");
    Some(load(&xml).unwrap_or_else(|e| panic!("failed to load {name}: {e}")))
}

/// Total root allomorphs a stratum owns (the count the trie must index).
fn expected_stratum_allomorphs(g: &Grammar, s: StratumId) -> usize {
    g.strata[s.0 as usize]
        .entries
        .iter()
        .map(|le| g.entries[le.0 as usize].allomorphs.len())
        .sum()
}

/// Build the index, assert each stratum's trie indexes exactly its root allomorphs, and report.
fn assert_indexing(g: &Grammar, tag: &str) -> RootAllomorphIndex {
    let index = RootAllomorphIndex::build(g);
    assert_eq!(
        index.stratum_count(),
        g.strata.len(),
        "{tag}: one trie per stratum"
    );
    let mut total = 0usize;
    for s in 0..g.strata.len() {
        let sid = StratumId(s as u8);
        let expected = expected_stratum_allomorphs(g, sid);
        let got = index.trie(sid).allomorph_count();
        assert_eq!(got, expected, "{tag}: stratum {s} allomorph count");
        total += got;
        eprintln!(
            "{tag}: stratum {s} ({:?}) indexed {got} root allomorphs into {} trie nodes",
            g.strata[s].name,
            index.trie(sid).node_count()
        );
    }
    eprintln!(
        "{tag}: {} entries, {total} root allomorphs indexed across {} strata",
        g.entries.len(),
        g.strata.len()
    );
    index
}

/// For up to `limit` real root allomorphs, search the allomorph's own (already-segmented) shape and
/// assert its id is in the returned set. Returns how many were checked.
fn assert_real_root_lookups(
    g: &Grammar,
    index: &RootAllomorphIndex,
    tag: &str,
    limit: usize,
) -> usize {
    let mut checked = 0usize;
    for s in 0..g.strata.len() {
        let sid = StratumId(s as u8);
        for &le_id in &g.strata[s].entries {
            if checked >= limit {
                break;
            }
            let entry = &g.entries[le_id.0 as usize];
            let Some(allo) = entry.allomorphs.first() else {
                continue;
            };
            // The allomorph's own shape has at least one segment? (all-boundary shapes are dropped
            // at load, but guard anyway.)
            let has_seg = (0..allo.shape.shape.len())
                .any(|i| allo.shape.shape.kind(i) == hc_shape::NodeKind::Segment);
            if !has_seg {
                continue;
            }
            let query_segs = seg_char_defs(&allo.shape.shape);
            let got = index.search(g, sid, &allo.shape.shape);
            assert!(
                got.iter().any(|&(a, _)| a == allo.id),
                "{tag}: searching root {:?} surface {:?} did not return its own allomorph id {:?}; got {got:?}",
                le_id,
                allo.shape.text,
                allo.id,
            );
            // The decisive char_def check (advisor): every returned allomorph must have the
            // *identical* segment-char_def sequence as the query. Trivially true here; under a
            // lane-only trie this is massively violated for Sena (which has zero phonological
            // features, so lanes cannot discriminate) — same-length, different-segment roots would
            // all be returned. This is the real-data evidence that char_def keying is necessary.
            for &(a, e) in &got {
                assert!(
                    (e.0 as usize) < g.entries.len(),
                    "{tag}: returned entry id out of range"
                );
                assert_eq!(
                    seg_char_defs(allomorph_shape(g, e, a)),
                    query_segs,
                    "{tag}: returned allomorph {a:?} has a different segment sequence than the query \
                     {query_segs:?} — the trie is not discriminating by char_def",
                );
            }
            checked += 1;
        }
    }
    eprintln!("{tag}: verified {checked} real root-surface lookups");
    checked
}

#[test]
fn sena_root_trie_indexes_and_looks_up() {
    let Some(g) = load_grammar("sena-hc.xml") else {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    };
    // Sanity: Sena is the zero-*authored*-phonological-feature grammar (discrimination is by
    // char_def). Plan §13.1 Tier-1 #1: `len()` itself is never 0 post-fix (the always-appended
    // synthetic `Type` feature) — `is_empty()` is the correct "zero authored features" check now.
    assert!(
        g.phon_features.is_empty(),
        "Sena has zero authored phonological features"
    );
    assert_eq!(g.entries.len(), 1371, "Sena lex entry count");
    let index = assert_indexing(&g, "sena");
    let checked = assert_real_root_lookups(&g, &index, "sena", 50);
    assert!(
        checked >= 10,
        "sena: expected to verify many real roots, checked {checked}"
    );
}

#[test]
fn indonesian_root_trie_indexes_and_looks_up() {
    let Some(g) = load_grammar("indonesian-hc.xml") else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    assert_eq!(g.entries.len(), 66, "Indonesian lex entry count");
    let index = assert_indexing(&g, "indonesian");
    let checked = assert_real_root_lookups(&g, &index, "indonesian", 66);
    assert!(
        checked >= 10,
        "indonesian: expected to verify many real roots, checked {checked}"
    );
}

/// Proves end-anchoring on real data: take the first real Sena root, search it (must hit), then
/// search that root's shape with one extra segment appended — a strictly longer input cannot accept
/// at the shorter root's node, so the original allomorph id must now be *absent*.
#[test]
fn sena_negative_lookup_is_end_anchored() {
    let Some(g) = load_grammar("sena-hc.xml") else {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    };
    let index = RootAllomorphIndex::build(&g);
    // Find the first entry whose first allomorph has >= 1 segment.
    let mut found: Option<(StratumId, LexEntryId, AllomorphId)> = None;
    'outer: for s in 0..g.strata.len() {
        for &le in &g.strata[s].entries {
            if let Some(a) = g.entries[le.0 as usize].allomorphs.first() {
                if (0..a.shape.shape.len()).any(|i| a.shape.shape.kind(i) == NodeKind::Segment) {
                    found = Some((StratumId(s as u8), le, a.id));
                    break 'outer;
                }
            }
        }
    }
    let (sid, le, aid) = found.expect("sena has at least one segmental root");
    let shape = g.entries[le.0 as usize].allomorphs[0].shape.shape.clone();

    // Positive: the root's own surface returns its allomorph id.
    let hit = index.search(&g, sid, &shape);
    assert!(
        hit.iter().any(|&(a, _)| a == aid),
        "positive lookup must hit"
    );

    // Negative: append one segment (duplicate the first segment's char_def) just before the right
    // anchor. Sena is feat_width 0, so the inserted lane row is empty.
    let first_seg_cd = (0..shape.len())
        .find(|&i| shape.kind(i) == NodeKind::Segment)
        .map(|i| shape.char_def(i))
        .expect("has a segment");
    let mut b = ShapeBuilder::from_shape(&shape);
    let insert_at = shape.len() - 1; // just before the right anchor
    b.insert(
        insert_at,
        NodeKind::Segment,
        first_seg_cd,
        NodeFlags::EMPTY,
        &[],
    );
    let longer = b.freeze();
    assert_eq!(
        seg_char_defs(&longer).len(),
        seg_char_defs(&shape).len() + 1,
        "the longer shape has exactly one more segment",
    );

    let miss = index.search(&g, sid, &longer);
    assert!(
        !miss.iter().any(|&(a, _)| a == aid),
        "a strictly longer input must not accept at the shorter root's node (end-anchored); got {miss:?}",
    );
}

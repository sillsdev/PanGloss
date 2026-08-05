//! P7 closure census: the evidence that the remaining
//! `NaturalClassKind::Segments` union over-approximation (id-lane-OFF matching — the
//! rewrite/metathesis pipelines on every table, and ALL compile sites on >64-char-def tables,
//! where P10's `StrRep` identity lane is disabled) is **inert on the reference grammars**.
//!
//! What id-lane-off matching loses vs C# is exactly char-def *identity*: a constraint matches any
//! char-def whose feature lanes unify, not just the authored member(s). That can only diverge
//! from C# when the feature lanes underdetermine identity, i.e. when either
//!   (a) a `Segments`-kind class's lane-union admits a non-member ("inexact union"), or
//!   (b) two distinct char-defs have mutually unifiable lane rows (a literal-constraint
//!       over-match).
//! This census checks both properties directly on the real grammars and asserts the state under
//! which P7 was closed:
//!   - **every** `Segments`-kind natural class in Indonesian (32-def table as loaded) and Amharic
//!     (420-def table as loaded) has an **exact** union — zero over-matching non-members —
//!     including Amharic's 417-member "S" class; so (a) never occurs;
//!   - the only unifiable distinct char-def pairs are the three boundary defs among themselves
//!     (`+` / the `^0 *0 &0 ∅` null boundary / `.`; both grammars) plus Amharic's known
//!     byte-identical-FS authoring artifact ቂː/ሺ (ids 217/221, the pair the P5 census found)
//!     — and none of these is reachable: no
//!     Indonesian/Amharic `<PhoneticShape>` contains any boundary character other than `+`
//!     (asserted here), so a `+`-literal constraint can never sit against a different boundary
//!     kind, and neither ቂː nor ሺ occurs in any shape.
//!
//! (Sena needs no census: it has zero `<PhonologicalRule>`/`<MetathesisRule>` elements — the
//! id-lane-off pipelines never run — and its ≤64-def morph/allomorph sites carry the P10 lane.
//! No sample grammar has any `<MetathesisRule>` at all.)
//!
//! If this test ever FAILS, it does not mean engine code broke — it means the grammar data on
//! disk no longer satisfies the conditions under which P7 was closed (e.g. a re-authored
//! reference grammar with underspecified phonemes), and the P7/P10 residual should be re-scoped
//! for that grammar. Corroborating end-to-end evidence at closure time: Indonesian 121/121
//! byte-identical (P10), Amharic 673/673 zero-DIFFERENT (V1b, unchanged by P10 which is inert on
//! >64-def tables by construction), Sena 7121-word zero-DIFFERENT (V2b).
//!
//! The sample grammars are untracked local corpus files (per `rust-conversion.md` §8); the test
//! self-skips when they are absent (fresh clone / CI).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use pg_featstruct::bitvec::flat_unifiable;
use pg_grammar::chardef::{CharDefId, CharDefKind};
use pg_grammar::model::{Grammar, NaturalClassKind, TableId};

fn sample_path(name: &str) -> Option<PathBuf> {
    // CARGO_MANIFEST_DIR = .../rust/crates/pg-rules ; samples live at repo_root/samples/data.
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
}

fn load(name: &str) -> Option<(Grammar, String)> {
    let path = sample_path(name)?;
    let xml = std::fs::read_to_string(path).expect("read sample grammar");
    let g = pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load {name}: {e}"));
    Some((g, xml))
}

fn census(g: &Grammar, xml: &str, tag: &str) {
    let table = &g.char_tables[TableId(0).0 as usize];
    let w = g.phon_features.len();

    // (a) Every Segments-kind class union is exact: no non-member char-def unifies with it.
    for (ncid, nc) in g.natural_classes.iter().enumerate() {
        let NaturalClassKind::Segments(segs) = &nc.kind else {
            continue;
        };
        let mut lanes = vec![0u64; w];
        for cd in segs {
            for (i, &l) in table.get(*cd).feature_lanes().iter().enumerate() {
                lanes[i] |= l;
            }
        }
        let member: HashSet<u32> = segs.iter().map(|c| c.0).collect();
        let over: Vec<u32> = (0..table.len() as u32)
            .filter(|id| {
                !member.contains(id)
                    && flat_unifiable(table.get(CharDefId(*id)).feature_lanes(), &lanes)
            })
            .collect();
        assert!(
            over.is_empty(),
            "{tag}: Segments-class nc{ncid} {:?} ({} members) has an INEXACT id-lane-off union — \
             over-matches char-defs {over:?}; the P7-closure conditions no longer hold for this \
             grammar (see module doc)",
            nc.name,
            segs.len(),
        );
    }

    // (b) Unifiable distinct char-def pairs are confined to boundary×boundary (unreachable: only
    // `+` occurs in shapes, asserted below) plus segment pairs whose reps occur in no
    // <PhoneticShape> (Amharic's ቂː/ሺ authoring artifact).
    let shapes: Vec<&str> = {
        // Cheap literal scan; the loader has already validated the XML.
        let mut v = Vec::new();
        let mut rest = xml;
        while let Some(s) = rest.find("<PhoneticShape>") {
            rest = &rest[s + "<PhoneticShape>".len()..];
            let e = rest
                .find("</PhoneticShape>")
                .expect("balanced PhoneticShape");
            v.push(&rest[..e]);
            rest = &rest[e..];
        }
        v
    };
    for a in 0..table.len() as u32 {
        for b in (a + 1)..table.len() as u32 {
            let (da, db) = (table.get(CharDefId(a)), table.get(CharDefId(b)));
            if !flat_unifiable(da.feature_lanes(), db.feature_lanes()) {
                continue;
            }
            if da.kind() == CharDefKind::Boundary && db.kind() == CharDefKind::Boundary {
                continue; // handled by the boundary-reachability assertion below
            }
            // A unifiable segment pair is tolerable only if neither member's representations
            // appear in any lexical/insertion shape (then no concrete node can carry either).
            for cd in [da, db] {
                for rep in cd.representations() {
                    assert!(
                        !shapes.iter().any(|s| s.contains(rep.as_str())),
                        "{tag}: unifiable char-def pair ({a},{b}) is REACHABLE — rep {rep:?} \
                         occurs in a <PhoneticShape>; the P7-closure conditions no longer hold \
                         (see module doc)"
                    );
                }
            }
        }
    }
    // Boundary reachability: `+` must be the only boundary character occurring in any shape,
    // so no boundary-literal constraint can ever face a *different* boundary kind.
    for id in 0..table.len() as u32 {
        let cd = table.get(CharDefId(id));
        if cd.kind() != CharDefKind::Boundary {
            continue;
        }
        for rep in cd.representations() {
            if rep == "+" {
                continue;
            }
            assert!(
                !shapes.iter().any(|s| s.contains(rep.as_str())),
                "{tag}: boundary rep {rep:?} (char-def {id}) occurs in a <PhoneticShape> — a \
                 boundary-literal over-match is now reachable; re-scope the P7/P10 residual"
            );
        }
    }
    eprintln!(
        "{tag}: P7 census holds ({} defs, {} phon features)",
        table.len(),
        w
    );
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/{indonesian,amharic}-hc.xml); run with --include-ignored"]
fn p7_segments_union_census() {
    let mut ran = 0;
    for name in ["indonesian-hc.xml", "amharic-hc.xml"] {
        match load(name) {
            Some((g, xml)) => {
                census(&g, &xml, name);
                ran += 1;
            }
            None => eprintln!("skipping: {name} not present on disk"),
        }
    }
    eprintln!("p7_segments_union_census: {ran}/2 grammars censused");
}

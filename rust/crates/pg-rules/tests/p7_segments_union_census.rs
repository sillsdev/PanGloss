//! Census of the `NaturalClassKind::Segments` union over-approximation under id-lane-off matching: on the reference grammars every `Segments`-kind class union is exact, and no unifiable distinct char-def pair occurs in any `<PhoneticShape>`. A failure means the grammar data on disk changed, not that engine code broke. Self-skips when the untracked sample grammars are absent.

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

    // (b) Unifiable distinct char-def pairs are confined to boundary×boundary (unreachable: only `+` occurs in shapes) plus segment pairs whose reps occur in no `<PhoneticShape>`.
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
            // Tolerable only if neither member's representations appear in any lexical/insertion shape.
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
    // Boundary reachability: `+` must be the only boundary character in any shape, so a boundary-literal constraint can never face a different boundary kind.
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

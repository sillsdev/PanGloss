use super::*;
use pg_grammar::model::MorphemeId as Mid;

fn sample_path(name: &str) -> Option<std::path::PathBuf> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
}

fn load_indonesian() -> Option<Grammar> {
    let path = sample_path("indonesian-hc.xml")?;
    let xml = std::fs::read_to_string(&path).expect("read grammar");
    Some(pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load grammar: {e}")))
}

/// A bare-root word confirms to a non-empty set of matches, all sharing the expected root entry.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn confirm_bare_root_word_verifies() {
    let Some(g) = load_indonesian() else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    let morpher = Morpher::new(&g, usize::MAX);
    let owners = build_morpheme_owners(&g);
    // Finds "ajar"'s morpheme id from the grammar itself (entry25/entry26 homograph) rather than hard-coding one that might drift.
    let entry = g
        .entries
        .iter()
        .enumerate()
        .find(|(_, e)| {
            g.morphemes[e.morpheme.0 as usize].xml_key == "entry25"
                || g.morphemes[e.morpheme.0 as usize].xml_key == "entry26"
        })
        .map(|(i, e)| (i, e.morpheme));
    let Some((_idx, morpheme)) = entry else {
        eprintln!("skipping: entry25/entry26 not found in indonesian-hc.xml");
        return;
    };
    let candidate = Candidate {
        morphemes: vec![morpheme],
        root_index: 0,
    };
    let matches = confirm_all(&g, &owners, &morpher, &candidate, "ajar");
    assert!(
        !matches.is_empty(),
        "\"ajar\" must confirm to at least one analysis"
    );
    for (wa, _, _) in &matches {
        assert_eq!(wa.root_morpheme_index, 0);
        assert_eq!(wa.morpheme_ids, vec![morpheme.0]);
    }
}

/// A candidate whose root position is out of range (or empty) must confirm to nothing, never panic.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn confirm_rejects_out_of_range_root_index() {
    let Some(g) = load_indonesian() else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    let morpher = Morpher::new(&g, usize::MAX);
    let owners = build_morpheme_owners(&g);
    let bogus = Candidate {
        morphemes: vec![],
        root_index: 0,
    };
    assert!(confirm_all(&g, &owners, &morpher, &bogus, "ajar").is_empty());
}

/// A non-root morpheme id owned by neither a `LexEntry` nor an `MRule` must confirm to nothing.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn confirm_rejects_unowned_non_root_morpheme() {
    let Some(g) = load_indonesian() else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    let morpher = Morpher::new(&g, usize::MAX);
    let owners = build_morpheme_owners(&g);
    let root = g.entries[0].morpheme;
    let candidate = Candidate {
        morphemes: vec![root, Mid(u32::MAX - 5)],
        root_index: 0,
    };
    assert!(confirm_all(&g, &owners, &morpher, &candidate, "ajar").is_empty());
}

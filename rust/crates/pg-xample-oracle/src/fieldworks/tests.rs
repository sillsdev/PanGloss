use super::*;

#[test]
fn witness_dir_matches_the_verified_path() {
    // Composed, not spelled: a literal pinned this to one machine's drive letter and separators.
    let base = Path::new("machine");
    assert_eq!(
        witness_dir(base),
        base.join("conformance")
            .join("edge-cases")
            .join("deep-optional-affix-nesting")
            .join("fieldworks")
    );
}

#[test]
fn locate_projector_exe_env_override_refuses_a_missing_path() {
    std::env::set_var(PROJECTOR_EXE_ENV, r"Z:\definitely\does\not\exist.exe");
    let err = locate_projector_exe().expect_err("a nonexistent override path must be refused");
    assert!(matches!(err, FieldworksError::ExeNotFound { .. }));
    std::env::remove_var(PROJECTOR_EXE_ENV);
}

#[test]
fn identical_files_compare_equal_without_any_canonicalization() {
    let dir = std::env::temp_dir().join(format!(
        "pg-xample-oracle-cmp-identical-{}",
        std::process::id()
    ));
    let base = dir.join("base");
    let clone = dir.join("clone");
    std::fs::create_dir_all(&base).unwrap();
    std::fs::create_dir_all(&clone).unwrap();
    std::fs::write(base.join("MPBaseadctl.txt"), "\\maxp 12\n").unwrap();
    std::fs::write(clone.join("MPBaseadctl.txt"), "\\maxp 12\n").unwrap();
    std::fs::write(base.join("MPBasegram.txt"), "rule {template 5}\n").unwrap();
    std::fs::write(clone.join("MPBasegram.txt"), "rule {template 5}\n").unwrap();
    std::fs::write(base.join("MPBaselex.txt"), "\\lx 7\n").unwrap();
    std::fs::write(clone.join("MPBaselex.txt"), "\\lx 7\n").unwrap();
    xample_files_equivalent_ignoring_hvo_renumbering(&base, &clone, "MPBase")
        .expect("byte-identical files must compare equal");
    std::fs::remove_dir_all(&dir).ok();
}

// Writes a byte-identical adctl/gram/lex triplet into both dirs; the caller overwrites whichever one it means to perturb.
fn write_matching_triplet(base: &Path, clone: &Path, database: &str) {
    for name in ["adctl.txt", "gram.txt", "lex.txt"] {
        let text = format!("-- unperturbed {name} --\n");
        std::fs::write(base.join(format!("{database}{name}")), &text).unwrap();
        std::fs::write(clone.join(format!("{database}{name}")), &text).unwrap();
    }
}

#[test]
fn hvo_renumbered_files_compare_equal_after_canonicalization() {
    let dir = std::env::temp_dir().join(format!(
        "pg-xample-oracle-cmp-hvocanon-{}",
        std::process::id()
    ));
    let base = dir.join("base");
    let clone = dir.join("clone");
    std::fs::create_dir_all(&base).unwrap();
    std::fs::create_dir_all(&clone).unwrap();
    write_matching_triplet(&base, &clone, "MPBase");
    // RootPOS106 -> RootPOS94 after a deletion shifts every later hvo; the surrounding text is unchanged.
    std::fs::write(
        base.join("MPBasegram.txt"),
        "rule { rootCat:106 template 5}\nRootPOS106\n\\wc 106\n(106_2)\n",
    )
    .unwrap();
    std::fs::write(
        clone.join("MPBasegram.txt"),
        "rule { rootCat:94 template 5}\nRootPOS94\n\\wc 94\n(94_2)\n",
    )
    .unwrap();
    xample_files_equivalent_ignoring_hvo_renumbering(&base, &clone, "MPBase")
        .expect("a file differing only by a consistent hvo renumbering must compare equal after canonicalization");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_corrupted_cap_is_still_caught_after_canonicalization() {
    // Falsification: an over-broad raw-\d+ blind would also erase this real \maxp regression.
    let dir = std::env::temp_dir().join(format!(
        "pg-xample-oracle-cmp-corrupt-{}",
        std::process::id()
    ));
    let base = dir.join("base");
    let clone = dir.join("clone");
    std::fs::create_dir_all(&base).unwrap();
    std::fs::create_dir_all(&clone).unwrap();
    write_matching_triplet(&base, &clone, "MPBase");
    std::fs::write(base.join("MPBaseadctl.txt"), "\\maxp 12\nRootPOS106\n").unwrap();
    std::fs::write(clone.join("MPBaseadctl.txt"), "\\maxp 3\nRootPOS94\n").unwrap();
    let mismatches = xample_files_equivalent_ignoring_hvo_renumbering(&base, &clone, "MPBase")
        .expect_err("a corrupted \\maxp cap must not be canonicalized away");
    assert_eq!(
        mismatches.len(),
        1,
        "only adctl.txt should differ: {mismatches:?}"
    );
    assert_eq!(mismatches[0].file_name, "MPBaseadctl.txt");
}

// A wrong-category assignment, not a renumbering: RootPOS/rootCat still resolve to index 0, the corrupted \wc value is a brand-new one.
#[test]
fn wc_field_corruption_is_caught() {
    let dir = std::env::temp_dir().join(format!(
        "pg-xample-oracle-cmp-wc-corrupt-{}",
        std::process::id()
    ));
    let base = dir.join("base");
    let clone = dir.join("clone");
    std::fs::create_dir_all(&base).unwrap();
    std::fs::create_dir_all(&clone).unwrap();
    write_matching_triplet(&base, &clone, "MPBase");
    std::fs::write(
        base.join("MPBasegram.txt"),
        "rule { rootCat:106 template 5}\nRootPOS106\n\\wc 106\n",
    )
    .unwrap();
    // rootCat/RootPOS correctly renumbered to 94; \wc wrongly set to an unrelated 999.
    std::fs::write(
        clone.join("MPBasegram.txt"),
        "rule { rootCat:94 template 5}\nRootPOS94\n\\wc 999\n",
    )
    .unwrap();
    let mismatches = xample_files_equivalent_ignoring_hvo_renumbering(&base, &clone, "MPBase")
        .expect_err(
            "a \\wc value inconsistent with the rest of the file's renumbering must be caught",
        );
    assert_eq!(
        mismatches.len(),
        1,
        "only gram.txt should differ: {mismatches:?}"
    );
    assert_eq!(mismatches[0].file_name, "MPBasegram.txt");
}

// A mis-wired reference at the same slot index, not a renumbering.
#[test]
fn paren_slot_corruption_is_caught() {
    let dir = std::env::temp_dir().join(format!(
        "pg-xample-oracle-cmp-paren-corrupt-{}",
        std::process::id()
    ));
    let base = dir.join("base");
    let clone = dir.join("clone");
    std::fs::create_dir_all(&base).unwrap();
    std::fs::create_dir_all(&clone).unwrap();
    write_matching_triplet(&base, &clone, "MPBase");
    std::fs::write(
        base.join("MPBasegram.txt"),
        "rule { rootCat:106 template 5}\nRootPOS106\n(106_2)\n",
    )
    .unwrap();
    // rootCat/RootPOS correctly renumbered to 94; the parenthesized slot reference wrongly set to an unrelated 999.
    std::fs::write(
        clone.join("MPBasegram.txt"),
        "rule { rootCat:94 template 5}\nRootPOS94\n(999_2)\n",
    )
    .unwrap();
    let mismatches = xample_files_equivalent_ignoring_hvo_renumbering(&base, &clone, "MPBase")
        .expect_err("a (hvo_slotIndex) reference inconsistent with the rest of the file's renumbering must be caught");
    assert_eq!(
        mismatches.len(),
        1,
        "only gram.txt should differ: {mismatches:?}"
    );
    assert_eq!(mismatches[0].file_name, "MPBasegram.txt");
}

// Checks the ported prefix list against build.ps1's own source text so the two cannot silently drift apart.
#[test]
fn hvo_identifier_prefixes_match_build_ps1_verbatim() {
    let ps1_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tools/xample-projector/build.ps1");
    let text = std::fs::read_to_string(&ps1_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", ps1_path.display()));
    let block_re = Regex::new(r"(?s)\$script:HvoIdentifierPrefixes\s*=\s*@\((.*?)\)").unwrap();
    let block = block_re
        .captures(&text)
        .unwrap_or_else(|| {
            panic!(
                "{}: no longer defines $script:HvoIdentifierPrefixes",
                ps1_path.display()
            )
        })
        .get(1)
        .unwrap()
        .as_str();
    let item_re = Regex::new(r"'([^']+)'").unwrap();
    let extracted: Vec<&str> = item_re
        .captures_iter(block)
        .map(|c| c.get(1).unwrap().as_str())
        .collect();
    assert_eq!(
        extracted, HVO_IDENTIFIER_PREFIXES,
        "this crate's HVO_IDENTIFIER_PREFIXES has drifted from build.ps1's own $script:HvoIdentifierPrefixes"
    );
}

#[test]
fn missing_file_is_reported_not_silently_ignored() {
    let dir = std::env::temp_dir().join(format!(
        "pg-xample-oracle-cmp-missing-{}",
        std::process::id()
    ));
    let base = dir.join("base");
    let clone = dir.join("clone");
    std::fs::create_dir_all(&base).unwrap();
    std::fs::create_dir_all(&clone).unwrap();
    std::fs::write(base.join("MPBaseadctl.txt"), "\\maxp 12\n").unwrap();
    // clone's file deliberately absent.
    let mismatches = xample_files_equivalent_ignoring_hvo_renumbering(&base, &clone, "MPBase")
        .expect_err("a missing clone file must be reported, never silently skipped");
    assert!(mismatches.iter().any(|m| m.file_name == "MPBaseadctl.txt"));
    std::fs::remove_dir_all(&dir).ok();
}

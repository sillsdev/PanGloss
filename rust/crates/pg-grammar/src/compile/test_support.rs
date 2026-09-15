//! Shared `Grammar` comparison for this crate's own unit tests and its integration test targets --
//! feature-gated (see this crate's `Cargo.toml`) so no production build carries comparison-only code.

/// Field-for-field `Grammar` equality; `phon_features`/`char_tables`/`fs_interner` own a private HashMap whose iteration order (not content) can differ between two identical builds, so those three go through deterministic public accessors instead of `Debug`. Destructured exhaustively (no `..`) so a field added to `Grammar` later breaks this build instead of going uncompared.
pub fn assert_grammars_equal(a: &crate::model::Grammar, b: &crate::model::Grammar) {
    let crate::model::Grammar {
        name: name_a,
        phon_features: phon_features_a,
        char_tables: char_tables_a,
        syn_features: syn_features_a,
        fs_interner: fs_interner_a,
        mpr_names: mpr_names_a,
        mpr_features: mpr_features_a,
        mpr_groups: mpr_groups_a,
        stem_names: stem_names_a,
        families: families_a,
        natural_classes: natural_classes_a,
        morphemes: morphemes_a,
        allomorph_owners: allomorph_owners_a,
        allomorph_sources: allomorph_sources_a,
        prules: prules_a,
        mrules: mrules_a,
        templates: templates_a,
        entries: entries_a,
        strata: strata_a,
    } = a;
    let crate::model::Grammar {
        name: name_b,
        phon_features: phon_features_b,
        char_tables: char_tables_b,
        syn_features: syn_features_b,
        fs_interner: fs_interner_b,
        mpr_names: mpr_names_b,
        mpr_features: mpr_features_b,
        mpr_groups: mpr_groups_b,
        stem_names: stem_names_b,
        families: families_b,
        natural_classes: natural_classes_b,
        morphemes: morphemes_b,
        allomorph_owners: allomorph_owners_b,
        allomorph_sources: allomorph_sources_b,
        prules: prules_b,
        mrules: mrules_b,
        templates: templates_b,
        entries: entries_b,
        strata: strata_b,
    } = b;

    assert_eq!(name_a, name_b, "name");
    assert_eq!(format!("{:?}", syn_features_a), format!("{:?}", syn_features_b), "syn_features");
    assert_eq!(mpr_names_a, mpr_names_b, "mpr_names");
    assert_eq!(format!("{:?}", mpr_features_a), format!("{:?}", mpr_features_b), "mpr_features");
    assert_eq!(format!("{:?}", mpr_groups_a), format!("{:?}", mpr_groups_b), "mpr_groups");
    assert_eq!(format!("{:?}", stem_names_a), format!("{:?}", stem_names_b), "stem_names");
    assert_eq!(format!("{:?}", families_a), format!("{:?}", families_b), "families");
    assert_eq!(
        format!("{:?}", natural_classes_a),
        format!("{:?}", natural_classes_b),
        "natural_classes"
    );
    assert_eq!(format!("{:?}", morphemes_a), format!("{:?}", morphemes_b), "morphemes");
    assert_eq!(allomorph_owners_a, allomorph_owners_b, "allomorph_owners");
    assert_eq!(allomorph_sources_a, allomorph_sources_b, "allomorph_sources");
    assert_eq!(format!("{:?}", prules_a), format!("{:?}", prules_b), "prules");
    assert_eq!(format!("{:?}", mrules_a), format!("{:?}", mrules_b), "mrules");
    assert_eq!(format!("{:?}", templates_a), format!("{:?}", templates_b), "templates");
    assert_eq!(format!("{:?}", entries_a), format!("{:?}", entries_b), "entries");
    assert_eq!(format!("{:?}", strata_a), format!("{:?}", strata_b), "strata");

    assert_eq!(phon_features_a.len(), phon_features_b.len(), "phon_features.len");
    for i in 0..phon_features_a.len() {
        let flat = crate::featsys::FlatIndex(i as u32);
        assert_eq!(phon_features_a.feature_xml_id(flat), phon_features_b.feature_xml_id(flat));
        assert_eq!(phon_features_a.feature_name(flat), phon_features_b.feature_name(flat));
        assert_eq!(phon_features_a.mask(flat), phon_features_b.mask(flat));
        assert_eq!(phon_features_a.default_bits(flat), phon_features_b.default_bits(flat));
        let sym_count = phon_features_a.symbol_count(flat);
        assert_eq!(sym_count, phon_features_b.symbol_count(flat));
        for idx in 0..sym_count as u32 {
            assert_eq!(
                phon_features_a.symbol_name(flat, idx),
                phon_features_b.symbol_name(flat, idx)
            );
        }
    }

    assert_eq!(char_tables_a.len(), char_tables_b.len(), "char_tables.len");
    for (ta, tb) in char_tables_a.iter().zip(char_tables_b.iter()) {
        assert_eq!(ta.xml_id(), tb.xml_id());
        assert_eq!(ta.name(), tb.name());
        let a_defs: Vec<_> = ta.iter().collect();
        let b_defs: Vec<_> = tb.iter().collect();
        assert_eq!(a_defs.len(), b_defs.len());
        for ((ida, cda), (idb, cdb)) in a_defs.iter().zip(b_defs.iter()) {
            assert_eq!(ida, idb);
            assert_eq!(cda.xml_id(), cdb.xml_id());
            assert_eq!(cda.kind(), cdb.kind());
            assert_eq!(cda.representations(), cdb.representations());
            assert_eq!(cda.representations_nfd(), cdb.representations_nfd());
            assert_eq!(cda.feature_lanes(), cdb.feature_lanes());
        }
        assert_eq!(ta.unif_closure_rows(), tb.unif_closure_rows());
    }

    let a_fs: Vec<_> = fs_interner_a.iter().collect();
    let b_fs: Vec<_> = fs_interner_b.iter().collect();
    assert_eq!(a_fs, b_fs, "fs_interner");
}

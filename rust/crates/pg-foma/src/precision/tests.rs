use super::*;

fn load_sample(name: &str) -> Option<Grammar> {
    let full = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../samples/data")
        .join(name);
    let xml = std::fs::read_to_string(&full).ok()?;
    Some(pg_grammar::load(&xml).unwrap())
}

fn load_conformance(path: &str) -> Grammar {
    let full = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../machine/conformance")
        .join(path);
    let xml = std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("{}: {e}", full.display()));
    pg_grammar::load(&xml).unwrap()
}

/// Sena has 144 `<RequiredEnvironments>` elements; a handful are literal-left, no-right instances the catalog must cover.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
fn sena_catalog_finds_the_expected_left_literal_instances() {
    let Some(g) = load_sample("sena-hc.xml") else {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    };
    let catalog = ConstraintCatalog::build(&g);
    assert!(
        !catalog.env.is_empty(),
        "Sena declares real environments; catalog must see them"
    );
    let coverable: Vec<&EnvConstraint> = catalog.coverable().collect();
    assert!(
        coverable.len() >= 2,
        "expected at least the root-side /ma_//na_ instances plus the rule-side /mb_ one, \
         got {} coverable: {coverable:?}",
        coverable.len()
    );
    assert!(
        coverable.iter().all(|c| c.require),
        "every coverable Sena instance is a RequiredEnvironments (none Excluded), got {coverable:?}"
    );
    assert!(
        coverable
            .iter()
            .any(|c| c.owner_kind == EnvOwnerKind::Root
                && matches!(&c.coverage, EnvCoverage::LeftLiteral { literal_variants }
                    if literal_variants.iter().any(|v| v == "ma") || literal_variants.iter().any(|v| v == "na"))),
        "expected a root-side /ma_ or /na_ instance among {coverable:?}"
    );
    assert!(
        coverable.iter().any(|c| {
            c.owner_kind == EnvOwnerKind::Rule
                && matches!(&c.coverage, EnvCoverage::LeftLiteral { literal_variants }
                    if literal_variants.iter().any(|v| v == "mb"))
        }),
        "expected the rule-side /mb_ instance (msubrule60) among {coverable:?}"
    );

    // Ids are stable/deterministic across repeated builds of the SAME grammar.
    let catalog2 = ConstraintCatalog::build(&g);
    let ids: Vec<u32> = catalog.env.iter().map(|c| c.id).collect();
    let ids2: Vec<u32> = catalog2.env.iter().map(|c| c.id).collect();
    assert_eq!(
        ids, ids2,
        "catalog ids must be deterministic across rebuilds"
    );
    // Attribute names are zero-padded ENV.nnnn per the design's own worked example.
    assert!(catalog.env[0].attr.starts_with("ENV."));
    assert_eq!(catalog.env[0].attr.len(), "ENV.".len() + 4);
}

/// Indonesian declares zero environments; the catalog is empty and `AllFlags` decides trivially all `Strip`.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn indonesian_catalog_is_empty() {
    let Some(g) = load_sample("indonesian-hc.xml") else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    let catalog = ConstraintCatalog::build(&g);
    assert!(
        catalog.env.is_empty(),
        "Indonesian declares no environments at all"
    );
    let report = catalog.decide(PrecisionConfig::AllFlags);
    assert!(report.decisions.is_empty());
}

/// A multi-environment allomorph is `Unsupported { reason: "or-ambiguous" }` for every one of its environments.
#[test]
fn multi_environment_allomorph_is_or_ambiguous_even_if_individually_simple() {
    // This fixture is scoped to a different gate, so this is a best-effort structural probe.
    let g = load_conformance("edge-cases/disjunctive-recheck/grammar.xml");
    let catalog = ConstraintCatalog::build(&g);
    for c in &catalog.env {
        if c.sibling_count > 1 {
            assert!(
                matches!(
                    c.coverage,
                    EnvCoverage::Unsupported {
                        reason: "or-ambiguous"
                    }
                ),
                "constraint {c:?} has sibling_count > 1 but wasn't marked or-ambiguous"
            );
        }
    }
}

fn one_constraint_catalog(id: u32, owner: AllomorphId, literal: &str) -> ConstraintCatalog {
    ConstraintCatalog {
        env: vec![EnvConstraint {
            id,
            attr: format!("ENV.{id:04}"),
            family: ConstraintFamily::Environment,
            owner_kind: EnvOwnerKind::Rule,
            allomorph: owner,
            env_index: 0,
            require: true,
            sibling_count: 1,
            coverage: EnvCoverage::LeftLiteral {
                literal_variants: vec![literal.to_string()],
            },
        }],
    }
}

/// Under `Strip`, `tagged_lower` is a byte-identical passthrough regardless of `owner`.
#[test]
fn precision_emit_tagged_lower_is_passthrough_under_strip() {
    let catalog = one_constraint_catalog(0, AllomorphId(7), "mb");
    let pk = PrecisionEmit::build(&catalog, PrecisionConfig::Strip);
    assert!(pk.flag_symbols.is_empty());
    assert_eq!(
        pk.tagged_lower("tumba", "tumba", Some(AllomorphId(7))),
        "tumba"
    );
    assert_eq!(pk.tagged_lower("", "", Some(AllomorphId(7))), "0");
    assert_eq!(pk.tagged_lower("", "", None), "0");
}

/// Under `AllFlags`, flag symbols are dot-free (`flag_id`, not `attr`) and follow `@[R|P].ENV{id}.[y|n]@`.
#[test]
fn precision_emit_flag_symbols_are_dot_free_in_the_name_field() {
    let catalog = one_constraint_catalog(7, AllomorphId(3), "mb");
    let pk = PrecisionEmit::build(&catalog, PrecisionConfig::AllFlags);
    assert_eq!(
        pk.flag_symbols,
        vec![
            "@R.ENV7.y@".to_string(),
            "@P.ENV7.y@".to_string(),
            "@P.ENV7.n@".to_string()
        ]
    );
}

/// Under `AllFlags`, the owner gets the require prefix, every non-empty entry gets one y/n set flag, empty gets none.
#[test]
fn precision_emit_tagged_lower_gates_owner_and_sets_on_every_entry() {
    let catalog = one_constraint_catalog(7, AllomorphId(3), "mb");
    let pk = PrecisionEmit::build(&catalog, PrecisionConfig::AllFlags);

    // Owner's own entry: surface "i" doesn't end in "mb", so its own set-flag is "n".
    let owner_lower = pk.tagged_lower("i", "i", Some(AllomorphId(3)));
    assert_eq!(owner_lower, "@R.ENV7.y@i@P.ENV7.n@");

    // An unrelated entry (no owner) whose surface ends in "mb" -> "y".
    let setter_lower = pk.tagged_lower("tumb", "tumb", None);
    assert_eq!(setter_lower, "tumb@P.ENV7.y@");

    // An unrelated entry that does NOT end in "mb" (and isn't a suffix of it) -> "n".
    let plain_lower = pk.tagged_lower("kucita", "kucita", None);
    assert_eq!(plain_lower, "kucita@P.ENV7.n@");

    // An EMPTY surface gets no set flag at all, even for the owner (only the @R@ prefix).
    let empty_owner_lower = pk.tagged_lower("", "", Some(AllomorphId(3)));
    assert_eq!(empty_owner_lower, "@R.ENV7.y@0");
    let empty_plain_lower = pk.tagged_lower("", "", None);
    assert_eq!(empty_plain_lower, "0");
}

/// `could_satisfy`'s two disjuncts: whole-literal `ends_with`, and the boundary-spanning proper-suffix case.
#[test]
fn could_satisfy_covers_whole_literal_and_boundary_spanning_suffix() {
    // Whole literal spelled within one entry.
    assert!(could_satisfy("tumb", &["mb".to_string()]));
    // Boundary-spanning: "i" is a PROPER suffix of "mi" (shorter, and "mi" ends with "i").
    assert!(could_satisfy("i", &["mi".to_string()]));
    // Not a match either way.
    assert!(!could_satisfy("ku", &["mi".to_string()]));
    // Matches if any literal variant matches: "an" is a proper suffix of both "man" and "nan".
    assert!(could_satisfy("an", &["man".to_string(), "nan".to_string()]));
    // An entry EQUAL to the literal itself still satisfies (ends_with is reflexive).
    assert!(could_satisfy("mb", &["mb".to_string()]));
    // Empty literal variants never match.
    assert!(!could_satisfy("mb", &[String::new()]));
}

/// `flag_id` excludes the digit `0` and the dot, and the `0`->`Z` substitution stays injective.
#[test]
fn flag_id_has_no_zero_digit_and_never_contains_a_dot() {
    assert_eq!(flag_id(7), "7");
    assert_eq!(flag_id(70), "7Z");
    assert_eq!(flag_id(700), "7ZZ");
    assert_ne!(flag_id(7), flag_id(70));
    for id in [0, 7, 10, 70, 700, 1007] {
        let fid = flag_id(id);
        assert!(!fid.contains('.'), "flag_id({id}) must never contain a dot");
        assert!(
            !fid.contains('0'),
            "flag_id({id}) must never contain the digit 0, got {fid:?}"
        );
    }
}

/// A grammar with zero phonological rules (e.g. Sena) has no risk at all: the loop is a no-op.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
fn prule_tail_rewrite_risk_is_false_with_no_phonological_rules() {
    let Some(g) = load_sample("sena-hc.xml") else {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    };
    assert!(
        g.prules.is_empty(),
        "Sena is the zero-phonological-rules reference grammar"
    );
    assert!(!prule_tail_rewrite_risk(&g, &["ma".to_string()]));
}

#[test]
fn wasm_surface_matches_native_supplied_lexicon_operations_and_removes_reload_api() {
    let source = include_str!("../src/lib.rs");
    for operation in [
        "classCatalog",
        "addSuppliedEntry",
        "getSuppliedEntry",
        "listSuppliedEntries",
        "searchSuppliedEntries",
        "updateSuppliedEntry",
        "removeSuppliedEntry",
        "clearSuppliedEntries",
        "setGlossLanguage",
        "setEntryAuthority",
        "exportSuppliedLexicon",
        "importSuppliedLexicon",
        "classificationMatrix",
        "analyzeWord",
    ] {
        assert!(
            source.contains(&format!("js_name = {operation}")),
            "missing {operation}"
        );
    }
    for legacy in [
        "candidateClasses",
        "disambiguatingForms",
        "applyUserLexicon",
        "augment_xml",
    ] {
        assert!(!source.contains(legacy), "legacy API remains: {legacy}");
    }
    assert!(source.contains("pub struct ClassificationGuide"));
}

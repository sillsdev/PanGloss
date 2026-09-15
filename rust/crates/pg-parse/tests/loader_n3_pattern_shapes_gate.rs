//! The loaded-grammar STRUCTURE behind the pattern-language fallback, which signatures cannot show.
//! Why this file survived the v1 cull and its siblings did not: docs/design/fixture-pins.md

use pg_conformance_fixtures::require_fixture;

/// A loader keeping the entry but dropping its pattern-shaped allomorph fails here alone.
/// Why the structural claim needs its own pin: docs/design/fixture-pins.md
#[test]
fn the_pattern_shaped_allomorph_survives_loading() {
    let fixture = require_fixture("edge-cases", "loader-pattern-shapes");
    let grammar = pg_grammar::load(&fixture.load_grammar_xml())
        .unwrap_or_else(|e| panic!("{}: grammar failed to load: {e}", fixture.label()));

    // Counted, not pinned to a number: an exact count pins the fixture's shape, not the loader's.
    let entries_with_an_allomorph = grammar
        .entries
        .iter()
        .filter(|e| !e.allomorphs.is_empty())
        .count();
    assert_eq!(
        entries_with_an_allomorph,
        grammar.entries.len(),
        "{}: every lexical entry declares exactly one pattern-shaped allomorph, so an entry \
         carrying none means `load_root_allomorph` dropped it — the defect this pins. Entries: {}",
        fixture.label(),
        grammar.entries.len()
    );
    assert!(
        grammar.entries.len() >= 2,
        "{}: expected at least the two pattern-shaped entries (`b[Vowel]t` and `b([Vowel])t`); \
         found {}. A dropped ENTRY looks identical to a smaller fixture from inside this check.",
        fixture.label(),
        grammar.entries.len()
    );
}

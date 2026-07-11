//! F7's NEW required gate (`HYBRID_FST_RUST_PLAN.md`'s F7 entry, "SCOPE EXPANDED 2026-07-11"):
//! "a toy grammar with a genuine word-internal (non-junction-conditioned) single-segment
//! phonological rule, generated so its C# `fst-candidates` dump shows real
//! `LockstepPhonologyProposer`/`ComposedPhonologyProposer` lines (not just FST/Redup) -- byte-match
//! this specifically in Rust."
//!
//! ## The real C# oracle dump was generated (not assumed) -- ground-truth findings
//! `F7LockstepComposedToyGrammar.xml` was fed to the real C# `hc.dll` tool (fst-oracle worktree,
//! `dotnet hc.dll -i grammar.xml -s script.txt`, `fst-candidates`/`fst-batch`, both chain-off and
//! `--chain`). Two real authoring bugs were found and fixed THIS way (not by inspection):
//! 1. The hand-authored XML's own doc comments used "--" (em-dash style), which is illegal inside
//!    an XML comment per the XML spec (`<!--...-->`  content may not contain "--"). C#'s
//!    `System.Xml` throws on load; this port's own `hc-grammar` loader is lenient and never caught
//!    it. Fixed by replacing every "--" with " - " in the comment bodies (not in the delimiters).
//! 2. `AffixTemplate`'s `final` attribute has a DTD-declared default (`"true"`), but the loader
//!    (`XmlLanguageLoader.cs:1307`, `(bool)tempElem.Attribute("final")`) does a NON-NULLABLE
//!    explicit cast with no `??` fallback, and DTD defaults are only applied by a DTD-VALIDATING
//!    parser -- which this hand-authored XML (no `<!DOCTYPE>`) never invokes. Every other
//!    attribute in the whole loader uses a nullable cast with a manual default; this is the ONE
//!    exception. Fixed by adding `final="true"` explicitly to `<AffixTemplate>`.
//!
//! ## The dedup-shadowing finding (confirmed empirically against the real C# dump, not derived)
//! The real C# `fst-candidates` dump for "lazi" (both chain-off AND `--chain`) shows ONLY
//! `ComposedPhonologyProposer -> eLas+mrLoc:0` -- `LockstepPhonologyProposer`'s (and
//! `ChainPhonologyProposer`'s) line never appears, even though each independently computes the
//! IDENTICAL candidate. This is NOT a bug or a toy-design flaw: `CompositeProposer`'s (and this
//! port's `CompositeAnalyzer`'s) fixed order places `ComposedPhonologyProposer` (position 5) BEFORE
//! `LockstepPhonologyProposer`/`ChainPhonologyProposer` (position 6), and the composite's dedup is
//! first-proposer-wins by signature -- so whenever both proposers find the SAME candidate (expected
//! for any single well-formed rule, since `ComposedPhonologyProposer`'s general un-apply is a
//! structural superset of what the narrower v1-compiled Lockstep/Chain proposers can find), Composed
//! ALWAYS wins the dedup at the composite level, on BOTH sides of the port identically (confirmed by
//! generating the actual C# dump, not by reasoning about it in Rust -- see the module-level advisor
//! guidance this milestone recorded: "you're byte-matching a theory of the C# output instead of the
//! C# output... generating it replaces this whole reasoning spiral with a fact").
//!
//! Given that, the achievable and CORRECT reading of the gate is: (a) byte-match the real composite
//! dump (which is what `indonesian_real_lockstep_and_composed_are_still_corpus_inert` in
//! `f7_chain_gate.rs` already does for the corpus-inert case) -- here it shows a REAL, non-empty,
//! non-FST/Redup proposer line (`ComposedPhonologyProposer`), which is the substantive thing F6 left
//! unconfirmed; and (b) independently confirm `LockstepPhonologyProposer` and `ChainPhonologyProposer`
//! EACH produce this candidate too, in isolation (outside the composite's dedup, which is exactly
//! the mechanism that shadows them at the composite level, on both sides of the port identically).

use std::path::{Path, PathBuf};

use hc_hybrid::composite::{self, CompositeAnalyzer};
use hc_hybrid::proposers::{ChainPhonologyProposer, LockstepPhonologyProposer};
use hc_hybrid::replay;
use hc_hybrid::surface::SurfacePhonology;
use hc_hybrid::trie::Trie;
use hc_hybrid::walk;
use hc_parse::Morpher;

const TOY_XML: &str = include_str!("fixtures/fst-advisor-toys/F7LockstepComposedToyGrammar.xml");

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fst-advisor-toys").join(name)
}

fn read_lines(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
        .lines()
        .map(|l| l.trim_end_matches('\r').to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

fn read_words(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path).unwrap().lines().map(|w| w.trim().to_string()).filter(|w| !w.is_empty()).collect()
}

fn load() -> hc_grammar::model::Grammar {
    hc_grammar::load(TOY_XML).unwrap_or_else(|e| panic!("toy grammar failed to load: {e}"))
}

/// Byte-matches the real C# `fst-candidates` dump (chain-off: FstTemplateAnalyzer, Reduplication,
/// Infix, ComposedPhonologyProposer (real), LockstepPhonologyProposer (real)) over the toy grammar's
/// 3-word list. This is THE headline byte-match: it proves Composed contributes a REAL,
/// non-empty, non-FST/Redup candidate on "lazi" -- exactly what F6 left unconfirmed.
#[test]
fn toy_candidates_chainoff_matches_csharp_oracle_dump() {
    let g = load();
    let surface = SurfacePhonology::new(&g);
    let build_morpher = Morpher::new(&g, usize::MAX);
    let trie = Trie::build(&g, &surface, &build_morpher, 1_000_000, 2, true);
    let lockstep_morpher = Morpher::new(&g, usize::MAX);
    let composite = CompositeAnalyzer::new(&g, &trie, &surface, walk::DEFAULT_MAX_BEAM_WORK, false)
        .with_composed_phonology(&g)
        .with_lockstep_phonology(&g, &surface, &lockstep_morpher, 1_000_000, 2);

    let words = read_words(&fixture_path("F7LockstepComposedToyGrammar.words.txt"));
    let golden = read_lines(&fixture_path("F7LockstepComposedToyGrammar.candidates-chainoff.tsv"));

    let mut rust_lines = Vec::new();
    for (i, word) in words.iter().enumerate() {
        rust_lines.extend(composite::candidate_lines(&g, &composite, i, word));
    }
    assert_eq!(rust_lines, golden, "toy candidates (chain-off) diverge from the real C# oracle dump");
    assert!(
        rust_lines.iter().any(|l| l.contains("ComposedPhonologyProposer")),
        "the whole point of this gate: a real ComposedPhonologyProposer line must appear"
    );
}

/// Same byte-match, `--chain` mode (ChainPhonologyProposer at position 6 instead of Lockstep).
/// The real C# dump is IDENTICAL to the chain-off one here (Composed still wins the dedup over
/// Chain too) -- this is expected and confirmed against the real oracle output, not assumed.
#[test]
fn toy_candidates_chainon_matches_csharp_oracle_dump() {
    let g = load();
    let surface = SurfacePhonology::new(&g);
    let build_morpher = Morpher::new(&g, usize::MAX);
    let trie = Trie::build(&g, &surface, &build_morpher, 1_000_000, 2, true);
    let chain_morpher = Morpher::new(&g, usize::MAX);
    let composite = CompositeAnalyzer::new(&g, &trie, &surface, walk::DEFAULT_MAX_BEAM_WORK, false)
        .with_composed_phonology(&g)
        .with_chain_phonology(&g, &surface, &chain_morpher, 1_000_000, 2);

    let words = read_words(&fixture_path("F7LockstepComposedToyGrammar.words.txt"));
    let golden = read_lines(&fixture_path("F7LockstepComposedToyGrammar.candidates-chainon.tsv"));

    let mut rust_lines = Vec::new();
    for (i, word) in words.iter().enumerate() {
        rust_lines.extend(composite::candidate_lines(&g, &composite, i, word));
    }
    assert_eq!(rust_lines, golden, "toy candidates (--chain) diverge from the real C# oracle dump");
}

/// Byte-matches the real C# `fst-batch` verified dump, chain-off: only "lazi" verifies, to exactly
/// one analysis `eLas+mrLoc:0`; "las" and "lasi" verify to zero (confirmed against the real oracle,
/// not assumed -- "las" alone does not pass the restricted-selector verify step in C# either).
#[test]
fn toy_batch_chainoff_matches_csharp_oracle_dump() {
    let g = load();
    let surface = SurfacePhonology::new(&g);
    let build_morpher = Morpher::new(&g, usize::MAX);
    let trie = Trie::build(&g, &surface, &build_morpher, 1_000_000, 2, true);
    let lockstep_morpher = Morpher::new(&g, usize::MAX);
    let composite = CompositeAnalyzer::new(&g, &trie, &surface, walk::DEFAULT_MAX_BEAM_WORK, false)
        .with_composed_phonology(&g)
        .with_lockstep_phonology(&g, &surface, &lockstep_morpher, 1_000_000, 2);

    let verify_morpher = Morpher::new(&g, usize::MAX);
    let owners = replay::build_morpheme_owners(&g);
    let words = read_words(&fixture_path("F7LockstepComposedToyGrammar.words.txt"));
    let golden = read_lines(&fixture_path("F7LockstepComposedToyGrammar.batch-chainoff.tsv"));

    let mut rust_lines = Vec::new();
    for (i, word) in words.iter().enumerate() {
        let [started, result] = composite::batch_lines(&g, &composite, &verify_morpher, &owners, i, word);
        rust_lines.push(started);
        rust_lines.push(result);
    }
    assert_eq!(rust_lines, golden);
}

/// Byte-matches the real C# `fst-batch` verified dump, `--chain`: identical outcome to chain-off
/// (confirmed against the real oracle output).
#[test]
fn toy_batch_chainon_matches_csharp_oracle_dump() {
    let g = load();
    let surface = SurfacePhonology::new(&g);
    let build_morpher = Morpher::new(&g, usize::MAX);
    let trie = Trie::build(&g, &surface, &build_morpher, 1_000_000, 2, true);
    let chain_morpher = Morpher::new(&g, usize::MAX);
    let composite = CompositeAnalyzer::new(&g, &trie, &surface, walk::DEFAULT_MAX_BEAM_WORK, false)
        .with_composed_phonology(&g)
        .with_chain_phonology(&g, &surface, &chain_morpher, 1_000_000, 2);

    let verify_morpher = Morpher::new(&g, usize::MAX);
    let owners = replay::build_morpheme_owners(&g);
    let words = read_words(&fixture_path("F7LockstepComposedToyGrammar.words.txt"));
    let golden = read_lines(&fixture_path("F7LockstepComposedToyGrammar.batch-chainon.tsv"));

    let mut rust_lines = Vec::new();
    for (i, word) in words.iter().enumerate() {
        let [started, result] = composite::batch_lines(&g, &composite, &verify_morpher, &owners, i, word);
        rust_lines.push(started);
        rust_lines.push(result);
    }
    assert_eq!(rust_lines, golden);
}

/// The isolation half of the gate (module doc part (b)): `LockstepPhonologyProposer`, called
/// DIRECTLY (outside the composite's dedup, which is the exact mechanism that shadows it at the
/// composite level on both sides of the port identically), genuinely computes the "lazi" ->
/// `eLas+mrLoc:0` candidate on its own -- proving it is real, not a stub, independent of whether the
/// composite's fixed proposer order ever lets its line survive dedup.
#[test]
fn toy_lockstep_proposer_finds_the_candidate_in_isolation() {
    let g = load();
    let surface = SurfacePhonology::new(&g);
    let morpher = Morpher::new(&g, usize::MAX);
    let lockstep = LockstepPhonologyProposer::new(&g, &surface, &morpher, 1_000_000, 2, walk::DEFAULT_MAX_BEAM_WORK);
    assert!(lockstep.has_arcs(), "the toy rule's Pinv must have a real non-identity arc (quirk 1 must not reject it)");
    let candidates = lockstep.analyze_word(&g, "lazi");
    assert!(
        candidates.iter().any(|c| c.root_index == 0 && c.morphemes.len() == 2),
        "LockstepPhonologyProposer must find eLas+mrLoc directly on 'lazi', got {candidates:?}"
    );
}

/// Same isolation check for `ChainPhonologyProposer`.
#[test]
fn toy_chain_proposer_finds_the_candidate_in_isolation() {
    let g = load();
    let surface = SurfacePhonology::new(&g);
    let morpher = Morpher::new(&g, usize::MAX);
    let chain = ChainPhonologyProposer::new(&g, &surface, &morpher, 1_000_000, 2, walk::DEFAULT_MAX_BEAM_WORK);
    assert_eq!(chain.chain_length(), 1, "the toy grammar has exactly one phonological rule");
    let candidates = chain.analyze_word(&g, "lazi");
    assert!(
        candidates.iter().any(|c| c.root_index == 0 && c.morphemes.len() == 2),
        "ChainPhonologyProposer must find eLas+mrLoc directly on 'lazi', got {candidates:?}"
    );
}

/// Sanity check mirroring the toy grammar's own header: the bare walker (no phonology proposer at
/// all) must NOT find "lazi" -- it is genuinely unreachable without Composed/Lockstep/Chain, which
/// is the entire premise of this gate (per the grammar's own design rationale: no junction/redup
/// mechanism can subsume a root-internal, non-boundary-conditioned substitution).
#[test]
fn toy_bare_walker_cannot_see_lazi_at_all() {
    let g = load();
    let surface = SurfacePhonology::new(&g);
    let build_morpher = Morpher::new(&g, usize::MAX);
    let trie = Trie::build(&g, &surface, &build_morpher, 1_000_000, 2, true);
    let bare = walk::analyze_word(&g, &trie, "lazi", walk::DEFAULT_MAX_BEAM_WORK);
    assert!(bare.analyses.is_empty(), "bare walker must miss 'lazi' entirely, got {:?}", bare.analyses);
}

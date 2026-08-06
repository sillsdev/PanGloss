# `fwdata_conformance_gate.rs` notes

This gate imports a real FieldWorks `.fwdata` project through the new pipeline
(`pg_fwdata::import_file` → `Snapshot` → `pg_grammar::compile_project` → `Grammar`) and
independently loads the committed HC-XML oracle through the legacy pipeline (`pg_grammar::load`),
then runs every word in `samples/data/{sena,amharic}-words.txt` through a `Morpher` built from each
`Grammar` and compares results behaviorally.

## Why behavioral comparison, not ID comparison

IDs cannot be compared directly: the legacy XML export keys morphemes by session-scoped `Hvo`
integers, the new pipeline keys everything by FieldWorks GUID, so even the parity `signature()`
strings differ trivially between the two paths even when the underlying analysis is identical.
Instead, each analysis is reduced to something comparable across both compilers: the in-order
sequence of morpheme glosses paired with the analysis's surface-shape string
(`BehavioralAnalysis`). A word's full result is the multiset of these pairs — order across analyses
is not meaningful, order of morphemes within one analysis is.

## Self-skipping

Both the real FieldWorks project directory (`PANGLOSS_FW_PROJECTS_DIR`, falling back to a known
sibling checkout) and the committed oracle/word-list files under `samples/data/` are untracked
local corpora; either being absent makes the relevant test self-skip with a printed reason rather
than fail, mirroring `pg-grammar`'s `sample_path()` tests and `pg-fwdata/tests/real_projects.rs`.

## Why every test in this file is `#[ignore]`d

The step cap stays `usize::MAX`: a step cap truncates the analysis cascade non-deterministically
(see `pg-parse/tests/batch_determinism.rs`), which would surface as spurious cross-compiler
mismatches unrelated to either compiler. Uncapped analysis of the full corpora (7,121 Sena words /
673 Amharic words) through two grammars each is expensive, so the two full-corpus tests are
ignored on that basis. The third, fast 50-word smoke test is *also* unconditionally ignored on
different grounds: the default local test run must not depend on the gitignored `samples/data/*`
corpus (or a real FieldWorks checkout) at all, regardless of test speed. Run any of them with
`cargo test -p pg-cli --release -- --include-ignored`.

## The hang, and why the fix is a wall-clock timeout rather than a step cap

A handful of real corpus words (confirmed: at least one in Amharic's first 50) hit a genuine
combinatorial blowup in the unmemoized-equivalent search space and never terminate under an
uncapped step count. Confirmed via `fwdata_grammar_equivalence_gate.rs` (which needs no `Morpher`
at all) that the two compiled `Grammar`s are structurally identical, so this is not an importer
defect — the same word blows up identically regardless of which pipeline produced its grammar.
`Morpher::with_word_timeout` (a wall-clock deadline, see
`pg-parse/tests/word_timeout_pathological_gate.rs`) fixes the hang without the step cap's
non-determinism problem: `run_conformance` arms `WORD_TIMEOUT` on both morphers, and `compare_word`
treats either side timing out as `WordComparison::TimedOut`, reported separately like known oracle
drift, never counted as a match or a mismatch — a wall-clock deadline is inherently
non-deterministic across runs/machines, so the partial result at the moment it fires is not a
meaningful cross-pipeline comparison either way.

## Known oracle drift (Sena 3): a documented failure, not a tolerance

The committed `samples/data/sena-hc.xml` no longer corresponds byte-for-byte to the live
`Sena 3.fwdata`: three lexeme forms were edited in FLEx after the oracle was exported. Verified by
regenerating a fresh oracle with FieldWorks' own `GenerateHCConfig.exe` from the current `.fwdata`
and diffing the digit-stripped line multisets — the only content differences are `peno`→`penohoho`
(entry `2976cd0f`), `guman`→`guman.hello.world`, and `mpaka`→`mpaka.la.la` (test edits); everything
else is `Hvo` drift.

Each `KNOWN_ORACLE_DRIFT` entry is matched against the corpus by substring, not exact equality: a
root-form edit doesn't just break the bare root word, it breaks every corpus word derived from that
root by affixation too (confirmed against the full Sena corpus: 13 words like `agumana` /
`kugumana` / `gumanik` all fail to parse on the new pipeline because their surface form is built on
`gu[mn]a[mn]`-style patterns that no longer match the new pipeline's edited root, while legacy's
stale-but-internally-consistent root still parses them fine). All such words are expected to
mismatch against the committed oracle — the committed oracle is wrong for them, not the new
pipeline (the fresh oracle agrees with the new pipeline).

This is a per-root *aggregate* invariant, not a per-word one: plenty of corpus words merely contain
a drift root's substring incidentally without being derived from the affected lexeme at all
(confirmed against the full Sena corpus: `kugumanya`, `gumanika`, `madawipeno` all contain
`guman`/`peno` yet parse identically on both pipelines — healthy, unrelated words). Such a word
matching both pipelines is not a sign of anything wrong. Instead, each drift root is asserted to
still resolve to at least one mismatch somewhere in the corpus (so this list self-invalidates if the
oracle is ever regenerated), tolerating `WORD_TIMEOUT` noise: a root is only flagged stale if every
corpus word containing it plain-matched with zero timeouts and zero mismatches, never merely
because a thin root's one qualifying word happened to time out. Confirmed live drift is reported
separately, never counted as a conformance failure.

# `fwdata_grammar_equivalence_gate.rs` notes

The primary correctness gate for the `.fwdata` import pipeline going forward, superseding this
file's own behavioral gate for that purpose: an uncapped `Morpher` search hung indefinitely on real
Sena corpus input (see above), a genuine combinatorial blowup unrelated to either compiler's
correctness. This gate instead compares the two compiled `Grammar` structs directly for semantic
equivalence — no `Morpher`, no analysis, no parsing of any kind — which directly tests what the
import pipeline is supposed to guarantee, with none of the parser's own performance
characteristics in the loop.

## Why not compare `Grammar` structs field-by-field

The two pipelines key everything by incompatible id schemes: the legacy HC-XML export uses
session-scoped `Hvo` integers baked into the XML at export time, the new pipeline uses FieldWorks
GUIDs. A raw struct/index comparison would show wall-to-wall spurious diffs even for an identical
grammar. Every category is instead reduced to a canonical, id-free string before comparing (the
`GV` canonicalizer), keyed by content — grapheme, name, resolved feature/symbol names, structural
shape — never by id.

## Granularity

Two comparison strategies are used, deliberately. Most categories (phonemes, natural classes,
syntactic features, MPR names/groups, phon rules, templates, morphological rules) reduce to one
fully-resolved canonical string per item, compared as a sorted multiset (order across items is not
meaningful; internal structure of one item — an allomorph's LHS part order, a template's slot
order — is preserved, never sorted, since position there *is* semantics). Lexical entries need a
different treatment: a stable pairing key (gloss + owning stratum + syntactic FS + MPR set +
partial flag, deliberately excluding allomorph surface forms and any id) groups the two sides'
entries so a surface-form edit shows up as "one paired entry, forms differ" rather than "one entry
only in legacy + one unrelated entry only in new" (which a naive full-content key would produce,
defeating the known-oracle-drift allowlist). See `EntryKey`/`compare_entries`.

## Known, legitimate drift (Sena 3)

The same three lexeme edits described above (`peno`→`penohoho`, `guman`→`guman hello world`,
`mpaka`→`mpaka la la`, GUIDs confirmed directly against a fresh `pangloss import` of the live
`Sena 3.fwdata`) must surface here as exactly those three paired-entry value mismatches and nothing
else. `SENA_ENTRY_DRIFT` documents each by the new-pipeline entry's GUID; `compare_entries` resolves
each to its pairing key and asserts the pairing still mismatches — self-invalidating, exactly like
`KNOWN_ORACLE_DRIFT` above.

## Bounded scope

`model.rs`'s own module doc records that all three reference grammars contain zero
`RealizationalRule`, `StemName`, `Family`-with-entries, `MorphemeCoOccurrenceRule`/
`AllomorphCoOccurrenceRule`, and `FootFeatures`. This gate asserts both pipelines agree the count is
zero for each (`check_zero_construct`) rather than building bespoke canonicalizers for constructs
neither corpus exercises. `properties` (arbitrary XML `<Properties>` key/value pairs) is a known,
permanent, non-bug divergence — the legacy loader parses them from XML, the new pipeline never
populates them (every `properties:` push site in `compile/{lexicon,affixes}.rs` is `Vec::new()`) —
so it is deliberately excluded from every canonical signature, not compared.

## Test-timing policy

Same self-skipping convention as above. The default local test run must not depend on a real
FieldWorks project checkout or the gitignored `samples/data/*-hc.xml` fixtures at all, so both
tests here are unconditionally `#[ignore]`d regardless of speed; the self-skip guards keep
`--include-ignored` runs green when either is absent.

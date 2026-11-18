# FST verification evidence, version 1

`pangloss fst-verify <grammar> <words> [--out <tsv>] [--json <path>]` compares
the FST propose/peel/HC-confirm pipeline with plain Rust HermitCrab search.
It requires a build with `foma-tools`. This is a comparison of two Rust
pipelines, not a C# oracle replay or a linguistic correctness certificate.

Input grammar formats are the same as `batch`: HC XML, snapshot JSON,
FieldWorks `.fwdata` and `.fwbackup`. The UTF-8 word list has one word per
line. Leading/trailing whitespace and empty lines are ignored, as in batch;
repeated words remain separate occurrences. An empty list is an error.
Embedded tabs and carriage returns in a word are rejected before either output
file is created or overwritten. Newlines separate words; CRLF line endings are accepted.

## TSV

The TSV goes to stdout unless `--out` names a file. It has a header and one
flushed row per input occurrence, in input order:

| Column | Meaning |
| --- | --- |
| `word` | Input word |
| `verdict` | `equal`, `missing`, `extra`, or `incomparable` |
| `missing` | Compact JSON array of identities found only by plain HC |
| `extra` | Compact JSON array of identities found only by FST plus confirmation |

Identities follow `pangloss.machine-word-analysis/v1`: ordered stable morpheme
keys, root position and category. Comparison ignores order and duplicate
discovery counts. If both difference directions occur, `missing` takes
precedence; **both arrays remain populated**. Incomparable rows carry empty
difference arrays because partial analyses cannot establish a mismatch.

## JSON

`--json` writes a document with `schema: "pangloss.fst-verify"` and
`schema_version: 1`. `rows` contains every occurrence, its zero-based `index`,
the four TSV fields, `hc_status`, `fst_status`, and each engine's raw analysis
count (diagnostic only). `hc_analyses` and `fst_analyses` retain the full
identity multiplicities from the shared differential driver; on incomparable
rows these are partial diagnostic evidence. `totals` contains the disjoint verdict counts and
`words`; these counts sum to `words`. `words_with_missing` and
`words_with_extra` separately count both directions, including a row with both.
`fst_refused_grammar` is a grammar-level boolean, not a fifth word verdict.

`grammar_sha256` and `words_sha256` hash the original file bytes. `engines`
names the two pipelines, gives each engine's workspace version and embeds the
shared PanGloss build revision. `identity_profile` identifies the comparison projection.
`options` records the effective controls, including construction configuration
and the resolved runtime `peel_chain_depth_cap` (null when unconfigured);
`construction` preserves resolved bounds, selected strategy, attempts and typed
refusals from the compiler. `compile_refusal` is its human-readable error or null.
When construction refuses, all words are incomparable and HC is not run.

Construction attempts also expose `capability_causes`, the typed causes from the
existing deduplicated diagnostics, and `proposal_recall_occurrences`. The latter
is an array of `{strategy, cause, occurrences}` records, or null when the proposal
recall admission check was not evaluated (for example, factored dispatch or a
developer measurement bypass). An evaluated
check with no witnesses produces `[]`. Each cause uses an adjacently tagged
object such as `{"kind":"proposal_recall","details":"ambiguous_segment_spelling"}`;
the emission strategy uses its stable label, `"tuned-surface-probed"`. Other
capability causes, including `{"kind":"alpha_environment"}`, remain in
`capability_causes`; no occurrence count is inferred for them.

Occurrences count witness production in the owning proposal-recall check, before
deduplication. A table-closure failure contributes one grammar-wide witness;
template reentry likewise contributes one. Reduplication and truncation contribute
one per affected stratum; rewrite spelling contributes one per referenced rewrite
per stratum, even if both sides fail. Symbolic epenthesis and narrowing class
outputs contribute one per affected subrule reference. Repeated stratum references
are counted separately. These are not counts of rules, syntax nodes, analyses or
input words. The first-witness diagnostic list and its text remain unchanged.
These are additive fields in schema version 1; old construction reports decode
with an empty cause list and an unevaluated (null) occurrence field.

## Bounds and exit status

`--work-cap N|unbounded`, `--word-timeout-ms N`, `--word-mem-cap-mib N` and
`--capture-cap-mib N|unbounded` use batch's parsers and defaults. Both parsers
receive the same values. The FST confirmation groups share a single per-word
parser budget, including its confirmation reserve.
`--always-enforce-final-templates[=true|false]` applies to both; omission uses
the grammar setting. Guessing is off on both sides because the FST does not
propose guessed roots.

`--apply-path-cap N` and `--apply-candidate-cap N` independently contain FST
proposal work, each defaulting to 1,000,000. Construction accepts the shared
`--fst-strategy`, `--fst-auto-order`, `--fst-closure-work`,
`--fst-structural-work`, `--fst-structural-bytes`, `--fst-closure-depth`,
`--fst-derivation-depth`, and `--fst-compound-depth` options. No limit is raised
automatically. Caps, timeout, invalid shape, incomplete peeling or confirmation
make a word incomparable. The FST elapsed-time check includes proposal time;
proposal's native network traversal is not preemptible by the parser deadline,
so a timeout is cooperative rather than a hard process deadline.

A completed mismatch or refused construction exits nonzero after writing
evidence. Invalid arguments and I/O errors also exit nonzero. A run containing
only equal and incomparable words exits zero; callers must inspect the totals
before claiming full parity. A partial TSV left after an interrupted run is
progress evidence; a final JSON summary is written only after all rows finish.
Outputs may not alias either input or each other, including existing hard links.

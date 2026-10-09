# 071 evidence

All data is synthetic. No external project data or binaries are committed.

## Seven-word comparison

Counts are complete parse-identity multisets; zero means an empty multiset.
Every Rust row is `ok`, uncapped, without timeout, invalid shape or unavailable
analysis. Both C# revisions complete every word without skip or failure.

| Word | C# b9e7db44 | C# a873d60d | HC-Rust aa59f3d0 | Rust proposed port |
|---|---:|---:|---:|---:|
| kat | 0 | 0 | 1 | 0 |
| kad | 0 | 1 | 0 | 1 |
| kak | 0 | 0 | 0 | 0 |
| dat | 0 | 0 | 0 | 0 |
| tat | 0 | 0 | 0 | 0 |
| kta | 0 | 0 | 0 | 0 |
| ka | 0 | 0 | 0 | 0 |

Both C# revisions generate exactly `kad` from underlying `kat`. Hand trace:
`k a t`, with the final `t` after `V = {a}`, becomes `k a d`. There are no
head-feature transitions. Old Rust's accepted `kat` contradicts synthesis and
both C# versions; original C#'s lost `kad` is the shared inversion defect.

`kat-fixture-with-k.fwdata` is the actual CLI-test-generated file.
`kat-snapshot.json` comes from the existing PanGloss import owner. Their SHA256
hashes are `592D52E071ACFFEDD1DE522C2540FD5D667F6351BA0E8DFBDDF96090EC2524AB`
and `463025359EB99D8C0022B517311F87B53C4B2BFDE387E6025A65140A28DFDF99`.
The compiled grammar/rule files are owner-provided dumps.

`kat-grammar.xml` translates the reachable synthetic snapshot slice for C#;
it is not a claimed FieldWorks HCLoader export. It preserves three strata,
the noun/plural root, segment table/class and rewrite. Two default compound
rules are omitted: two nonempty three-segment roots yield at least six segments,
while all measured words have two or three and the rewrite preserves length.
Other roots are unsegmentable in this table. Rust XML runs match the actual
fwdata runs before and after. XML SHA256:
`1605BE8A0F9A00834AEFF60ED0992AA23B17149D7670729315784BDB64AD616A`.

`kat-csharp-*.tsv` and native `kat-hc-*.txt` come from exact source archives of
Machine `b9e7db4435c325494bdb2c68ec569cadeb10df23` and
`a873d60da6dfe1e3b613d474743613fd371a5170`, built outside the repo. The native
tool is unmodified. Supplemental `kat-observer.cs` calls each engine's own
`GenerateWords` and `ParseWord`, using official `SignatureFormat` from `25ddf914`.
Its build/run scripts are preserved. Offline restores and Release builds pass
with zero warnings/errors; no archived source was adapted.

`kat-rust-*-{fwdata,xml}.tsv` retain official CLI signatures/status rows.
`kat-rust-*-analyses.jsonl` retain form/MSA GUID projection. Original Rust admits
`kat` as `|k[atdk][atdk]`; the port admits only `kad` as `|kad`. XML uses the
MSA GUID as root tag while fwdata retains its existing empty lexical tag;
signatures are recorded separately. `kat-commands.md` records all managed runs.

## Six-word StrRep fixture

`xpuma-oracle-{before,after}.tsv` retain complete official adapter output;
`xpuma-oracle-*-selfcheck.log` retain the exact reviewed mismatch:

`1/6 word(s) mismatched: xpuma: expected [XMUMA|xpuma] got []`

Before uses the exact `b9e7db44` production rewrite spec; after uses `a873d60d`.
The original retains only the additive public string superset API, which the
original spec never calls. Surrounding engine HEAD is `721c6043` (its later
change only cleans up a pre-existing comment). Master has no batch/conformance
runner, so adapter/harness are from `25ddf914`. This is a measured compatibility
build, not an exact pinned-engine run or a full oracle-suite pass.

Ignored scratch glue supplies master's fourth trace argument and a reflection
adapter that throws if its unavailable failure-allomorph API is requested;
normal fixture self-check does not request it. Official `SignatureFormat`,
`Batch` and `Runner` comparison logic are unchanged. The scratch manifest omits
only unsupported `fieldworks_producible` metadata, leaving all expectations,
words and grammar unchanged. The default wrapper exits 25 because its Windows
executable is absent here. The targeted real adapter/self-check completes:
original self-check exits 1 and proposed exits 0. A missing cached historical
PCRE package blocked an exact pinned-engine build; no dependency was substituted.

Fixture `words.yaml` records five original-oracle words and one forward-derived
word, deliberately red against original C#. Both C# revisions synthesize
`xmuma` to `xpuma`; patched C# and Rust invert it.

## Regression checks

The final `/tmp/pangloss-lanes/strrep-unapply.md` lists exact gates, per-test
production-fix-removed checks and commit identities, distinguishing actual
checks from fixture presence and oracle compatibility limitations. The lead
publishes the proposed Machine PR and adds its link to ledger 071.

## Backend scoreboard inventory comparison

`scoreboard-before.log` and `scoreboard-after.log` retain the focused gate output with
one temporary `SCOREBOARD_CELL` line for every owner-measured cell and `--nocapture`.
Original `aa59f3d0` production code and the original inventory yield 73 fixtures / 219 cells,
exit 0. The port plus `strrep-rewrite-unapplication` yields 74 fixtures / 222 cells, exit 101
with the old ratchet deliberately retained. Both checks exit 0. The proposed run reaches
and passes the soundness assertion before failing the count assertion.

`scoreboard-{before,after}.tsv` retain each named state; `scoreboard-comparison.txt` records
**zero existing cell state changes**, zero removals and exactly three additions:
TunedSurfaceProbed / TemplatedUnderlyingTokens exact, PlanComposed refused, all for
`staging:edge-cases/strrep-rewrite-unapplication`. Counts move 69 to 70, 47 to 48 and 35 to 36,
respectively. The lead approved only these inventory updates after requiring this comparison.

`scoreboard-measure.ps1` preserves the temporary measurement/restoration script;
`scoreboard-compare.py` checks unique keys, both complete inventories, unchanged existing
states and the precise added-cell map. Their temporary paths assume this lane's `/tmp`
layout. The production test contains no diagnostic print. The comparison happened before
editing expectations. PlanComposed's refusal is safe while HC answers; broader coverage
is recorded as a coverage-ledger follow-up in 071.

Committed transcripts normalize trailing whitespace and final blank lines; output values are unchanged.

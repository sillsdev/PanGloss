# FST construction strategies

`TunedSurfaceProbed` is the only backend. Its construction axis is independent:

- `surface-enum` builds the existing surface-probed lexc construction.
- `factored` reserves the factored underlying morphology/shared permissive phonology
  construction. It returns typed `NotImplemented` and builds no network.
- `auto` tries the configured order, records each outcome and stops at the first
  complete construction. It cannot accept a partial artifact. The default order
  is `surface-enum,factored`; explicit selection never falls back.

Analyzer compilation and completed-build compilation share the same strategy
dispatcher and profiled construction. `compile_proposer_with_config` retains the
analyzer admission precheck. Completed-build measurement calls
`compile_proposer_for_measurement_with_config`, omitting that advisory precheck
so the independent envelope/compiler differential can detect both divergence
directions. Both reject partial emission and enforce the same construction bounds. The returned `CompileProfile`
contains `construction`: the requested configuration, effective bounds, ordered
attempts, refusal types/details, compile phase, closure evidence, state/arc counts,
elapsed milliseconds and selected construction. RSS is a process high-water mark
at attempt completion on Linux, including previous attempts; it is not a measured
allocation charge or an isolated per-attempt peak. Other platforms report null.
Completed-build wire evidence retains the same report and binds selection to the
original request. Worker protocol v10 rejects older peers so a worker that does not
understand construction selection cannot silently ignore it. Every artifact remains
a proposer confirmed by HermitCrab.

The configuration type is `pg_foma::construction::FstCompileConfig`. It controls
an optional extra logical closure-work cap, structural work (100,000 synthesis calls),
structural retained-output charge (16 MiB), closure depth (64), compound depth (200) and
standalone derivation depth (200, or the legacy `HC_DERIV_CHAIN_DEPTH_BUDGET`
setting when the explicit value is absent). The work cap defaults to null, preserving the analyzer's existing construction;
the separate characterization probe retains its 3,000-step cap. Promoting that
probe cap into construction caused two previously exact fixtures to refuse. Depth
bounds and the finite worker/process envelope remain in force; a numeric
`--fst-closure-work` adds a typed work refusal. The structural controls use #179's existing counters and canonical commuting-order collapse.
`--fst-structural-work` and `--fst-structural-bytes` override
`HC_STRUCTURAL_CLOSURE_WORK_LIMIT` and `HC_STRUCTURAL_CLOSURE_BYTES_LIMIT`, respectively;
absent explicit values preserve those environment overrides and defaults. The shared
resolver records effective values once per build and supplies the existing emitter
mechanism. The retained charge remains conservative (about four times retained size),
independent of RSS. Logical closure transitions and structural synthesis calls
(including reverse-order proofs) are distinct measures. The emitter owns the authored-rule depth calculation; construction
refuses when that depth exceeds the budget. Configuration errors and exhausted
limits are refusals, never truncated successful output. The external process envelope remains necessary.

Use the same options with `grammar_pass`, `fst-health`, or `grammar-health --fst`
(the health commands require the `foma-tools` feature):

```
--fst-strategy surface-enum|factored|auto
--fst-auto-order factored,surface-enum
--fst-closure-work N
--fst-structural-work N
--fst-structural-bytes N
--fst-closure-depth N
--fst-compound-depth N
--fst-derivation-depth N
```

`fst-health` remains characterization-only until a construction option is supplied.
`grammar-health --fst` includes the construction report alongside authoring checks.
A readiness finding does not bypass the selected construction attempt. This stage
preserves existing representability refusals, including alpha environments;
per-site permissive alpha lowering belongs to the next construction stage.

## Stage-0 differential

Build/run the `grammar_pass` example through `rust/tools/pg.ps1`:

```
PANGLOSS_EXTRA_ARGS='<grammar> <words.txt> <out.jsonl> --fst-strategy surface-enum' \
  pwsh -NoProfile -File rust/tools/pg.ps1 -Mode run -Package pg-foma \
  -Example grammar_pass
```

Use the shared Cargo lock and four jobs/threads on this workstation. Heavy grammar
attempts run one at a time with `ulimit -v 8388608` and `timeout 3600` as a hang
guard. Build first, then use `pg.ps1 -Mode run -Exe <built-example>` for measurements
so compile time is the grammar's construction time, excluding Rust compilation.

The JSONL preserves each filtered input occurrence's zero-based word ID, including
repeated spellings. The adjacent `.manifest.json` records source/word hashes,
source words, Machine pin, language and operational bounds, routing witness,
compile phase/profile and completed-row count. An FWData file has a source hash
and no invented XML hash. Rows distinguish raw paths, unique candidate projections,
proposal misses, spurious confirmed identities and confirmed identity multiplicities.
`raw_candidates` records raw FST apply paths across the direct and peel-root calls;
`unique_candidates` records the final propose/peel union. A peel can expand one root
path into several candidates, so the latter can exceed the former. Both counters
are retained without treating them as the same stage of deduplication.
Caps, invalid shapes, refusals, missing rows and interrupted phases remain incomplete;
a refused build does not have measured zero misses or measured perfect recall.
The HC reference has a 5-billion-step work bound, with no short wall timeout. Apply
traversal has one-million-path and one-million-candidate limits. Confirmation uses
the existing complete HC confirmation path with no word timeout. The external hang
guard is still required.

`strategy_scoreboard <out.jsonl>` uses the shared fixture discovery and YAML loader,
all words in each fixture, and one column set per construction. The existing backend
scoreboard remains the named regression ratchet. The new scoreboard additionally
reports full multiset parity. Fixtures marked `expect_crash` remain named exclusions;
other incomplete words stay in their grammar's denominator.

The C# adapter and identity comparison instructions are in
`rust/tools/fst-strategy-oracle/README.md`. The independent comparison is:

```
python3 rust/tools/fst_differential.py <out.jsonl> \
  --csharp <oracle.jsonl> --out <comparison.json>
```

It checks pin, source and word hashes before comparing. C#/HC-Rust differences,
missing FST proposals and confirmed-multiset differences remain separate evidence.
For FWData inputs this runner measures against plain HC-Rust, explicitly labelled;
it does not claim an unavailable founding-oracle replay.

`rust/tools/fst_strategy_run.sh <built-grammar-pass> <grammar> <words> <out.jsonl>
<strategy> [auto-order] [closure-work]` applies the serial attempt envelope through managed
`pg.ps1 -Mode run -Exe`. The real-grammar baseline uses an explicit additional
3,000-step work cap (the optional last argument), independently of the compile
API's default of no additional work cap. The manifest retains this choice. It records the actual exit status in the manifest after
the child exits. A hang-guard expiry is typed `machine_limit`; other abnormal exits
remain `process_failure` unless their cause is demonstrated. The Python summarizer
retains every missing word ID from the prewritten manifest after an interrupted run.
Raw Rust morph occurrences retain dense allomorph IDs alongside the immutable input
hash; the C# adapter additionally exports source XML allomorph keys. Confirmed parity
uses the existing ordered-morpheme/root/category identity, with full multiplicity.

A run whose every construction attempt is `NotImplemented` writes explicit
`not_run_unimplemented_construction` HC statuses and incomplete rows without
repeating the HC baseline. There is no candidate relation on which to measure
recall or confirmed parity. Surface-enum's separate baseline retains HC results,
including when its actual construction refuses.

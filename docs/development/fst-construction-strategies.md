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
mechanism. Depth bounds also resolve once: explicit values override environment settings.
`--fst-closure-depth` otherwise uses `HC_COMPOSE_CHAIN_DEPTH_BUDGET`, then 64;
`--fst-compound-depth` otherwise uses `HC_COMPOUND_CHAIN_DEPTH_BUDGET`, then
`HC_COMPOSE_CHAIN_DEPTH_BUDGET`, then 200. Requested bounds remain null when unset;
the build report records their effective numeric values. Production evidence collection
preserves `HC_PREEXPAND_FLAT` and the measurement-only `HC_PREEXPAND_PROBE_CAP`.
The retained charge remains conservative (about four times retained size),
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

## Permissive phonology relation

`replace::compile_and_compose_rules_recall_safe` returns a complete ordered rewrite
relation or `PhonologyLoweringError` naming the unsupported rule. Empty input is
identity. A failed cascade returns no partial net and appends no partial tuple
reports. The unused cascade variants were removed; both lab cascade consumers use
this typed proposing API.

Every matching site independently rewrites or retains its material. Alpha
agreement is deferred to HC, and output classes include compatible feature
combinations. All subrules share one replacement batch. Each obligatory site
chooses between the replacement material and unchanged material; foma's native
`(->)` loses ordinary expansions such as `a -> ai`. For simultaneous epenthesis, an unchanged
site emits a temporary symbol, transparent to contexts and erased from the output
after the batch. This avoids an empty-to-empty insertion center, which foma cannot
represent with an epsilon alternative. Direct tests cover expansions before and
after the target, multi-segment replacements, contractions, deletion and insertion, in both directions and modes; unproven
overlapping targets and iterative empty focus assert typed refusals. Iterative contexts use the
processed side. RTL batches mirror all operands and swap the environments, compile
with LTR processed contexts, then reverse both tapes of the complete relation.
This preserves RTL feeding while avoiding foma's lossy `\\` construction with
unchanged-material alternatives. Simultaneous contexts use the input (`||`).
Rule batches compose in stratum order for cascade feeding. Word
anchors and literal morpheme boundary targets retain their token/position semantics.
Boundary ignore loops consume and copy optional markers across pattern matches;
they introduce no output-only boundary loop.
POS/MPR applicability and feature defaults are confirmed by HC.

Iterative empty focus returns `IterativeFedInsertionUnavailable` for every
direction and environment. One insertion per original gap cannot model fed
insertion into the same gap (for example `∅ -> a / # a? _ e`: `e -> ae -> aae`).
No nonfeeding exception is claimed. Simultaneous insertion tests each original
input gap and remains supported.

Quantified/grouped/anchored focus or replacement and other unsupported shapes
return a typed refusal. Focus spanning multiple tokens returns
`SimultaneousMultiTokenFocusUnavailable` or `IterativeMultiTokenFocusUnavailable`:
identity alternatives can consume overlapping matches, so inclusion is unproven.
These decisions use the actual lowered focus in the proposing owner and change
neither characterization nor production surface enumeration. Any refusal rejects
the entire cascade, leaving tuple reports untouched.
The dedicated metathesis characterization compiler remains available, but the
proposing cascade explicitly refuses metathesis: it has no per-site permissive
construction. It cannot silently skip that rule. No partial-stem capability or
new memoization is added.

This relation is the phonology component for subsequent morphology composition.
`surface-enum` still uses its existing HC surface probing and alpha refusal;
`factored` remains unimplemented. Direct relation coverage does not claim that
those production constructions now compile additional grammars. The same public
stage-0 differential measures that separately.

### RTL inclusion by reversal

Let `rev` reverse token sequences. Mirroring a branch
`L -> R / E _ F` gives `rev(L) -> rev(R) / rev(F) _ rev(E)`.
This includes the unchanged alternative: `rev(R | L) = rev(R) | rev(L)`.
An original processed right suffix is exactly a processed left prefix in the
mirrored traversal. Thus each right-to-left site choice corresponds to the same
left-to-right choice on the reversed word, including expansion, deletion and
unchanged material. Reversing both tapes of the compiled relation takes
`rev(input) -> rev(output)` back to `input -> output`.

The transformation covers the whole shared subrule batch; it does not compose
subrules separately or reverse the order of rules in a cascade. Environment
concatenations and quantified children reverse recursively; word anchors swap
sides, and alpha constraints retain their feature identities and polarity.
Insertion skip markers are erased on the output before reversal. Boundary copy
loops are added after returning to the original orientation.

This is a reversal of the relation, rather than a switch to input contexts or to
LTR traversal on the original operands. Independent forward expectations test
literal, anchored and alpha environments on both sides for every nonempty operand
row, asserting typed refusal for multi-token focus in either mode instead of treating
those rows as completed relations. Expansion feeding and bleeding have directional negative controls; unsupported
successors still fail atomically, leaving neither a partial cascade nor reports.

### Intermediate bundles and the inclusion contract

One `lower::IntermediateBundles` inventory lives for one complete cascade. It
starts with the owning table's attested segment bundles, retaining their original
token IDs. In rule order it closes under RHS feature updates to a fixpoint per
rule. `pg_rules::rewrite::{node_pins,node_full_lanes}` supplies the shared priority
union constraints and fresh-node constraints; this owner does not infer them from
diagnostics or duplicate the matcher's admission decision. Target, environment,
POS and MPR restrictions are ignored during closure, so the inventory is a
conservative superset of reachable material. Alpha output lanes enumerate every
subset of their feature mask, including negations and zero. Fresh-node rows also
conservatively cover unspecified/default/full-valued material. No existing
construction limit is increased.

Previously unattested rows get private PUA tokens after the table's IDs. Pattern
lowering consults these rows directly for targets, material and environments;
it never projects a row back to a spelled phoneme between rules. All compatible
material tokens remain alternatives at each site. Only after composing the last
rule does a spelling relation map retained rows to compatible attested tokens.
Private tokens cannot escape its output. Unspellable rows have no spelling arc;
they are not deleted into a spurious surface. Morpheme boundaries may be retained
for further token composition or erased for surface spelling.

**Resource containment:** at most 4,096 retained rows and a 1 MiB conservative
charge (four times lane storage plus row headers). Alpha expansion work vectors
obey the same count/charge bounds. The existing BMP PUA token domain must also
fit. Exceeding any bound returns `IntermediateBundleClosureUnavailable(rule)`;
there is no successful partial relation or report, override, cache, or raised
production threshold. These bounds contain the closure, not total FST build RSS.
A multi-table grammar returns `MultipleCascadeTablesUnavailable`, even for one
selected rule: the API cannot disambiguate a rule reused by different strata.
An empty cascade remains identity. This is a representability scope refusal,
not a new production admission policy.

**Inclusion argument.** Represent a concrete HC segment by an equal retained row
or a less constrained row containing all its feature values. Attested input has
an equal row. Updating a represented row with RHS pins is closed in the inventory;
for fresh material, the fresh-node row is included. Full/default/absent lanes may
widen the representation, and alpha lanes include every possible binding. Thus
feature-only material can retain a jointly back-and-rounded intermediate even
when no phoneme spells it. Every valid target/environment match is in the
compatible token set: non-alpha constraints use the shared pins, alpha lanes are
independent, and membership/identity correlations are conservatively widened.
Zero-bit intermediate lanes are widened rather than used to claim exact matching.
Spelling by compatibility includes every HC-spellable output of a represented row.
The resulting extra proposals require HC confirmation.

For each supported HC traversal, choose the actual branch/material at changed
sites and the original token at skipped sites. An iterative focus consumes exactly
one token; a simultaneous focus consumes zero or one token,
so a skipped widened match cannot consume a later overlapping segment target.
Obligatory replacement with explicit material-or-unchanged alternatives includes
these choices; native `(->)` is never used. The parallel subrule batch includes
all authored subrule alternatives. Iterative LTR contexts see the processed left
side; RTL is the conjugate relation under whole-batch reversal, as proved above.
Simultaneous contexts see input on both sides. This covers expansion before/after
a target and arbitrary finite multi-token output, deletion, and simultaneous
epenthesis at each original input gap with its transparent, output-erased skip
marker. Iterative empty focus is excluded: insertions can feed a new site within
an original gap, outside this construction's inclusion argument. Composition in
stratum order preserves the represented intermediate at every cascade step.

Environment concatenation, literal and natural-class atoms, alpha widening,
bounded/unbounded repetition and word anchors preserve inclusion compositionally.
An empty class renders the empty language (`[0 - 0]`), never epsilon. Literal
morpheme boundary targets remain available, and optional boundary copy loops let
matches cross HC's optional boundary markers without creating output-only loops.
Defaults and POS/MPR restrictions only remove choices at HC confirmation.

This argument covers both modes and directions within the existing shared shape
admission decision. It excludes iterative empty focus, multi-token focus, quantified/grouped/anchored
focus or material, metathesis, and multi-table grammars: each is typed
refused, never silently skipped. `every_pattern_kind_is_included_or_refused_at_its_actual_compile_seam`
exhaustively classifies the five `PatternNode` variants in each position and
both modes/directions against the real compiler. Dedicated tests cover the other
refusals, closure exhaustion, empty cascades, feature-only intermediates,
spelling and unchanged report state. The finite operand matrices are regression
evidence for this construction contract, not an independent universal proof.

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

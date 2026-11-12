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
A readiness finding does not bypass the selected construction attempt. The
production constructions keep their representability refusals, including alpha
environments. The permissive phonology relation below lowers alpha environments and
outputs per site on its own; no production construction selects it.

## Permissive phonology relation

`replace::compile_and_compose_rules_recall_safe` returns a complete ordered rewrite
relation or `PhonologyLoweringError` naming the unsupported rule. Empty input is
identity. A failed cascade returns no partial net and appends no partial tuple
reports. The unused cascade variants were removed; both lab cascade consumers use
this typed proposing API.

Every matching site independently rewrites or retains its material. Alpha
agreement is deferred to HC; each alpha output is one any-value lane (see the
inclusion contract below). All subrules share one replacement batch. Each obligatory site
chooses between the replacement material and unchanged material; foma's native
`(->)` loses ordinary expansions such as `a -> ai`. For simultaneous epenthesis, an unchanged
site emits a temporary symbol, transparent to contexts and erased from the output
after the batch. This avoids an empty-to-empty insertion center, which foma cannot
represent with an epsilon alternative. Direct tests cover expansions before and
after the target, multi-segment replacements, contractions, deletion and insertion, in both directions and modes; iterative
empty focus asserts a typed refusal. Iterative multi-token foci and the overlapping
one-token simultaneous foci take the block relation below. Iterative contexts use the
processed side. RTL batches mirror all operands and swap the environments, compile
with LTR processed contexts, then reverse both tapes of the complete relation.
This preserves RTL feeding while avoiding foma's lossy `\\` construction with
unchanged-material alternatives. Simultaneous contexts use the input (`||`).
Rule batches compose in stratum order for cascade feeding. Word
anchors and literal morpheme boundary targets retain their token/position semantics.
Boundary ignore loops consume and copy optional markers across pattern matches;
they introduce no output-only boundary loop.
POS/MPR applicability and feature defaults are confirmed by HC.

Iterative empty focus compiles as a widened relation. Synthesis inserts after a gap's node and
continues from the node it inserted, so one gap can take several insertions, each landing inside
the block of the first (for example `∅ -> a / # a? _ e`: `e -> ae -> aae`). The net places a
marked block at any gap, built from the first node of the replacement with any replacement row
allowed after every node, then keeps the block only where its subrule's environment holds at the
block's first node: the left environment on the output so far, the right one on the input with
every other block skipped. Later insertions in the block carry no environment, a superset of what
synthesis makes. A right-to-left rule places the replacement reversed, as synthesis does. Rules
whose replacement is not a plain row union return `IterativeFedInsertionUnavailable`. Simultaneous
insertion tests each original input gap and remains supported.

Quantified/grouped/anchored focus or replacement and other unsupported shapes
return a typed refusal. A simultaneous focus of several tokens returns
`SimultaneousMultiTokenFocusUnavailable`, and an iterative focus of several tokens
that is not made of plain characters and classes returns
`IterativeMultiTokenFocusUnavailable`: identity alternatives can consume overlapping
matches, so inclusion is unproven.
These decisions use the actual lowered focus in the proposing owner and change
neither characterization nor production surface enumeration. Any refusal rejects
the entire cascade, leaving tuple reports untouched.
The dedicated metathesis characterization compiler remains available, but the
proposing cascade explicitly refuses metathesis: it has no per-site permissive
construction. It cannot silently skip that rule. No partial-stem capability or
new memoization is added.

This relation is the phonology component for subsequent morphology composition (see
“Rule admission”). `surface-enum` still uses its existing HC surface probing and alpha refusal;
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

[ADR 0013: FST complexity budget](../adr/0013-fst-complexity-budget.md) records the
design: one conservative any-value representation per alpha output, bounded widening
and minimization during composition, a sound first-pass candidate filter and identity
merge, and per-word fallback governed by HC parser steps. It defines each stage's
input size and bound and the corpus “% less work” metric. The cascade implements the
any-value alpha lane and minimization after each composition; the identity merge and
the per-word fallback are not part of it.

One `lower::IntermediateBundles` inventory lives for one complete cascade. It
starts with the owning table's attested segment bundles, retaining their original
token IDs. Affix `Modify` and `InsertContext` outputs, in affix and compounding
rules, can create bundles before any phonology runs, so the closure is seeded with the
rows they can create, closed under repeated modification. In rule order it then
closes under RHS feature updates to a fixpoint per rule.
`pg_rules::rewrite::{node_pins,node_full_lanes}` supplies the shared priority
union constraints and fresh-node constraints; this owner does not infer them from
diagnostics or duplicate the matcher's admission decision. Target, environment,
POS and MPR restrictions are ignored during closure, so the inventory is a
conservative superset of reachable material. An alpha output gets one any-value
lane: the lane is zero, which matching treats as unconstrained, so one row covers
every binding, negation and absent value, and the row count does not depend on the
width of the feature mask
(`several_alpha_output_lanes_do_not_multiply_private_rows`). Fresh-node rows also
conservatively cover unspecified/default/full-valued material. Rules with a block
relation or a one-segment pair net intern the rows their focus tuples produce
instead (see below).

Unattested rows get private PUA tokens after the table's IDs. Pattern
lowering consults these rows directly for targets, material and environments;
it never projects a row back to a spelled phoneme between rules. All compatible
material tokens remain alternatives at each site. Only after the last
rule does a spelling relation map retained rows to compatible attested tokens.
Private tokens cannot escape its output. A row that no attested segment is
compatible with spells as nothing, as in HC's final surface test. In a grammar
without phonological features a row spells only as itself. Morpheme boundaries may
be retained for further token composition or erased for surface spelling.

**Resource containment:** at most 65,536 rows and a 100 MiB conservative
charge (four times lane storage plus row headers). Retained rows must also fit the
existing BMP PUA token domain (at most 6,400 table IDs and private tokens combined).
Exceeding any bound returns `IntermediateBundleClosureUnavailable(rule)`; there is
no successful partial relation or report, override, or cache. These bounds contain
the closure, not total FST build RSS.
A multi-table grammar returns `MultipleCascadeTablesUnavailable`, even for one
selected rule: the API cannot disambiguate a rule reused by different strata.
An empty cascade remains identity. This is a representability scope refusal,
not a new production admission policy.

**Inclusion argument.** Represent a concrete HC segment by an equal retained row
or a less constrained row containing all its feature values. Attested input has
an equal row. Updating a represented row with RHS pins is closed in the inventory;
for fresh material, the fresh-node row is included. Full/default/absent lanes may
widen the representation, and the any-value alpha lane includes every possible
binding. Thus
feature-only material can retain a jointly back-and-rounded intermediate even
when no phoneme spells it. Every valid target/environment match is in the
compatible token set: non-alpha constraints use the shared pins, alpha lanes are
independent, and membership/identity correlations are conservatively widened.
Zero-bit intermediate lanes are widened rather than used to claim exact matching.
Spelling by compatibility includes every HC-spellable output of a represented row.
The resulting extra proposals require HC confirmation.

For each supported HC traversal, choose the actual branch/material at changed
sites and the original token at skipped sites. In the per-site construction an
iterative focus consumes exactly one token; a simultaneous focus consumes zero or one
token, so a skipped widened match cannot consume a later overlapping segment target.
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
admission decision, plus the block relation for the foci below. It excludes iterative
empty focus, a simultaneous focus of several tokens, a multi-token focus of
anything but plain characters and classes, quantified/grouped/anchored
focus or material, metathesis, and multi-table grammars: each is typed
refused, never silently skipped. `every_pattern_kind_is_included_or_refused_at_its_actual_compile_seam`
exhaustively classifies the five `PatternNode` variants in each position and
both modes/directions against the real compiler. Dedicated tests cover the other
refusals, closure exhaustion, empty cascades, feature-only intermediates,
spelling and unchanged report state. The finite operand matrices are regression
evidence for this construction contract, not an independent universal proof.

### Block relation

An iterative focus of several tokens, and a one-token simultaneous focus the shape
check rejects because matches can overlap, lower to `[ ? | block ]*` instead of the
per-site construction. Identity passes at every token, and each block rewrites one
focus occurrence; boundary tokens between focus nodes are transparent. The
relation ignores environments and applicability gates, so a block may apply wherever
its focus matches, and any occurrence may also pass unchanged. A focus qualifies when
every focus and replacement node is a plain character or class and no target is
quantified.

A block's output comes from HC's own synthesis
(`pg_rules::rewrite::focus_outputs`, which rewrites a focus tuple without its
environment or gate), run once per focus tuple of retained rows. The tuple walk
interns each output row, so the closure is seeded with rows the rule creates,
and repeats until no row is new. It enumerates tuples only while the product of the
focus slot sizes is at most 65,536. Past that bound, and in a grammar without
phonological features, positions are independent and alpha outputs are any value:
the rule widens instead of refusing. When focus and output
have equal length the block rewrites in place; otherwise the first replacement node
rides on the last target's arc and the others follow it. Block inclusion is tested
against `pg_rules::rewrite::synthesize` on every string up to length four, for
left-to-right rules.

### Focus rows and synthesis outputs

A Site rule net relates a focus row only to itself or to what synthesis makes of it.
Foma's class-to-class replacement relates every focus row to every output row, so
the net is intersected with identity plus the focus-to-output pairs. With
phonological features the pairs are the exact synthesis outputs of each focus row,
from the tuple walk above. In a grammar without phonological features a literal or
segment-list node stands for the rows sharing its spelling, because synthesis
matches a featureless segment by spelling, and the pairs are the focus spellings
crossed with the replacement spellings. The pairs are known for a one-segment focus
whose subrules each have at most one replacement node; other Site rules keep the
class-to-class relation.

### Cascade parts

`CascadeParts::build` returns the cascade before composition: each rule's
minimized net (`RulePart`, with its construction `Site` or `Block`), the spelling
relation, the token inventory (`UpperTokens`: attested segments, private segments,
boundaries) and the tuple reports. `CascadeParts::compose` folds the nets from the
spelling back through the last rule to the first, minimizing after each composition,
so every intermediate relation's output stays on the attested alphabet. Build and
compose run on a thread with a 1 GiB stack, because foma drops deep chains
recursively. `compile_proposing_cascade` is `build` followed by `compose`; rule
admission uses the parts directly.

## Rule admission

`pg_foma::admission::build_rule_admission(opts, grammar)` builds a transducer from a
surface word to the affix rules that could take part in an analysis of it. It is a
library API exercised by tests and by the examples `admission_measure` and
`admission_conformance`; `pangloss batch --prefilter auto` is the only production caller
(see **The gate**).

**Contract.** The admitted set is a superset of the affix rules used by any analysis
HC returns for the word. HC then confirms with one restricted parse
(`Morpher::parse_word_selected`): every stratum and template stays on, a morphological
rule runs only if it is admitted, and no lexicon filter applies. The analyses and
their multiplicity are the restricted parse's result; over-admission costs HC work,
never an analysis. The examples compare the restricted and plain parses word by word.

**Free rules** are always admitted: compounding and realizational rules, an affix rule
with a subrule that leaves no usable literal, a rule whose piece does not compile
(a warning names it), a rule whose table accepts every text, and a rule whose table
passes the state bound (see **Net**).

**Plan.** `plan_admission` renders one piece per affix subrule: a regular expression
over tokens for its best literal. It then loosens the piece wherever a later rule
application could damage the literal, by testing whether the damaging item can reach
the literal's position counted in segments from both stem edges. Cuts insert material
inside it, zones delete or modify its segments, and part edges drop its boundary
tokens. Loosening makes a token optional, a boundary optional, or a gap material.
The deletion capacity of the whole grammar is counted per stem side.

**Net.** Each piece is `tokens* piece tokens*` crossed with the rule's tag, one small
machine per rule, joined as a union on the tag arc. The tag net is then folded through
each rule net in cascade order, the spelling and the orthography (every NFD
representation of each segment token, with a combining mark after a segment read as
foma's apply merges them), minimizing and recording evidence after each. The alphabet
is the cascade parts' token inventory, so private bundle tokens are in it.

**Tables.** The final net is not applied per word. `build_rule_admission` sorts its arcs
and reads out one deterministic acceptor per rule tag (`admission/acceptor.rs`): the
lower side of the paths whose upper side is exactly that tag, determinized by subset
construction, minimized, with the symbols compressed into classes. A tag with no path
gets no table and its rule is never admitted. A tag whose language is every text frees
its rule. A rule whose determinization passes `RULE_STATE_BOUND` (20,000 determinized
states) is also freed, with a warning in the shape of the Budget paragraph whose action
is "the rule is always admitted", and its item is listed in `evidence().tables.bounded`.
`admit(word)` splits the NFD word into net symbols the way foma's apply does (longest
symbol first, a combining mark after a symbol making it the any-symbol token), starts
from the free rules, and sets each tabled rule by one pass of its table over the tokens;
it returns the admitted flag per rule and never fails. `evidence().tables` holds, for
the tabled rules, their count, the counts that are `always`, `never` and `bounded`, the
largest and the summed table size (states, arcs, bytes) and the build time.
`admit_with_foma(word, path_allowance)` applies the whole net upward instead and is
compiled only for tests and the `developer-tools` feature; it returns `None` past the
allowance. `admission_conformance` compares `admit` with `admit_with_foma` on each
fixture word and on edits of it (a deleted, replaced or marked character, a prefix and a
suffix), and reports any difference as `table-differences`.

**Strata.** Strata up to and including the one with phonological rules are inner;
later strata are outer. Outer rules must leave their input intact (affix rules that
copy each part once, in order, and only insert segments; compounding that copies both
members whole). Outer pieces are exact literals, boundaries optional, folded through
the spelling and orthography only; with outer rules the inner net is padded with
text on its lower side and unioned with the outer net.

**Refusals.** `AdmissionRefusal` says why no net exists; the caller then parses
plainly, and no partial net is returned. `reason()` gives a stable string.

| Variant | Meaning |
|---|---|
| `CharacterTables(n)` | The grammar has more than one character table. |
| `StratumLayout(why)` | More than one stratum has phonological rules, or an outer rule edits its input. |
| `CompoundingEditsStem(rule)` | A compounding rule cuts or deletes stem material. |
| `Cascade(error)` | The proposing cascade refuses a phonological rule. |
| `Construction(why)` | A structure could not be built or measured. |
| `NoUsablePiece(n)` | No affix rule has a usable piece, so every rule would be admitted. |
| `Containment { structure, size }` | A structure exceeds the containment bound below. |

**Budget.** Each structure (every rule's relation, the tagged acceptors, each folded
net before and after minimizing, and the sum of the table sizes) is measured in states
plus arcs. Above 1,000,000 the build records a warning in the shape of
[ADR 0013](../adr/0013-fst-complexity-budget.md#warn-on-representation-size-and-preserve-the-backstop):
the structure, its size and bound, the grammar items driving it, the sentence “This is
a sign that there may be a better representation, such as widening, factoring or
filtering.”, and the action taken (the build continued, or the rule is admitted for every
word). Above 50,000,000 the build
returns `Containment`. `AdmissionEvidence` holds the sizes of every structure.

### Tiled acceptor

`pg_foma::admission::build_tiled(opts, grammar, &admission)` builds one acceptor per
grammar for the surface words that affix pieces around a lexical root can spell: the
language of `outer-left* cascade(inner-left* root inner-right*) outer-right*`,
projected to the surface side and held as a table. The pieces are the edge affixes
(prefix, suffix, circumfix, null), whole-stem reduplication and compounding as
`(left* root right*)+`, and edits modelled as windows at a stem edge (one unbounded
part copied once). `Tiled::rejects(word, admitted, options)` says the word has no
analysis. Like the filter, it is a library API exercised by tests and by the examples
`admission_measure --tiled` and `admission_conformance --tiled`, and used by the gate
below.

**Contract.** A rejection is sound only when all of these hold: the parse does not
guess roots (`ParseOptions::guess_root` is false); the lexicon is the grammar's own,
with no supplied-root overlay; `admitted` comes from the `RuleAdmission` the acceptor
was built with; and no rule outside the model (infix, partial reduplication,
truncation of stem material, modification of stem segments) is admitted for the word.
`Tiled::applies` checks the first, third and fourth conditions; the caller owns the
second. A rejected word skips the restricted confirm parse.

The acceptor needs a rule admission to build; where admission refuses (`NoUsablePiece`),
there is no acceptor.

**Inactive.** Nothing is rejected, and `inactive_reason()` names the cause, when a rule
outside the model is admitted for every word, the rule admission was built for another
grammar, the cascade or a construction step refuses, the build passes a size bound
(4,000,000 states plus arcs of any intermediate, 2,000,000 table states), or the
language is empty or accepts every text. When edit zones do not settle within 8 rounds
they widen to any text, which keeps the acceptor sound and makes it looser.

### The gate: when `pangloss batch` uses them

`pangloss batch --prefilter auto` lets `pg_foma::admission::gate::decide` choose, for one
batch, whether the rule admission and the tiled acceptor run. The default is `--prefilter off`,
which is today's plain parse, byte for byte. `auto` needs a build with the `foma-tools` feature
and is refused with `--stats`, `--word-timeout-ms` and the `HC_*` per-word diagnostics, whose
output is that of the full plain search. `--work-columns` and `--cost-column` are allowed: the
columns give the work the selected pipeline spent on the word (the restricted parse, plus the
guess-only parse where one ran; the tiled and admission walks are not in them), and the first
64 + K sampled words, whose outcomes are the gate's own plain parses, give the plain work. The decision is
made in parser work, never in time, and prints one `PREFILTER` line when it is made and one
`PREFILTER-TOTAL` line at the end of the batch on stderr; the batch TSV does not change.

The tables are built on every batch start, never cached or read from disk, and the build is charged:
its construction units are counted while it runs and reported at their weight. The weights are fitted
to measured instructions, not exact: the admission estimate is 0.77 to 2.1 times the measured
instructions across nine grammars (instructions per unit 526 to 1,426 against the weight 1,100), the
tiled estimate 0.77 to 3.3 times across the eight it applies to (224 to 973 against 750). A build
whose budget is under a floor
computed from the plan (admission) or the root lexicon (tiled) is refused before any net is built and
spends nothing; the floor is 12 to 30 times below the full build on most grammars, so it catches only
hopeless budgets. Every foma operation of the build, the cascade included, asks the meter first, and a
regular expression is charged by an estimate before foma parses it. A build that would pass its budget
stops at the operation that does not fit and the units it had spent are reported as construction cost.
That is a counted loss, but not a tight one: measured with whole-process instruction counts, a refused
build stopped by its budget cost up to about twice budget x weight (0.18 to 2.1 on five grammars at 5% and
25% budgets), and the unit overshoot is bounded by the largest single foma operation, which is up to
millions of units on the largest grammars (a Kleene-plus minimization in the tiled build). Amharic at a
25% budget is the measured case that exceeds 1.5 x (budget + largest operation): the cascade there costs
more than its regex estimate charges.

`--prefilter on` is the verification and measurement mode of the same pipeline (`decide_forced`): no warm-up,
calibration, budget (`UNLIMITED_UNITS`) or minimum batch length, so it pays the whole construction whatever the
batch length and nothing is parsed to decide. Admission runs on every word unless its build returns a typed
refusal; the tiled stage runs on every word unless it is not valid (no prospect, or an inactive build, with the
reason `auto` reports). A supplied-root overlay or a guess-only parse leaves both stages off with that reason,
because `on` never overrides a soundness precondition; root guessing with lexical patterns is handled as under
`auto`. It prints the same `PREFILTER` line (`warmup=0`, `calibration=0`, `net` the negated construction cost) and
`PREFILTER-TOTAL`, has the same refusals and the same `foma-tools` requirement, turns the meter on, and writes the
words in batch order; the TSV stays version 5.

For a batch of N words the gate runs, before the parallel phase:

1. Preconditions, each ending in off with its reason: fewer than 512 words (`MIN_BATCH`, 8 x 64: the
   calibration below runs K restricted parses of N words, K = N / 64, so it stays near 1/64 of the batch), a guess-only parse, a non-empty supplied-root overlay
   (`Morpher::has_supplied_roots`: the admission's root floor and the tiled lexicon assume the
   grammar's own roots). Root guessing does not turn the gate off; see "Root guessing" below.
2. Sample: word lists are ordered (by form, length or frequency), so the gate never takes the head of
   the batch. It takes 96 evenly spaced distinct indices `floor(m * N / 96)`, m = 0..95, from N alone
   (the decision line says `sample=even`; with `--start` the sample is over the words after the resume
   index). The indices with m % 3 == 1 (32 of them) are calibration candidates; the other 64 are the
   warm-up. Warm-up words are parsed plainly, which are their production parses. They give the mean
   plain cost, and so the projected remaining plain cost R: that mean times the number of words not in
   the warm-up.
3. The admission build (`build_rule_admission_within`), under a budget of R / 64 parser work
   (`ADMISSION_BUILD_SHARE`), that is R / 64 / `ADMISSION_UNIT_WEIGHT` construction units. A build that cannot
   fit spends its whole budget and nothing can be learned earlier (the cost before the first table is 70% to
   97% of the build, measured on nine grammars), so the share is the nominal bound on a refused build,
   1.6% of the batch (the measured overshoot is below). A typed refusal, or a build over budget, is off
   with the reason, and the units it spent are in `admission_units` and in `construction_cost`.
4. Calibration on K = clamp(N / 64, 8, 32) of the candidates (`calibration_words`; K_MIN, K_MAX and
   `CALIBRATION_DIVISOR` in `gate.rs`), spread evenly over the 32 candidates and taken in index order, each parsed plainly and restricted. The word's result is
   the plain one. S is the plain cost minus the restricted cost, F the filter's walk cost, both
   summed. The gate stops with on once S >= 4 F from the eighth word; at the Kth word it is on
   if S > F. A restricted parse that differs from the plain parse of a completed word (or, for a
   word the plain parse could not finish, lacks one of its analyses) is a hard error that names the
   word: it is a soundness bug, not a gate outcome.
5. The tiled stage, only if admission is on. First the free prospect (`tiled_prospect`): if it cannot
   be active, or no sample word without an analysis has `prospect.applies(...)`, the stage is off with
   the reason and nothing is built. Otherwise the potential is the estimated restricted cost of those
   sample words, projected from the sample to the rest of the batch, and the build budget is
   min(R / 16, potential / 2) parser work (`TILED_BUILD_SHARE`, `TILED_POTENTIAL_SHARE`; already bounded by half the saving the sample predicts), divided by `TILED_UNIT_WEIGHT` units. A refused or over-budget
   build is off with the reason and its units in `tiled_units`. The stage is on if the estimated
   restricted cost of the sample words it rejects exceeds the walk cost over the sample.
6. The rest of the batch (every index whose outcome the gate did not keep, candidates the early stop
   did not reach included) runs with the decided stages, and the outcomes are written in batch order. A word the tiled stage rejects keeps no
   analysis; any other word gets one restricted parse under the same work cap, and a capped
   restricted parse is the word's outcome.

**Root guessing.** The report line says `guess=none` (guessing is off), `guess=no-patterns` or
`guess=orchestrated`. A grammar with no lexical pattern cannot guess anything
(`Morpher::lexical_patterns()` is empty), so guessing counts as off and every stage runs with
guessing off in its options. With patterns, a word goes through the stages with guessing off: the
tiled stage under the non-guessing options, then the restricted parse. A restricted parse that is
complete and empty, or a tiled rejection, is a confirmed lexical miss and is followed by one plain
`guess_only` parse, whose outcome is the word's (its work step count includes the restricted
parse's). A restricted parse that has analyses, or reached the work cap, is the outcome as it is.
The `guess_only` parse after a miss returns what the one-call `guess_root` parse returns: analyses,
order, guessed flag and guessed strings, pinned by the pg-parse test
`miss_then_guess_only_equals_one_guess_root_call`. Calibration compares the whole staged parse
with the plain parse under the caller's options (guessing on): the saving is the plain cost minus
the restricted and guess-only costs, and the equality check covers the analyses, the guessed flag
and the guessed strings.

Equality with `off`. The analyses equal those of `off` on every word whose parse completes in both modes.
A word that is incomplete in either mode (it reached the work cap or the capture cap) can have a different partial
list, in either direction
(a restricted search does less work and a tiled rejection of a word whose plain parse would cap returns an
uncapped empty outcome; 2 of 320 such words in the nine-grammar runs returned fewer analyses under `auto`,
and both complete with identical analyses at a raised cap). The guess-only path (grammars with lexical patterns) is covered by synthetic grammars
only; none of the nine measured grammars has a lexical pattern.

Cost is the parser work of `pg_cost` (`Counts::cost` when the meter runs, else work steps times its
work weight, which is 3 to 6 times too low; `--prefilter auto` therefore turns the meter on with or without
`--cost-column`, so the decision line is the same either way, and turns it off again after the calibration
unless `--cost-column` asked for it, so the rest of the batch pays no metering (the `outcome_cost` of those
words in `PREFILTER-TOTAL` is then the work-step fallback; `--prefilter on` keeps the meter on for the batch;
before this the meter cost a measured 1.2% of a whole Tamil process and 2.0% of its parse, 1,000 words,
instructions); a library caller of `decide` with the meter off gets the
work-step fallback, documented on `parse_cost`), plus the counted filter walks (`ADMIT_STEP_WEIGHT`, `TILED_STEP_WEIGHT` per table
transition) and the construction units (`ADMISSION_UNIT_WEIGHT`, `TILED_UNIT_WEIGHT`), the weights in
pg-foma `admission/meter.rs`. The `PREFILTER` line gives both stage states with reasons, `admission_units`,
`tiled_units` and the `construction_cost` of both, the calibration words and their extra cost, S and F, and
the projected net over the batch with construction and calibration subtracted; the net may be negative and
is still reported. The `PREFILTER-TOTAL` line gives `words`, `restricted` (words through the restricted
parse), `rejected` (words the tiled stage rejected), `admit_steps` and `tiled_steps` with their `filter_cost`
(steps times weight), `outcome_cost` (`parse_cost` summed over every word's outcome), `construction_cost`,
`calibration_cost` and their sum `total_cost`, so a run states its whole cost in parser units; with the gate
off it still prints, with zero filter fields. Production does not otherwise run an extra plain parse; the
calibration is at most 32 words per batch and about 1/64 of it, and the warm-up words are plain parses, not extra ones.

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

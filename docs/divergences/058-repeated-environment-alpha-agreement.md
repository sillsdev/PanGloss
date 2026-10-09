# 058 — Repeated environment alpha variables lose agreement in HC-Rust

## Kind and status

Behavioural, fixed in this branch; reproduced Rust-only correctness defect.
The resumed research found a separate nullable-environment parity blocker, fixed in entry 065.
Repeated-alpha FST lowering and its independent containment proof are recorded in entry 066.

## Refusal research before lowering

Rust baseline: `87979306d1eb0336030a0d021c3dc46fa6914fa8`.
Oracle: `hc.dll`, Machine `18cf242f4b114b0eb9bac304b4b171ca2f499a39`.
FieldWorks source: `089eb9027b6d81be7883c40960f0de3ffa04b699`.
Read background: `/home/johnm/work/pangloss-review/wild-variants.md`.
Evidence classification: **correctness/representability**, current verdict **CannotRepresent**.
No readiness policy, resource limit, or admission predicate was changed.

Each row has three concrete synthetic XML probes, prefixed `rtl-`, `bounded-`, and
`unbounded-`, in `/tmp/pangloss-lanes/variant-probes/<prefix><suffix>/`.
Each contains `grammar.xml`, `words.txt`, `oracle.tsv` (where C# completed), and
`oracle.log`. The generator and complete C# outcomes are preserved under
`docs/divergences/evidence/058-variant-lowering/`.
All successful final oracle runs completed 17 words: 0 skips; no missing result rows.
The six compile-error controls intentionally have no completed result rows.

| Concrete reason / probe suffix | Actual FST trigger and variants | C# observation | Can FieldWorks author it? Source evidence | Classification / disposition |
|---|---|---|---|---|
| `ambiguous-disagree` | A minus alpha occurrence whose class has two members sharing its governed feature value; `lower.rs:243-254`. All three requested variants when a quantifier accompanies it. | Initial voice probe: all 17 words rejected. Resumed back/round probe: C# accepts `ia -> AU\|ia`, rejects `au`; bounded, unbounded, RTL and plain controls complete 16 rows each, exit 0. | Yes: feature constraints, direction, and simple contexts: HCLoader.cs:2003-2089,2745-2770. | Authorable, positive witness established. The nullable-environment parity defect is fixed in 065; ambiguous-disagreement lowering still refuses and requires its own containment proof. Not a permanent refusal. |
| `alpha-in-repeat` | Any alpha occurrence anywhere inside a repetition, including deeper nested repetitions; `lower.rs:282-285`. All three requested variants. | Bounded/RTL: `ecct -> ROOT2\|ecct`, `accct -> ROOT3\|accct`, `ecat -> ROOT1\|ecat`, `actt -> MIXED\|actt`; `ectt` rejected. Unbounded also rewrites ROOT3 to `eccct`. All exit 0. | Yes: HCLoader.cs:2338-2344 passes the loaded child and authored bounds to Quantifier; 2745-2770 supplies alpha variables. The word-final anchor comes from 2085-2086. | (a). HC-Rust agreement is fixed and oracle-pinned. Repeated environment alpha lowering is implemented with confirmation and independently tested in 066; target alpha repetition stays outside that scope. |
| `inverted-repeat` | Finite `min > max`, even when another quantifier is unbounded; `lower.rs:266-272`. All three requested variants. | Minimum 3, maximum 2 behaves as exactly 3 mandatory copies: `eccct -> ROOT3\|eccct`; `acat`, `acct` unchanged. Exit 0. | Loader can pass stored numbers unchanged (HCLoader.cs:2343), but the authoring dialog refuses inverted values: OccurrenceDlg.cs:217-223. RegRuleFormulaControl.cs:731-742 stores accepted values. | (b) for ordinary authoring: documented permanent refusal of malformed stored bounds; C# acceptance is not evidence of UI authorability. |
| `empty-repeat` | Child slot list empty; `lower.rs:275-281`. All three requested variants. | Empty body contributes epsilon: `acet -> ROOT1\|acet`, ROOT2/ROOT3 unchanged; exit 0. | No: HCLoader.cs:2321-2336 rejects an empty sequence; 2338-2346 only constructs a quantifier when loading its child succeeds. | (b): documented permanent refusal; HC-XML-only loader permissiveness. Empty OptionalSegmentSequence is also outside the DTD (`HermitCrabInput.dtd:560`). |
| `no-owning-table` | Rule missing from grammar or unattached to any stratum; `replace.rs:629-634`, `owning_table_id_for_prule_position`; `capability.rs:792-794`. All three requested variants for structural probes. | Unattached rule is inert: raw ROOT1/ROOT2/ROOT3 parse, rewritten surfaces rejected; exit 0. | No: HCLoader.cs:227-233 constructs strata with m_table; 310-318 attaches each admitted rewrite rule to a stratum. A dangling rule is not a reachable variant in a production plan. | (b): permanent refusal of unowned structural calls. Do not guess table zero. |
| `nested-repeat-control` | **Not a refusal by itself**: recursion accepts nonempty, alpha-free, noninverted nested quantifiers (`lower.rs:275,286-290`). DTD forbids nesting in XML, but C# loader accepts these probes. | Nested bounded groups extend the maximum to 4: `eccct -> ROOT3\|eccct`; exit 0. Unbounded also parses that word. | HCLoader.cs:2318-2346 recursively loads a child; ordinary UI authorability was not established before stopping. | Existing lowering control, not a reason to flip an unlowerable variant. No new support claim. |
| `quantified-focus-control` | **Not a `pattern_slots` refusal by itself**: a nonempty alpha-free quantifier lowers in focus too. | C# compilation rejected all three controls, exit 255: `Load Error: Could not compile phonological rule named rtlBoundedQuantifierDemo`. | No: HCLoader.cs:2033-2040 loops over IPhSimpleContext for focus; Machine PatternNodeCastExtensions.cs:21-39 permits quantifiers only in environments. | (b): permanent unsupported semantic position. This lane did not add or change a separate gate. |
| `quantified-replacement-control` | Same distinction as quantified focus. | C# compilation rejected all three controls, exit 255, same load error. | No: HCLoader.cs:2060-2067 loops over IPhSimpleContext for replacement; Machine PatternNodeCastExtensions.cs:21-39. | (b): permanent unsupported semantic position. No gate changed. |

The whole-rule probes visit LHS, RHS, left environment, and right environment in
`capability.rs:796-815`, calling the owner's `pattern_slots` at the rewrite scope.
The bounded/unbounded label depends on every quantifier in the rule, not on the
specific failing node (`capability.rs:876-924,3313-3319`).
Anchor and same-/foreign-table Segments are already accepted at that scope;
there are no other PatternNode kinds in the exhaustive lowering match.
Natural-class membership with zero members produces an empty relation, not this refusal.
These statements describe the structural census, not newly demonstrated parity.

## Reproduced divergence

No lowering was written. No expectations, coverage golden, or ratchets were edited.
The unchanged HC path, built from the baseline, produced:

| Word | hc.dll | PanGloss HermitCrab path |
|---|---|---|
| `actt` | `ok`, `MIXED\|actt` | `ok`, `-` |
| `ectt` | `ok`, `-` | `ok`, `MIXED\|ectt` |

All other 15 word rows matched. Both engines completed all 17 words.
Reproduction: `bounded-alpha-in-repeat` (committed XML/words/TSVs under the evidence directory).
The environment repeats Any(+voice alpha) twice before t at the word edge.
c is +voice and t is -voice: the two repeated occurrences cannot share one alpha value.
C# leaves underlying actt unchanged. Rust treats the repeated alpha as unconstrained,
rewrites a to e, loses the valid unchanged parse, and invents the rewritten parse.
`pg-rules/src/bridge.rs:64-83` explicitly omits variables inside quantifiers from
`pattern_var_occurrences`; `bridge.rs:249-258` removes their feature constraint.
`rewrite.rs::resolve_bindings` then has no repeated-variable occurrences to check.
This is evidence of a Rust-only correctness defect, not an FST optimization opportunity.
Machine issue: none; this defect is Rust-only. The lead explicitly authorized its owner fix.

## Evidence and follow-up

Committed research evidence: `evidence/058-variant-lowering/probe.py` generates the
24 XML/word probes from the baseline's existing synthetic RTL fixture. Run it from
the repository root using `pwsh -NoProfile -Command 'python3 docs/divergences/evidence/058-variant-lowering/probe.py'`.
It invokes hc.dll with a 30-second external timeout per probe and records completed
word rows and process exits; a timeout is an error, not an expected rejection.
`oracle-results.json` preserves all final observed oracle outcomes and diagnostics.
`bounded-alpha-in-repeat/{grammar.xml,words.txt,oracle.tsv,rust.tsv}` preserves the
exact Rust/C# reproduction (17 completed rows each, 15 matching, 2 divergent).

Fixture presence: twelve oracle-recorded synthetic fixtures under
`conformance-staging/edge-cases/quantified-alpha-*`. Eight cover bounded/unbounded,
LTR/RTL, left/right environments; four add zero-count environments at both word edges.
All XML is well formed. Their words carry `founding-oracle` provenance naming hc.dll
and its exact Machine revision, with no hand-derived expectations.
Regression coverage: every one of the twelve `quantified_alpha_*` parse tests fails
with both owner files restored to the pre-fix revision, then passes with the fix.
No admission or coverage-golden update was made.
No Machine issue or PR was opened: network is closed and this is a Rust-only defect.
Permanent refusal dispositions above follow D7 in
`openspec/changes/archive/2026-08-06-plan-construct-coverage-completion/design.md`;
they do not turn the authorable repeated-alpha variant into a permanent refusal.

## Owner fix and verification

`pg-rules/src/bridge.rs` retains alpha occurrences in a recursive agreement tree while
deriving the same widened frozen-FST pattern from that tree. Constraint resolution has
one owner. `rewrite.rs::resolve_bindings` checks each consumed occurrence in C#'s
target/left/right binding order, reversing left-environment traversal. It considers
subsequent environment matches when a widened FST match violates agreement.
Nullable repeated-variable environments at a word edge consume no occurrences.
The recursive agreement walk charges the existing FST work budget; no limit was raised.

The original eight fixtures complete 136 word rows, and the four zero-count fixtures
complete 64 more. A further bounded/unbounded × left/right × minimum 0/1 ×
bare/trigger environment sweep completes 256 rows with exact C#/HC-Rust status and
signature agreement. `bound-sweep.py` and `bound-sweep-oracle.json` preserve that evidence.
Full `pg.ps1 -Mode conformance-test -Scope local` exits 0; the parse harness exits 0
(84 passed, 10 existing ignored); managed check and quick pass.

The first full local run exposed pre-existing named-fixture lookups through scoped
discovery in CLI stats, build provenance, grammar identities and parse statistics.
A fix-removed full local run reproduced the same failures, plus the new regression failures.
Those named pins now call `pg_conformance_fixtures::require_fixture`, as prescribed by
`docs/design/fixture-pins.md`; generic sweeps retain their requested scope.
No expectations, ignores or gate thresholds were changed.

## Separate blocker found while resuming lowering

The discriminating two-variable back/round witness has underlying `au`. C# preserves
each target segment's non-variable feature while flipping the variable feature: `ia`.
The bounded environment allows zero or one vowel before a final word boundary.

| Word | hc.dll | PanGloss HermitCrab path after repeated-alpha fix |
|---|---|---|
| `ia` | `ok`, `AU\|ia` | `ok`, `-` |
| `au` | `ok`, `-` | `ok`, `AU\|au` |

Both engines complete all sixteen two-segment words; fourteen agree. Evidence is in
`evidence/058-variant-lowering/ambiguous-disagree-parity-blocker/` (XML, words, both TSVs).
`ambiguous-witness.py` reproduces the C# plain/bounded/unbounded/RTL controls. Only the
bounded control was compared with HC-Rust before the mandatory stop. The responsible
Rust operation has not been diagnosed or changed. This is a reproduced defect, not a
lowering failure or a fixture expectation to adjust.

The lead allocated entry [065](065-nullable-rewrite-environment.md) for this defect.
Diagnosis isolated it to the alpha-free nullable word-edge environment, not the
disagreement binding. That owner fix is now oracle-pinned independently of this entry.
FST lowering, permanent-refusal detection tests and coverage obligations remain separate.

# Rule-interaction and ordering coverage

Coverage over which grammar rules actually interact, and in which order — for conformance grammars
as a correctness gate, and for a real grammar against a corpus of analyzed texts as a diagnostic.

## Why the existing mechanism does not answer this

`pg-foma/src/plan_interaction_coverage.rs` is the closest thing in the repo and it answers a
different question. Verified, not assumed:

- **The unit is a plan-node KIND, not a rule.** `leaf_detail` (line 187) matches
  `FragmentSpec::RewriteRule { .. }` and returns the literal string `"RewriteRule"`; the `PRuleId`
  is present in the type and discarded. `AdjacencyTuple` carries no rule id and no cascade position.
- **There are 7 legal tuples in total**, hard-coded, over a five-value node-kind vocabulary
  (`Leaf|Compose|Union|Gate|Replace`). A fixture with twenty rewrite rules contributes the *same
  single tuple* as a fixture with one: the whole ordered cascade is one node, and results land in a
  `HashSet`.
- **Morphological rules are not plan nodes in production at all.** `Provenance::MorphRule` has two
  references workspace-wide: a construction inside `#[cfg(test)]` and a display arm in the diagram
  renderer. Every morph rule across every template compiles into one opaque lexicon leaf, so there
  is structurally nothing to distinguish rule A feeding rule B.
- **It sees one backend of three.** `templated_compile.rs` contains zero mentions of `Plan`, and
  `enumerate.rs`'s own doc says two of the three emission strategies ignore `plan` "entirely since
  this compiler derives its own topology". Coverage over one backend that reads as the compiler's
  coverage is the inheritance trap `coverage_ledger.rs` already documents on another axis.

So `Covered` means "the fixture corpus exhibits this structural shape somewhere". It is a
structural-diversity smoke check. An uncovered tuple cannot mean "undercut rule" or "missing
conformance grammar", because rules are not in the model.

## What IS already modeled

- **Rule order is part of plan identity.** `ReplaceCascadeSpec.rules` is an ordered `Vec<PRuleId>`
  feeding a content-addressed `NodeId`, so two grammars differing only in rule order produce
  different plans. The compiler orders correctly; nothing reports whether we tested it.
- **The crate already draws the mandate/report line this plan needs.** `enumerate.rs` permutes
  gate-group order because it is proven commutative, and refuses to permute rewrite-rule order
  because it is not: "two different rule orders are not, in general, the SAME relation at all …
  left unexplored rather than emitted unsoundly." That discipline exists in the enumerator and has
  no counterpart in the coverage layer.
- **Rule-level tracing exists, with real identity.** `pg_rules::trace::TraceSource` has
  `MorphRule(MRuleId)` and `PhonRule(PRuleId)`; the sink records begin/end events per rule, template
  and stratum as a tree keyed by `TraceHandle`, so application ORDER is recoverable from structure.
  A 1:1 port of C#'s `ITraceManager`.
- **A working consumer of that trace.** `pg-cli/src/assess.rs` runs `parse_word_traced` with a real
  `TreeTraceSink` and walks the result into structured findings. That is the hard part, and it is
  built.

## Three senses of "ordering" — do not conflate

| Sense | Where | Covered today |
|---|---|---|
| Morphotactic slot/template order (d5, the proposer) | dead-end census | per grammar |
| Within-rule application mode (simultaneous vs iterative) | `multipleApplicationOrder` | **live fixture** in `machine/conformance/languages/templatic-root-modification` (`prSimulFeeding`/`prIterFeedingControl`, identical Lhs/Env, different pinned outputs under different POS gates) |
| Between-rule cascade order (A feeds/bleeds B) | `ReplaceCascadeSpec.rules` | **nothing** |

`docs/research/grammar-feature-space.md` cites that feeding/bleeding pair as living in
`polysynthetic-stratal-derivation-chain`. It does not; it is in `templatic-root-modification`. The
fixture is real, the citation is wrong.

## The unit of coverage

Ordered pair `(rᵢ, rⱼ)`, `i < j`, **within a stratum**. For 20 rules that is 190 pairs, per stratum,
summing linearly across strata. It does not explode. Full cascade permutations (`n!`) would, and
nothing here proposes that.

Detection has an in-crate precedent: `SimultaneousSubruleOverlapPredicate` already builds real
automata to detect environment overlap *within* one rule. The same technique generalizes across
rules — intersect rule A's output language against rule B's trigger context. A real automaton
construction, not a structural field read, which is the whole difference from the tuple mechanism.

## Three states, not two

1. **Cannot interact** — environments provably disjoint. Retired with evidence, exactly as
   `retired_interactions()` already does for orthogonal pairs.
2. **Can interact, witnessed** — a real word where both fire and order is load-bearing.
3. **Can interact, unwitnessed** — the actionable signal.

State 3 needs one more bit to be diagnostic rather than merely alarming: is the pair **reachable**
in this grammar — does gating ever let both fire on one form?

- reachable but unwitnessed → **conformance gap**; write a fixture
- unreachable → **undercut rule**; the grammar's own gating prevents it

Without that bit, state 3 relocates the question instead of answering it.

**Mandate only state-3-reachable. Report everything else.** Mandating a fixture for a pair the
linguist never intended to interact manufactures work and inverts the conformance discipline of
deriving fixtures from real typology rather than from a metric.

## Two consumers, one report

**Conformance grammars** — assert the interactions and orderings behave, and declare big-O per
backend. The per-backend part belongs with `recipe-scoped-fst-health`, which already owes a backend
field on every finding.

**A real grammar against analyzed texts** — the same logic, different input. Report per rule and per
ordered pair whether it was exercised, and for rules declared `Unordered`, **which orderings were
actually attested**.

Per backend, always. A report that cannot say which backend it measured repeats the trap above.

## Where this lives: PanGloss or Motif

Split at the evidence/action seam.

**Computing it is PanGloss**, unavoidably: only PanGloss holds the trace, the rule ids, the plan and
the backends. Motif would have to grow a parser to answer any of it.

**Acting on it is Motif**: a PR-like system for semantic changes to language data, with CI-style
checks and typed review before landing, whose named operations already include
`CreateAffixProcessRule`.

The `Unordered` case is the clean illustration. "These two rules are declared `Unordered`, but across
40,000 analyzed words only one ordering is attested" is a MEASUREMENT — PanGloss, from traces.
"Therefore constrain them" is a PROPOSAL about the grammar — reviewed, approved, auditable, and
exactly the kind of change nobody should land unreviewed, because the corpus may simply lack the
counterexample.

So PanGloss emits a schema-versioned report; Motif consumes it as a check and turns findings into
Proposals. Neither grows the other's competence.

## Keeping the contrary witness

Given 40,000 words where all but one use ordering A, the single word using ordering B is the most
valuable datum in the row. Design consequences:

**It cannot be recovered from a saved analysis.** `pg_parse::WordAnalysis` carries `morpheme_ids`,
`root_morpheme_index`, `pos_id`, `syn_fs`, `mpr`, `guessed` and `provenance` — and no rule trace.
Derivation order is an internal application fact, not part of the stored analysis, so "look it up
later in the saved analyses" cannot work regardless of storage format.

**Retention is asymmetric, and that is the point.** Exact counts per attested ordering are cheap.
Witnesses are not stored uniformly: keep ALL minority-ordering witnesses up to a small cap, and only
a bounded sample of the majority. The rare side is what anyone needs; the common side is trivially
re-findable. This mirrors the dead-end census's pinned worst words.

**Store a locator, not a surface string.** The same word form occurs many times in a corpus with
different analyses; a witness must be a stable reference (text + occurrence), or it cannot be
re-examined. `HealthFinding.affected: Vec<String>` is the existing slot shape for this.

**Re-run targeted, not wholesale.** The report carries the witness's identity; a full derivation
trace for that ONE word is then cheap on demand. Re-running 40,000 words with tracing on to re-find
something the first pass already saw is the expensive way to learn nothing new.

**A 1-in-40,000 minority is more likely a data error than an alternation** — a mis-analyzed text, a
wrong POS tag, a typo. The witness is needed to TRIAGE, not only to confirm, which is a further
argument for keeping it and against auto-proposing a grammar edit from a count alone.

## Cautions

- Single-attested-ordering is **evidence, not proof**. The report must say "attested orderings: 1 of
  2 possible, over N words", never "this rule is really ordered", or it licenses a wrong grammar
  edit.
- Tracing costs. This is a batch analysis job, never the parse hot path.
- Advisory report first, three states, per backend, with the reachability bit. Promote individual
  rows to build-breaking only when a real fixture earns it — the path
  `plan_interaction_coverage` itself took.


## Decided: what "proven" means, and how evidence is obtained

Settled 2026-08-09, and it changes both the definition and the mechanism.

**`Proven` is per backend and evidence-backed.** It means "THIS backend supports this construct and
there is evidence", never "some backend can". A grade that is true of one backend and read as true
of the compiler is the same defect as `evaluate_capability`'s join, one level up.

**Evidence is COLLECTED BY RUNNING, never asserted.** At specific gates the conformance suite runs
and records, as a byproduct, which `(characteristic, backend)` pairs were actually exercised. A
completeness test then asks "did I cover everything" and fails if not. This replaces today's
hand-written citations in `strategy_coverage.rs` — a citation can be stale, wrong, or written by
someone who ran nothing, which is exactly how a `Proven` grade ends up resting on air.

**The two halves have different provenance, deliberately.** A run only ever produces POSITIVE
evidence, so:

- **witnessed** — collected mechanically; cannot be hand-asserted.
- **cannot represent** — declarative, backed by the backend's own code and tests.

Every `(kind, backend)` pair must be one or the other. `unwitnessed` stops being a permanent third
category and becomes simply the failure condition. If both halves were declarative we would be back
where we started; if we tried to collect both, a missing fixture would silently read as "this
backend cannot do it".

**The denominator must be stated or the completeness test is meaningless.** "Covered everything"
depends on which fixtures ran and which backends ran. `pg.ps1 -Mode conformance-test -Scope
local|all` already forces the first to be claimed — 25 fixtures under `local`, 46 under `all` — and
the completeness assertion must carry both that scope and the set of backends exercised. Otherwise a
green "covered everything" means one scope with one backend, which is the inheritance trap wearing a
new hat.

**Forcing gates on stated capability, not on evidence.** A user may force a specific backend when it
CAN REPRESENT the grammar, even where no witness exists yet — requiring evidence to force would make
an unwitnessed backend impossible to ever witness. Forcing an unproven-but-capable backend is loud,
reusing the existing capability-override degraded-trust broadcast, not forbidden. Cost is never the
reason to refuse a force: a deliberately-forced performance grammar is a cost decision.

### Where this stands, measured

23 characteristics x 3 backends = 69 pairs. **44 are gaps.**

| backend | witnessed | gaps | cannot represent |
|---|---|---|---|
| `plan-composed` | 8 | 13 | 2 |
| `tuned-surface-probed` | 14 | 9 | 0 |
| `templated-underlying-tokens` | **0** | **22** | 1 |

One backend has no evidence for anything it claims to support.

### Consequences accepted

- **Release is held until every backend is sorted.** `templated-underlying-tokens` at 0-of-22 is not
  a release-blocking accident to be waived; it is the work.
- **The conformance runner must run every backend that can support a grammar, not just the best
  one.** Today `cross_compiler_equivalence_gate` runs all three backends against ONE pinned fixture
  (`template-category-sharing`); evidence for the other 45 fixtures is collected for no backend at
  all. Generalising that is what turns the 44 gaps into a real number instead of an artefact of what
  the gate happens to run.
- **CI fails on any gap**, once the collecting run exists and the backlog is worked down. Enforcing
  it before the gaps are closed would make main red on 44 counts and get the gate switched off,
  which protects nobody — the same reasoning that retired the comment-hygiene ratchet applies here
  in the opposite direction, because that backlog was already at zero and this one is not.

## Status

Design agreed. Not built. The witness-retention section is the resolution of "how do we get the
contrary word", and is recorded here as the answer rather than as an open question.

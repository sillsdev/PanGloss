# Filter-reach shipping specification

## Outcome

Ship the three-language backend-characterization work for Indonesian, Amharic, and Aweti without
permitting an incomplete FST to look successful. Mbugwe is explicitly deferred and is not an
acceptance blocker for this deliverable. The branch is complete only after the required tests pass,
temporary diagnostics are removed, and the reviewed changes are committed and pushed.

That production slice now runs alongside a five-grammar developer stress loop covering Indonesian,
Amharic, Aweti, Sena, and Mbugwe. The stress loop must attempt correctness-admitted Error routes
under external containment, preserve exact completeness, and report backend pain/remedies. Mbugwe
is deferred from production certification, not from stress construction and regression work.

This specification is a delivery contract, not evidence that any of these languages already has a
trusted shipped FST. A language is accepted only after its selected route has a complete payload,
certificate, and the required runtime evidence.

### Scope reset from Sol/xhigh review (2026-08-23)

Outcome-weighted progress is **0/3 certified routes**. The existing infrastructure is useful, but
it is not language acceptance evidence. In particular, an expected constant such as Aweti
`106/106` is a target, not a measurement; only a current exact-case run can establish that result.

The delivery order is now: freeze real case IDs, source bindings, and denominators; make the
selected payload and its evidence one artifact; then fix only failures reproduced by those cases;
then run exact semantic certification. Bare-FST claims and complete-pipeline claims remain
distinct: a pipeline using a proposer, peeler, confirmation engine, or filter is acceptable only
when every required stage is explicitly bound and covered by the certificate.

Stop Amharic TunedSurface exploration as a production-route candidate at the observed
>3-million-pair boundary. Its production route is the Templated route with the required runtime
stages. TunedSurface may still be measured in contained developer stress mode. Do not add generic
capability, peeler, advice, or PlanComposed work without a frozen failing case. Mbugwe remains
deferred from this production slice and active in the separate stress loop.

| Language | Code path | Bounded corpus evidence | Certified artifact | Trusted shipped FST |
|---|---|---|---|---|
| Indonesian | Foma propose→confirm and replace-rule prototype paths exist; current construction is not identity-bound. | Historical 121/121 F3, separate 114-case P6, and separate 97/97 non-redup P6 scopes; denominators are not interchangeable without an exact case mapping. | None | None |
| Amharic | Templated/replace prototype code paths exist. | Bounded comparison: 622 cases, 51 engine-timeout exclusions, 0 mismatches; not timeout-free. | None | None |
| Aweti | Templated-underlying prototype code path exists, including the 18-rule composition path. | 100/106 oracle-bearing words, with six gaps. | None | None |

Plain-English reading: a code path is an exercised implementation route; bounded evidence is a
scoped measurement; a certified artifact has a valid completeness certificate; and a trusted shipped
FST is a certified artifact accepted for delivery. None of the three current languages reaches the
last two columns.

Current-tip note: `fst/selector-relation-floor` includes the atomic template-slot carrier and
fail-closed validation against the grammar's final active pipeline table. The carrier is relevant
to Aweti's mixed-slot route correlation; it is not Amharic-dependent. Indonesian, Amharic, and
Aweti certification is `not_run`/pending marker-relation production wiring on this tip, even though
the private corpora are available through `PANGLOSS_CORPUS_ROOT`; the historical denominators above
remain bounded, non-certifying evidence.

## Terminology

In this work, a **computationally awkward FieldWorks grammar** may be described informally as a
“bad grammar” only in this narrow sense: the project models a valid language with constructs that
overgenerate candidates or interact less efficiently than a more precise, language-valid
FieldWorks representation might. It is not a claim that the language, analyses, or author are bad.
Any suggested restructuring remains conditional and must include: “Don't make any change that
would make your language invalid!”

## Required behavior

1. Every executable backend produces a compatibility report, including backends that refuse or
   fail. Reports retain findings, predicates, shapes, cost evidence, advice references, and status.
2. Semantic/representability gaps, including inability to prove that a construction can be
   complete, are Critical. An otherwise complete strategy stopped by the selected resource
   envelope is an Error-level incomplete attempt. Normal production builds fail closed on either.
   A developer stress attempt may use hidden
   `--remove-size-limits` to disable only internal deterministic size/work caps, while retaining
   worker isolation, bounded I/O, external watchdog/RSS/absolute ceilings, capability checks,
   complete closure, finalized payload, and parity. Hidden developer-only `--allow-unproven` is a
   separate correctness override that may omit valid parses, is rejected in production, and never
   produces a publishable or certifiable result. Partial/truncated/skipped output is never success.
3. A completeness certificate exists only when a real FST payload was built, the emitter reported
   Full, no constructs were uncovered, no successors remained, and no enumeration budget tripped.
4. The selector reports warnings and errors for every backend. Normal production selection chooses
   only a correctness-admitted backend with health at most Warning; an explicit stress selection
   may attempt a complete Error candidate, but keeps it production-unready. Critical correctness
   candidates remain refused unless a developer explicitly uses `--allow-unproven`; ranking and
   fallback never silently change these dispositions.
5. Mbugwe is deferred from the current three-language production-acceptance slice and is not its
   acceptance blocker. It is included in the five-grammar developer stress loop. Its existing
   reports and full morphological parser are evidence inputs; the parser is an analysis path, not
   proof of FST completeness.
6. The two regression grammars under
   `rust/crates/pg-foma/tests/fixtures/pangloss/fst-completeness/` remain PanGloss-internal and are
   never promoted to Machine.
7. Capability cards are static and machine-independent. Language, corpus, timing, and machine
   measurements belong only in per-build compatibility/build reports.
8. A physical template slot is one authored choice. Classification remains per allomorph: each
   true two-sided allomorph returns `DirectWholeRootWrapper { prefix_variants, suffix_variants }`
   and must emit its full internal Cartesian product. A `SlotProjectionAlternative` whose
   `route()` is `SlotAlternativeRoute::Coupled` keeps different allomorph alternatives correlated
   across the root: for `Coupled(p,s)` plus `Suffix-only(t)`,
   accept `pROOTs` or `ROOTt`, never invented `pROOTt`. Carry that route-specific state through the
   root/continuation topology (duplicated topology is acceptable) or a verified carrier relation.
   If neither is available, fail closed before lexc parsing. The independent
   `classify_template`/`build_slot_chain` prefix/suffix lists, flags, candidate filtering, and
   partial FSTs cannot satisfy this gate. One-sided wrappers remain ordinary edge routes;
   template-only rules are not standalone derivations, and explicit stratum sites remain distinct.

## Capability-card contract

Generate one checked-in Markdown card for each executable backend:

- `tuned-surface-probed`;
- `templated-underlying-tokens`; and
- `plan-composed`.

Each card is generated from a single versioned capability catalog and contains:

- stable backend and envelope IDs plus human names;
- whether each envelope is inherent or controlled by a named switch, including its default;
- Big-O notation, named variables, and which ordering, null, deletion, or other features contribute;
- linked remedy IDs and a reference to the authoritative shared advice catalog; and
- the mandatory language-validity safety statement on potentially meaning-changing remedies.

The managed PanGloss `pg.ps1` build and release modes regenerate deterministic output after a
successful workspace build or `pg-foma` package build. The Markdown cards are checked in for human
and AI use. There is intentionally no stale-diff enforcement: ordinary tests do not compare or
rewrite the tracked cards. The static catalog remains the source of truth, and a subsequent managed
build or release refreshes the cards when that catalog changes.
Card presence or metadata is not evidence that a language has a trusted build.

## Historical lessons for the three-language slice

These observations explain why the current work adds capability envelopes and artifact evidence;
they are historical context, not current shipping claims.

- **Indonesian:** the older construction had strong corpus evidence, including 121/121 parity, but
  that result did not by itself bind an exact envelope to a reproducible, complete artifact. See
  [`foma-fst-plan.md`](foma-fst-plan.md), the historical F3 verdict, and commit `87320bff`.
- **Amharic:** the older mainline result compared 622 cases with 51 engine-timeout exclusions and
  zero mismatches. That is useful bounded evidence, not timeout-free grammar-wide certification.
  See [`foma-fst-plan.md`](foma-fst-plan.md) and commit `87320bff`.
- **Aweti:** the templated prototype compiled all 18 rules and recalled 100/106 oracle-bearing
  words; six real morphology/rule gaps remain. See [`synthetic-stress-grammar-plan.md`](synthetic-stress-grammar-plan.md),
  [`2026-08-20-aweti-enum-budget-census.md`](2026-08-20-aweti-enum-budget-census.md), and commit
  `9b06b102`.

### Lessons carried forward / traceability

Each item below turns historical evidence into a current invariant. The cited gate is the consumer;
these are not new test results or claims that a trusted artifact already exists. The surviving
cross-change authority is
[`cover-circumfix-cross-product-and-infix-drop`](../../openspec/changes/cover-circumfix-cross-product-and-infix-drop/tasks.md)
for Amharic/Aweti morphology coverage and route certification.

- **Review status is an acceptance invariant.** Count certified routes, not infrastructure or
  expected constants: the current baseline is 0/3, and Aweti `106/106` remains unmeasured until
  its exact frozen gate passes. Freeze denominators before adding framework work; every later fix
  must be driven by a declared red case. Consumed by tasks 6.1–6.6 and 7.1–7.3.
- **A backend claim names its whole execution path.** A bare FST cannot inherit correctness from a
  peeler, confirmation engine, or filter. Conversely, a complete pipeline may be certified only
  when those stages and their bindings are part of the selected artifact evidence. Consumed by
  tasks 4.1–4.3 and 6.1–6.4.
- **Scope is deliberately narrow.** Amharic TunedSurface exploration stops at the observed
  >3-million-pair boundary; use the Templated route. Mbugwe, generic peeler support, advice
  expansion, and PlanComposed implementation are deferred. The static card contract remains in
  force, but cards are not route evidence. Consumed by tasks 6.1–6.6 and 7.1–7.3.

- **No single old builder solved all three languages.** The P6 rule compiler reached Indonesian
  parity and Amharic alpha-tuple scale, while the underlying emitter was still template-less and
  Aweti needed a different route. Retain per-grammar capability selection; reject one universal
  builder or language-specific shortcut. Consumed by the sibling change's tasks 5.3 and 6.1; the
  morphology change's route gates remain specific to Amharic/Aweti.
- **The production hand-spun builder was renamed, not lost.** `emit` remains the
  `TunedSurfaceProbed` production backend. Indonesian's July P6 97/97 result came from the separate
  `uflexc + replace` prototype and cannot certify the Tuned artifact. Retain exact route provenance;
  reject treating prototype success as evidence that production construction completed. Consumed
  by the sibling change's tasks 3.1–3.4 and 5.1–5.3.
- **`hc-hybrid` retirement is intentional.** Commit `9a89a32c` removed the old proposer after the
  propose→confirm architecture was gated. Retain Foma proposal plus real-engine confirmation and
  fail closed; reject reviving a hidden full-engine/FST fallback. Consumed by the sibling change's
  tasks 5.3, 6.1, and 6.2.
- **Indonesian bridges were useful but not proof.** Junction probing, static MPR/POS partitioning,
  and real-engine confirmation supplied upward candidates and bounded parity (121/121, 114, and
  97/97 are distinct scopes). Retain upward-only proposals and HC set confirmation; reject treating
  corpus parity, candidate identity, or a partial closure as a certified artifact. Consumed by
  the sibling change's Indonesian tasks 3.1–3.3 and artifact gate 4.1–5.4, 6.1.
- **Indonesian needs complete closure and exact artifact binding.** `8997cdac` made incomplete
  composite closure refuse, and `87320bff` recorded the fail-closed artifact boundary, while the
  114-case retry was only static admission. Retain empty worklists, completeness certificates,
  corpus/grammar identity, envelope, and network fingerprint; reject depth-based success or static
  admission standing in for a built artifact. Consumed by the sibling change's closure tasks
  2.1–2.4, Indonesian tasks 3.1–3.3, and artifact tasks 4.1–5.4, 6.1.
- **Amharic successes remain bounded.** Interdigitation and 20-variable alpha tuples (312
  survivors), plus static partitioning, establish code-path evidence; the 622-case comparison
  excluded 51 engine timeouts. Retain those mechanisms and exact analysis-set comparison; reject
  timeout-free or grammar-wide claims. Consumed by the morphology change's tasks 3.3–3.6, 6.2,
  and 6.4.
- **Aweti's scale result selects architecture, not a pass.** The census measured 3,093,412 eager
  composites (the budget latch is only a floor); `dfb5025f`/`f892cfd0` show the underlying route
  compiles all 18 rules but recalls 100/106. Retain templated-underlying composition and bounded
  counters; reject eager budget inflation, unsound filtering, and depth caps. Consumed by the
  morphology change's tasks 4.1–4.3 and 6.3–6.4.
- **Aweti never had a complete hand-spun FST to recover.** The direct `emit` path reached roughly
  4.9 GB RSS on 855 entries and 135 morphotactic rules without completing construction. The later
  18-rule P6 artifact was a rule-only cascade with no Aweti lexc/emission result. Retain Templated
  Underlying Tokens as the scale successor; reject describing current work as restoration of a
  previously complete Aweti builder. Consumed by the morphology change's tasks 3.1–4.3 and 6.1–6.4.
- **The Aweti truncation hypothesis was false.** `fa81ec82` found 0/16 gain: the apparent drops were
  floating-segment realization, and post-hoc boundary deletion regressed `apply_up`. Retain only
  explicit classified structural recipes with cleanup after phonology; reject `rhs_drops_lhs_material`
  as a deletion proxy and post-hoc boundary deletion. Consumed by the morphology change's tasks
  2.4, 3.6, and 4.2.
- **Fixed-depth and truncated closure are never success.** Historical chain work reduced ambiguity,
  but incomplete worklists still refuse and the remaining Aweti misses remain open. Retain native
  loops, finite closure, or typed refusal; reject fixed-depth returns, early stopping, and partial
  artifacts. Indonesian consumption is the sibling change's tasks 2.1–2.4 and 3.1–3.3; the
  Amharic/Aweti route consumption is the morphology change's tasks 4.2–4.3 and 7.1.
- **Identity and marker handling must be closed under every consumer.** The Aweti marker experiments
  require one known marker consumed exactly once; the historical Foma-rs literal-`0` tag bug showed
  that `fsm_intersect` can fail even when `apply_up` appears correct. Retain source-level tag
  encoding, marker isolation/leak counters, and consumer regression gates; reject unsafe identity
  fallback, marker leakage, and trusting one API's view. Consumed by the morphology change's tasks
  3.5–3.6, 4.1–4.4, and 6.4, plus the sibling artifact tasks 4.1–5.4 and 6.1.
- **Foma token framing is semantic.** Adjacent non-ASCII PUA tokens historically reduced the
  Indonesian P6 result from 97/97 to 72/97; separating rendered tokens restored the intended
  alphabet. Retain explicit token separation and round-trip witnesses; reject source text whose
  code-point adjacency changes tokenization. Consumed by the morphology change's task 4.5.
- **Composition algebra is not interchangeable.** Unioning mutually exclusive alpha-tuple branches
  created spurious paths and a 392,311-state/6,892,003-arc network; sequential composition produced
  38 states/401 arcs and correct paths. Retain authored stratum/rule order and narrowly prove any
  safety union; reject generic tuple-rule union or reordering for compactness. Consumed by the
  morphology change's tasks 3.6 and 4.6.
- **Selected and realized routes must be the same artifact.** Keep-old-paths and the Aweti cascade
  experiment showed that a smaller network can still lose recall. Retain `preferred == selected ==
  realized`, immutable envelope metadata, and a fingerprint for the measured network; reject
  language-ID switches, stale profiles, and un-fingerprinted substitutions. Consumed by the sibling
  artifact tasks 4.1–5.4 and 6.1; the morphology route check remains in its tasks 6.1 and 7.1.
- **Completion evidence is backend-specific.** Candidate admission answers which routes are
  eligible; it does not prove that a completed artifact for any one of those routes is available.
  Retain a TunedSurface closure proof only for TunedSurface and a Full-emission proof only for the
  templated route; never let one route masquerade as another's certificate.
- **Semantic equality and deterministic counters outrank elapsed time.** Historical gates exposed
  losses hidden by reachability, containment, or timing; Aweti diagnostics require exact canonical
  analysis sets and reproducible recipe/state/arc/timeout counters. Retain exact set equality and
  deterministic evidence; reject elapsed-time success, first-analysis agreement, or containment
  as recall proof. Indonesian construction/identity is consumed by the sibling change's tasks
  3.1–3.3, 4.1–4.3, and 6.1; Amharic/Aweti exact-set gates are the morphology change's tasks
  6.2–6.4 and 7.2.
- **Construct coverage must be explicit.** The cascade experiment lost 6/25 on process-morphology
  shapes despite smaller networks, and P6 left RTL, simultaneous, quantifier, and metathesis
  boundaries. Historical P6 also did not prove POS/MPR gating or multi-table behavior. Retain
  synthetic positive/negative witnesses and honest fail-closed/`ConfirmOnly` findings; reject
  generalizing from the three reference grammars. Consumed by the morphology change's tasks
  2.1–4.6 and 5.2–6.4.

## Verification and delivery

- Resolve every failure from the 1,066-test `pg-foma` run without weakening normal fail-closed
  construction.
- Re-run focused regression targets, the two PanGloss-only completeness targets, Indonesian/
  Amharic/Aweti backend reports, package tests, CLI/pack tests, and repository hygiene. Mbugwe
  corpus smoke is deferred with the language, not a release gate for this slice.
- Run the authoritative package/full-suite checks after focused tests are green.
- Remove diagnostic output hooks and exclude `.tmp/`, the transcript, and the intentional dirty
  `machine` pointer from the commit.
- Inspect the final diff, commit on `filter-reach`, and push the branch.

## Explicitly out of scope

FLExText ingestion, LibLCM cache export/import, Motif orchestration, text filtering, UI presentation,
and promotion of the PanGloss-only fixtures are separate work.

# PanGloss Grammar Coverage

This context defines the language used to describe HermitCrab compatibility, FST proposal coverage, and resource safety.

## Compilers, switches, and the compatibility report

Settled 2026-08-07. Terminology first, because it decides how the thing gets built.

**Compiler.** A whole-grammar compilation strategy. There are three. They are not variations on one
compiler — they combine features in genuinely different ways, so they grow and fail differently on the
same grammar. Called compilers rather than *recipes* deliberately: no analogy. Every metaphor tested
broke somewhere (a recipe implies the same dish each time and is overloaded in build tooling; a
kitchen has no word for the small variations; a road has none either; an *accelerator* actively
misleads, because a switch can change what is REPRESENTABLE, not merely how fast). The domain already
carries grammar, rules, strata, feeding and bleeding. It does not need a kitchen.

**Switch.** An optional variation within one compiler. Composable, and each must earn its place;
added complexity is earned rather than assumed.

**Compatibility report.** What each compiler says about a given grammar, after characterization. Not
a verdict — a report, with reasoning a human or an AI can act on.

**Selector.** Reads every compiler's compatibility report and chooses no path, one path, or two. Its
decision, and the reports behind it, are what the user sees.

### The report has two axes and they must never merge

**Correctness is binary.** Either this compiler can produce right answers for this grammar, or it
cannot. There is no "probably".

- can represent it, and the result is right — **yes**
- can represent it, with the confirm pass pruning the proposal to keep it right — **yes**
- cannot represent the construct at all — **no**
- might produce INCORRECT results — **no**. This is a rejection, not a caveat.

**Cost is graded, and never a rejection on its own.** How large, how slow, how fast it grows with the
number of features. Cost may be unknown, and unknown cost is still a **yes** — with the uncertainty
stated.

The distinction is the whole point. "Not sure" is not a third category: it is either uncertainty about
SIZE, which is a yes carrying a caveat, or uncertainty about CORRECTNESS, which is a no. Merging them
gives "maybe" somewhere to hide, and a maybe is how a compiler ends up shipping analyses nobody
checked.

This is not a new rule, only a consistently applied one — `docs/adr/0001` already requires cost and
capability to be gated by different standards, warning on cost and never hard-failing on it alone.

## Deployment domains

PanGloss is one engine delivered in capability-specific forms:

- **Inference deployment** — browser/WASM, word-processor, and native C hosts load a precompiled
  analysis artifact and perform bounded analysis, spell checking, and glossing. They do not compile
  grammars or FSTs.
- **Native build deployment** — FieldWorks and AI-framework hosts use the C ABI and native CLI to
  import grammar sources, compile analysis artifacts, run compiler-health audits, compare grammar
  versions, and diagnose Rust/FST behavior.
- **Native reference-validation deployment** — an explicit CLI/PowerShell utility may additionally
  run the pinned C# Machine HermitCrab oracle for HC XML. It produces comparison and trace evidence;
  it is neither linked into the runtime library nor available in WASM.

PanGloss is a build/inference engine, not an authoring UI. FieldWorks or another caller owns project
history, baseline selection, publication policy, presentation, and interactive grammar debugging.
PanGloss returns structured diagnostics and investigation handoffs; it never launches FieldWorks.

## Software lifecycle model

PanGloss follows the familiar code/build/test/release/runtime model:

| Software concept | PanGloss equivalent |
|---|---|
| Source code | LibLCM/HermitCrab grammar and stem data |
| Build | Import the grammar, compile the proposer FST, and bind matching Rust-HermitCrab runtime data |
| Compiler diagnostics | FST-health warnings/errors, thresholds, resource outcomes, and remedies |
| Test execution | Run caller-supplied words against one explicitly selected compiled model and pipeline |
| Test artifact | An immutable assessment report with exact per-word outcomes |
| Test diff | Compare two assessment reports; never compare hidden live engine state |
| Release artifact | One validated `.pgpack` PanGloss Language Pack |
| Runtime | PanGloss Runtime in WASM, a word processor, or a native host |
| Plugin installation | Load a data-only Language Pack into PanGloss Runtime |

Build reports and assessment reports are logically separate artifacts with separate comparison
operations. A convenience command may compile and then assess words, but it emits both artifacts.
Compilation may remain in memory for iterative testing; writing a Language Pack is optional. There
is no initial evidence-bundle manifest. Callers own tests, baselines, history, release policy,
installers, and deployment UI.

## Language

**Construct disposition**:
One of compiled, safely overapproximated, peeled outside the FST, confirm-only, or detected unsupported.
_Avoid_: Done, covered

**Supported construct variant**:
A variant whose disposition is not detected unsupported and whose disposition-specific oracle gates pass.
_Avoid_: Detected construct

**Construct witness**:
A positive and negative oracle-backed pair proving that a semantic variant is exercised and distinguished. For detected unsupported variants, it proves reliable detection and rejection, not compilation support.
_Avoid_: Fixture coverage

**Interaction coverage**:
Oracle-backed evidence for declared combinations of two or more semantic variants, normally selected by a pairwise covering array before seeded fuzzing.
_Avoid_: Full coverage

**Corpus recall**:
Containment of every oracle analysis in the proposer-to-confirm result for a declared corpus and denominator.
_Avoid_: Word coverage, language coverage

**Supported language**:
A grammar whose declared corpus has complete corpus recall, stays within its resource envelope, and exercises no detected-unsupported construct.
_Avoid_: Parses some words, corpus covered

**Honest unsupported**:
A semantic variant that is detected and reported before compilation rather than silently mistranslated.
_Avoid_: Done, harmless skip

**Resource envelope**:
The named, versioned combination of parent-enforced worker limits, sampled resource guardrails, bounded communication, and deterministic logical work budgets under which a pipeline is accepted.
_Avoid_: State budget

**Worker watchdog limits**:
Parent-enforced wall time, sampled worker RSS, and bounded input/output protecting the host from a compiler worker. Wall time and I/O sizes are enforced bounds; sampled RSS is a guardrail rather than a kernel memory ceiling.
_Avoid_: Hard RSS limit, process-tree sandbox

**Logical work budgets**:
Deterministic counters used as the primary, reproducible early-stop and admission mechanism. Cooperative elapsed-time checks and the parent wall-time watchdog are outer safety nets, not the normal way to discover excessive grammar work and not memory-safety enforcement.
_Avoid_: Hard limits

**Diagnostic run**:
An observational run that reports correctness, timing, and resource evidence without by itself certifying construct support or a supported language.
_Avoid_: Certification

**Frozen HermitCrab model**:
The closed semantic surface already represented by the complete Rust HermitCrab port. Bug fixes may correct behavior, but no new grammar constructs or model features are expected.
_Avoid_: Extensible grammar model

**Compilation authority**:
The trusted native PanGloss tooling that converts a grammar into a validated, versioned analysis artifact under the applicable resource envelope.
_Avoid_: Browser compiler

**Capability profile**:
The explicit set of operations exported by a particular PanGloss build. Hosts query it rather than
assuming that inference, Rust-HermitCrab diagnostics, FST compilation, or grammar comparison is
universally present. Unsupported requests return a typed
`unsupported_capability` outcome; unavailable capabilities are not inert WASM exports.
_Avoid_: One universal runtime, hidden compiler

**PanGloss Runtime**:
The inference distribution. Native Runtime is `pangloss-runtime`; WASM is another Runtime build.
It loads Language Packs and performs analysis, spell checking, glossing, and supported packaged
Rust-HermitCrab diagnostics. It physically excludes grammar/FST compilation. A process may load
multiple packs as isolated immutable handles; every request names its handle and owns independent
scratch, budget, trace, and cancellation state.
_Avoid_: Lean SDK, browser compiler

**PanGloss SDK**:
The native build/integration distribution. It contains the exact PanGloss Runtime plus the additive
`pangloss-build` library, C headers, CLI, and PowerShell tooling for grammar import, compilation,
health diagnostics, and report generation/comparison. SDK and Runtime share a major/minor
compatibility line. Each SDK bundle ships one exact tested Runtime build; an externally supplied
patch-level Runtime may interoperate only when the declared ABI/package compatibility check passes.
_Avoid_: Full runtime

**Analysis artifact**:
A single, self-contained, versioned, fingerprinted file containing the precompiled FST proposer network and the matched runtime grammar data required by the Rust HermitCrab port to confirm and complete analyses. It is produced by the compilation authority and consumed without rebuilding the FST.
_Avoid_: WASM grammar

**PanGloss Language Pack**:
The product-facing name for the `.pgpack` analysis artifact: a data-only runtime plugin containing
the proposing FST, matching Rust-HermitCrab runtime data, configured compact diagnostic symbols,
and package metadata. It cannot contain WASM modules, native libraries, scripts, or executable
extensions. New engine behavior requires a PanGloss Runtime release.
_Avoid_: Executable plugin, compiler bundle

**Build report**:
An immutable artifact describing one compilation attempt: inputs, effective compiler budgets,
construction outcome, compiled-model fingerprint, FST measurements, and compiler-health findings.
It does not contain word-test results and is not mutated if that model is later serialized as a
Language Pack. The package carries its own identity; a same-invocation write result may report its
path/hash separately.
_Avoid_: Test report, evidence bundle

**Assessment report**:
An immutable artifact describing one caller-supplied word-set run against one compiled model and
named pipeline: context, effective apply budgets, atomic word outcomes, canonical analysis sets,
and runtime evidence. Semantic deltas compare assessment reports only.
_Avoid_: Build report, mutable retry log

**Assessment suite**:
A caller-owned, versioned collection of cases with authoritative order and an optional executable
expectation per case. PanGloss validates and executes a suite and never authors, amends, or
re-adjudicates one.
_Avoid_: Test corpus, word list

**Assessment case**:
One entry in a suite, identified by an opaque caller-issued case ID that is the only join key
between two runs. Two cases may share a surface form and remain distinct, because a case is a
question the caller is asking, not a word.
_Avoid_: Test word, corpus entry

**Analysis failure narrative**:
A pruned prose rendering of why particular analyses were not produced for one word: which candidate
parses were attempted, where each was rejected, and under which typed failure reason. It states
observed rejections, never a root cause or a prescribed grammar edit.
_Avoid_: Trace dump, diagnosis, explanation of the bug

**Analysis-only runtime**:
A runtime that loads a validated analysis artifact and performs bounded analysis but cannot construct or recompile its proposer network. PanGloss WASM is an analysis-only runtime.
_Avoid_: WASM compiler

**First-class platform**:
A production target required to provide the same public behavior, watchdog outcomes, and conformance evidence as its peers. Windows and Linux are first-class PanGloss platforms.
_Avoid_: Primary platform, CI-only platform

**Evidence availability**:
The recorded state of an optional external corpus, submodule, oracle executable, or quiet benchmark environment. Unavailable evidence is reported explicitly; it does not prevent independent implementation or self-contained verification from proceeding.
_Avoid_: Environment hard gate

**Package license declaration**:
Optional, self-declared licensing and publisher metadata carried by an analysis package. A signature can authenticate who made the declaration, but neither the declaration nor its signature grants or denies permission to analyze.
_Avoid_: License enforcement, entitlement

**FST compilation health**:
The compiler-owned assessment of whether a grammar's FST construction and use are predictable, bounded, compact, and actionable. It describes computational consequences, not linguistic quality.
_Avoid_: Grammar quality, language quality

**FST health finding**:
A stable coded compiler diagnostic with severity, phase, affected constructs, measured or predicted values, thresholds, and applicable remedies.
_Avoid_: AI grammar advice

**FST admission result**:
The worst non-overridden FST health severity for one grammar compilation: Ideal, Info, Warning, Error, or Critical. Error and Critical are both **overridable via the capability override** (ADR 0005), which force-compiles behind an indelible degraded-trust runtime signal; the override is explicit and permanently recorded. The trust axis is binary — proven vs unproven — and the *only* non-overridable hard floor is ADR 0003 apply-time execution containment (the killable compile worker + cooperative apply budgets), which converts a real blowup into a clean containment error rather than a crash. A predicted FST-health verdict, however severe, never becomes a non-overridable refusal.
_Avoid_: Supported language status, "Critical is a hard refusal"

**Semantic uncertainty**:
A condition where the compiler cannot preserve every analysis required by the frozen HermitCrab model. It fails closed rather than producing a knowingly incomplete analysis artifact.
_Avoid_: Performance risk

**Cost uncertainty**:
A condition where compilation is recall-preserving but its resource cost cannot be bounded accurately before execution. It is attempted inside the worker watchdog and logical work budgets; uncertainty alone is not a Critical finding.
_Avoid_: Unsupported semantics, automatic rejection

**Explicit resource retry**:
A new caller-requested compilation using a named, versioned resource envelope with larger limits. The compiler never escalates limits or retries automatically; the prior terminal finding remains available to guide grammar improvement or the explicit retry.
_Avoid_: Automatic backoff, hidden retry

**Proven work bound**:
An exact value or conservative mathematical lower bound derived from compiler inputs, suitable for proving that an operation cannot fit within its remaining logical budget. A heuristic estimate is diagnostic evidence, not a rejection proof.
_Avoid_: Guess, expected size

**Semantics-preserving compiler transformation**:
An internal lowering or optimization with a compiler-owned correctness argument that preserves the complete HermitCrab analysis set. Potentially meaning-changing grammar edits, including rule reordering or added constraints without such a proof, are recommendations only.
_Avoid_: Automatic grammar repair

**Propose-and-confirm invariant**:
The PanGloss FST may safely overapproximate by proposing analyses that the matched Rust HermitCrab runtime rejects, but it must not omit a valid HermitCrab analysis. Proposal volume and confirmation work are first-class resource-health dimensions even when the final confirmed set is correct.
_Avoid_: FST-only correctness, free false positives

**Atomic word-analysis result**:
Either the complete confirmed analysis multiset for one word or a typed incomplete outcome. Partial analyses and counts may be diagnostic evidence but are never presented as a definitive result. In a batch, completed words remain valid and incomplete words may be explicitly retried with caller-selected apply budgets.
_Avoid_: Best-effort analysis result

**Batch analysis outcome**:
An ordered collection of atomic per-word outcomes under both per-word and cumulative batch budgets. A word is complete, incomplete if its analysis started but exhausted a limit, or not attempted if the batch stopped before it began.
_Avoid_: All-or-nothing batch

**Named analysis pipeline**:
An explicitly caller-selected runtime path reported with every outcome. Normal deployable analysis is FST propose plus Rust HermitCrab confirmation; Rust HermitCrab-only analysis is also supported for engine integration, parity, and detailed parse-failure diagnostics. No failure changes pipelines implicitly.
_Avoid_: Automatic engine selection

**Semantic analysis equality**:
Equality of completed, deduplicated analysis sets by complete structured analysis identity rather than output order, diagnostic trace, timing, serialized bytes, or duplicate discovery count. Combined FST-plus-HermitCrab results must equal Rust HermitCrab-only results for the same package, inputs, and options; incomplete outcomes are not comparable. Duplicate copies remain attributed diagnostic evidence.
_Avoid_: Byte-perfect parity

**Structured analysis identity**:
The versioned canonical projection of C# Machine `WordAnalysis.Equals`: ordered stable morpheme identities, root-morpheme position, and category/POS. A self-contained value composed of stable source keys, never a reference resolved against a compiled model, so it stays meaningful when the grammar that produced it has changed or become unloadable. Rust's `guessed` flag is a required separately reported annotation for Rust-to-Rust equality. Gloss, surface shape, properties, duplicate counts, discovery order, paths/traces, timing, counters, prose, and serialization formatting are diagnostic or presentation evidence, not core identity.
_Avoid_: Gloss identity, trace identity, ordinal identity

**Stable source key**:
The identity a grammar construct carries in its authored source — the MSA or entry GUID for LibLCM-derived grammars, the `id` attribute for HC XML — retained through compilation so it survives grammar edits. Compiler-assigned dense ordinals are internal addressing and shift whenever authored content is added or reordered.
_Avoid_: Morpheme index, dense id, HVO

**Analysis annotation**:
A property reported alongside a structured analysis that is not part of its identity, such as `guessed`. Two analyses whose identities match but whose annotations differ are the same analysis observed differently, never an addition paired with a removal.
_Avoid_: Identity field, analysis flag

**Reproducible assessment**:
An assessment whose case outcomes were decided only by deterministic logical budgets, so re-running it anywhere yields the same outcome digest. An outer safety net firing — a wall-clock word timeout or watchdog — makes the assessment unreproducible, which is recorded on the report rather than hidden, since the same run on another machine could have completed.
_Avoid_: Stable report, deterministic run

**Outcome digest**:
The digest answering whether a grammar behaved differently: case outcomes and deduplicated structured analysis sets, deliberately blind to engine version, budgets, pipeline, and duplicate counts. It survives a PanGloss upgrade that changes no analysis.
_Avoid_: Result hash, quality digest

**Semantic digest**:
The digest answering whether two runs were the same run: everything the outcome digest covers plus the run's own evidence — duplicate counts, effective budgets, pipeline, importer and compiler versions, model fingerprint, and source hash. It changes whenever the engine did different work, even when every analysis is identical.
_Avoid_: Content hash, run id

**Upstream semantic precedent**:
The relevant behavior and public contract already established in Machine and, where applicable, LibLCM. PanGloss adopts that linguistically designed, production-tested behavior by default. A deliberate divergence requires cited source evidence, a reason the precedent is unsuitable, compatibility consequences, and focused parity/regression tests.
_Avoid_: Clean-slate redesign, assumed equivalence

**Duplicate analysis evidence**:
The count and provenance of repeated copies of one semantic analysis before result deduplication. Duplicates do not change semantic parity, but are first-class FST/runtime health evidence because overlapping proposal paths or rules may produce large redundant confirmation work.
_Avoid_: Extra semantic analysis

**Cross-engine validation batch**:
A native CLI/PowerShell validation run over a caller-supplied word set. It compares the combined pipeline with Rust HermitCrab-only and can additionally invoke C# HermitCrab for HC-XML source grammars. It reports authoritative comparison evidence and availability, never a publish/deny decision; C# execution is never included in WASM.
_Avoid_: Runtime oracle, publication gate

**FieldWorks investigation handoff**:
A machine-readable assessment-delta or trace-diagnostic record for a changed word containing grammar fingerprints, semantic
deltas, associated stable source IDs, suggested trace filters, trace references, and evidence
completeness. It supports a caller-owned FieldWorks or AI diagnostic UI but neither asserts a root
cause nor instructs, launches, or controls FieldWorks.
_Avoid_: FieldWorks automation, automatic grammar repair

**Grammar delta comparison**:
A before/after run over a caller-supplied word set used to show how an intentional grammar, stem-data, option, engine, or policy change affects semantic analyses, duplicates, diagnostics, and health. Each side records its context, but differing context does not prevent execution or suppress the delta.
_Avoid_: Strict parity gate, automatic improvement verdict

**Golden-set diff**:
An exact comparison between observed semantic analysis sets and caller-supplied expected sets, reporting missing, unexpected, and matching identities plus incomplete/unattempted outcomes. It is evidence about agreement, not a PanGloss judgment of linguistic quality or a single closeness score.
_Avoid_: Grammar score, automatic quality verdict

**Golden proposal**:
A separately written candidate golden artifact generated from completed observed results, carrying its source context and a diff from the current golden. Validation never overwrites or mutates an input golden; adoption is an explicit caller-owned review action.
_Avoid_: Bless current output, automatic baseline update

**Analysis breadcrumb**:
Compiler/runtime-owned factual provenance for an observed proposal, confirmation, result, duplicate, or delta: stable rule/construct IDs, named stages, path relationships, outcomes, and completeness/truncation. Breadcrumbs show participation and association; they do not claim that one grammar edit caused a semantic change.
_Avoid_: Root-cause verdict, AI explanation

**Absolute resource ceiling**:
A versioned, hard-coded, deliberately high non-disableable limit above all default, app, and caller limits. Runtime ceilings and budget dimensions are identical across native Windows, native Linux, and WASM. It is an emergency containment boundary, not a normal operating target or a substitute for earlier logical-budget diagnostics.
_Avoid_: Unlimited, default budget

**Characteristics profile**:
The total, fail-closed projection of a grammar-plus-stem-data compilation input onto the constructs, configurations, cardinalities, and scale dimensions that determine compiler capability and cost. Produced by an enumerator exhaustive over the frozen model; an unrecognized construct is a build break, never a silent omission.
_Avoid_: Grammar summary, feature flags

**Capability envelope**:
A compilation stage's oracle- or proof-backed statement of the construct configurations it faithfully handles, the interactions it is proven orthogonal across, and the cardinalities it accepts. Envelopes compose bottom-up over a compilation plan; an interaction neither proven orthogonal nor proven-together fails closed.
_Avoid_: Feature list, supported flag

**Capability predicate**:
An oracle-verified proof obligation that discharges one characteristic (or an interaction of several) at a plan node, returning `Admit`, `ConfirmOnly`, or `Refuse`. It is conservative: it may over-refuse but must never admit a configuration that could omit a valid analysis. Predicates are what the capability envelope is composed from.
_Avoid_: Check, rule (unqualified)

**Confirm-only by default**:
The standing rule that a construct's proposer over-approximates (proposes the superset) and confirm prunes, unless a proven no-false-negative argument licenses the FST to admission-filter. Admission-filtering is an optimization with a proof obligation; absent the proof the verdict is `ConfirmOnly`. This makes "never under-propose" structural rather than a matter of per-author diligence.
_Avoid_: FST filtering as the default, propose-narrow

**Parallel-independence**:
The general form of the interaction predicate proving two composition branches may be merged by union: neither branch's rewrite reads or deletes what the other touches (graph-transformation theory's Local Church-Rosser condition; feeding/bleeding disjointness is the phonology-specific special case). Proven pairwise by critical-pair enumeration over the constructs actually present; this is how "proving orthogonality retires combination space" becomes a finite checklist.
_Avoid_: Independence (unqualified), commutativity (as if free)

**Characteristics check**:
The unifying accept/refuse decision matching a characteristics profile against the composed capability envelope, recording why. It hard-fails any grammar using a not-proven-faithful configuration (an honest carve-out) and never converts a cost prediction into a correctness verdict.
_Avoid_: Validation pass, lint

**Capability evidence provenance**:
Whether a "supported" claim rests on behavioral oracle witnesses (the black-box lexc mainline) or a structural proof over controllable composition (the P6 path). The two are distinguished in the profile and never conflated; presenting a behavioral pass as a structural guarantee is overclaiming.
_Avoid_: Supported (unqualified)

**Conformance coverage gate**:
The CI check that makes "supported" mean "proven accurate": a construct or configuration may not flip from fail-closed to supported unless the in-repo conformance suite exercises it and passes. The capability registry is cross-checked against conformance coverage, and marking anything supported without a covering, passing fixture breaks the build. This is how the claim "if a grammar compiles, it is accurate" is mechanically enforced rather than promised — the conformance suite is the gate through which a construct earns "supported."
_Avoid_: Certification, sign-off (there is no separate terminal certification stage; ground truth is committed per-fixture and integration tests run the current engine against it)

**Capability override**:
An explicit escape hatch that force-compiles a grammar the characteristics check refused. It produces an artifact indelibly stamped unproven/recall-unsafe that still loads and runs. It is the development on-ramp for promoting a construct (force-compile → iterate → prove → cover → flip supported) and the way an in-progress grammar reaches a user for trial. The override is recorded in the pack manifest; it never passes the conformance gate and is cleared only by genuine proof plus a clean recompile.
_Avoid_: Bypass, disable-check (it does not silence the contract; it records the exception and marks the result)

**Unproven-grammar trust signal**:
The strong, machine-readable degraded-trust status a Runtime broadcasts for a capability-overridden pack — pack-level `unproven` at load and a degraded/experimental flag on every analysis result — which a consuming app keys off to warn the user ("this is potentially broken"). For unproven packs the signal, not a refusal, is the safety mechanism; a proven pack fires neither.
_Avoid_: Warning (unqualified), error band (that is the separate cost/health axis)

**Compilation plan**:
A first-class, enumerable description of one legal composition topology for a grammar. Multiple plans may pass capability for one grammar; because every capability-passing plan is recall-preserving, all produce the identical confirmed analysis set and differ only in cost.
_Avoid_: Build path, pipeline (unqualified)

**Plan node**:
One node of a compilation plan's content-addressed DAG, drawn from a closed kind set (`Leaf`, `Compose` — n-ary, with a separate physical strategy — `Union`, `Gate`, `Replace`). Node identity is content-addressed (a function of kind, children, and config) so identical sub-plans share one identity, storage, and measurement, and each node is individually addressable as a coverage/fuzz target.
_Avoid_: Stage, pass (unqualified)

**Differential-correctness oracle**:
The check that builds two or more capability-passing plans for one grammar and asserts their confirmed analysis sets are identical per word; disagreement is a predicate bug (a plan was admitted that is not recall-equivalent), reported with the shortest disagreeing word and the set difference. Because every capability-passing plan is recall-preserving, agreement is required, not hoped for. (Novel: combines sound over-approximation with differential testing; no prior art.)
_Avoid_: Regression diff, A/B test

**Language-preserving operation**:
An FST operation that changes representation but not the recognized relation (trim/coaccessible, epsilon-removal, determinization-where-valid, minimization). Only language-preserving operations are permitted anywhere in a proposer pipeline; anything that can change the relation (beam pruning, top-k/best-path shortcuts) is forbidden in propose and confined to confirm/ranking. This is the mechanical guarantee behind propose being a sound over-approximation for recall.
_Avoid_: Optimization (unqualified), pruning (as if always safe)

**Projected cost**:
A calibrated heuristic estimate carrying an error bound, used to rank compilation plans before building. Overlapping bounds mean "close" — build the top candidates and measure. A point estimate never prunes a candidate by itself, and projected cost is never a correctness gate.
_Avoid_: Cost, size (as if exact)

**Committed plan**:
The reviewed compilation plan recorded authoritatively in grammar configuration and used by every normal build for reproducibility. Distinct from the derived plan cache, a disposable fail-safe local optimization that is never authoritative over it. A tuning run proposes a plan diff; committing it is an explicit human-reviewed action.
_Avoid_: Cached plan (as authoritative), default path

**Runtime feature set**:
The versioned set of runtime operations, payload-format versions, and core (foma/HC-port) semantic versions that a Language Pack requires and a PanGloss Runtime provides. A pack loads iff its required set is contained in the Runtime's provided set. It concerns runtime features only, never grammar constructs, which the compiler has already lowered into generic FST and runtime data.
_Avoid_: Engine version (as the compatibility key), API level

**Append-only runtime compatibility**:
The invariant that a Runtime's provided feature set only ever grows and an existing behavior is never altered in place, so every older pack's required set stays contained and old packs run unchanged on newer engines forever. Forward incompatibility is allowed — a new pack needing a feature an older engine lacks yields a typed upgrade request. A genuine non-additive change is a rare, carefully staged major-version event that recompiles grammars, never a silent in-place behavior swap.
_Avoid_: Semver compatibility, breaking patch

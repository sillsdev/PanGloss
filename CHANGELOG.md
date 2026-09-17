# Changelog

Release notes are authored, not generated; `rust/tools/release.ps1` refuses to tag a version this
file has no section for.

## 0.3.0

The release theme is the FieldWorks project as the input that matters: what PanGloss compiles from
an `.fwdata` file must be the grammar FieldWorks itself would build, every loss on the way must be
named, and every remaining difference from the C# oracle must be pinned by a fixture FieldWorks
could have produced.

### FieldWorks conversion, lossless and measured

- **`.fwbackup` is accepted wherever `.fwdata` is**, with its LDML exemplar sets read for the
  alphabet; plain `.fwdata` imports read sibling `WritingSystemStore/*.ldml`, and an unreadable store
  is distinguished from a missing one.
- **Conversion provenance is recorded end to end.** A `SelectionRecorder` shared by the fwdata
  extractor and the grammar compiler records every owner selection, reference resolution and
  rejection as a typed inventory; finalizers revoke the atoms they compact away; every reject is
  preceded by a select. The conversion-loss delta is derived from that inventory and gated.
- **Typed lossless-conversion policies.** `compile_project_with` takes a `SemanticLossPolicy`;
  the default refuses semantic loss during conversion and names the construct, `MeasureOnly` keeps
  every issue non-fatal so real projects can be measured. Duplicate and missing GUIDs are censused
  and refused as typed issues; dangling template-slot and compound references are fatal when active.
- **A featureless character substrate is completed from authored usage** (affix literal text,
  environment tokens, phoneme codes), so XAmple-shaped projects with no phonological features
  segment the way FieldWorks does. Position-remap mismaps are refused as a class rather than
  re-inferred. Real Sena 3 and Amharic compiles are gated differentially before and after.
- `ActiveParser` and the XAmple cap block are carried on the snapshot and exposed as
  `AnalysisCaps` on the grammar (`None` for XML-loaded grammars); parser parameters report which
  caps parsed.
- **Circumfixes are built the way `HCLoader` builds them** (divergence 039): one allomorph per
  prefix-half x suffix-half x environment combination, each half's stem-side context embedded in the
  stem pattern and only the outer contexts kept as an environment. The previous environment-union
  encoding parsed fewer words than FieldWorks for the real Aweti entry shape.

### The XAmple oracle

- **`XampleProjector`**, a FieldWorks 9 helper that projects a conformance fixture back into a real
  FieldWorks project, runs the HC and XAmple engines against it, and captures deterministic,
  mode-scoped, self-checking results. It refuses before creating a project when the input is not
  fully consumed, and exits 5 naming the failing step.
- **Phonology-mutation manifests** drive LibLCM phoneme counterfactuals through the real XAmple
  engine; a witness-drift probe compares reference wiring, not literal content.
- **The XAmple migration differential gate** compares PanGloss against XAmple on stable GUIDs,
  counting duplicates, canonicalizing hvos instead of blinding them, and splitting the
  compared / XAMPLE-only / HC-only headline by phase. It refuses to run silently: an absent witness
  is an explicit opt-out, never a skip.

### Oracle alignment

- **The divergence catalogue** (`docs/divergences/`, 44 entries) records every known C#/Rust
  difference with kind, status, both sites, the pinning fixture and the upstream issue; a gate keeps
  ids unique and indexed. Every open HC-Rust correctness entry has a `sillsdev/machine` issue
  (#504, #505, #506, #510). The `oracle-alignment` skill states the evidence discipline.
- **Ported from upstream**: the analysis syntactic-feature fold is `PriorityUnion` with state-keyed
  merging (Machine #493/#494), with an exact-inverse variant kept behind the ledger; zero-width
  morpheme identity is pinned as already correct.
- **Fixed in HC-Rust**: iterative epenthesis now walks C#'s one-node cursor, with inserted nodes
  eligible and the 256-node runaway cap (014); natural-class membership is decided by table kind;
  feature-only class membership and synthesis stratum match hc.dll; `Word::alternatives` is shared
  rather than deep-cloned and `apply_mrules`/`apply_templates` stream instead of concatenating.
- **Every fixture states whether FieldWorks could produce it.** Seven staged grammars were rewritten
  into the shape `HCLoader` emits (unordered strata, template slots, inflection-class groups,
  exclude-only co-occurrence, the `IsCircumfix` cross-product) and re-verified against the oracle;
  the five that remain engine-only carry a permanent verdict with the `HCLoader.cs` citation.
  Upstream gained oracle-verified fixtures for per-piece environment checking, simultaneous versus
  iterative application, and iterative epenthesis, each proven to fail with its fix reverted.
- **Fixtures are pinned by name, never by place.** The retired v1 layout is gone; `require_fixture`
  fails loudly instead of skipping when a named fixture is absent, and a gate refuses the three
  shapes of location pinning (`docs/design/fixture-pins.md`).
- The founding-oracle wrapper runs over the filter-pass fixtures too; six staged grammars were made
  loadable by hc.dll; the known-divergence list is empty apart from one rules-attribution artifact.

### Engine and CLI

- **Final-template pruning** with a policy threaded through `Morpher`, dense prune counters, and
  deterministic prune rows; partial grammars are rejected at production admission.
- **A finite default step cap** (`--step-cap`, 50,000,000) replaces the `usize::MAX` sentinel; a
  capped word is typed `CAP` in batch output, never `ok`, and a stats report refuses to span two caps.
- **Grammar health checks** ported from HermitCrab as a `grammar-health` command.
- One `COMMANDS` table drives dispatch and `--describe`; every command consults its own spec for
  unknown flags.
- Analysis memoization gained a per-table byte budget and retained-word bound, measured and tuned;
  its removal is in progress on a branch and is not part of this release.

### Backends

- `pg-health`, a wasm32-safe crate holding the compiler report vocabulary; `pg-wasm` and `pg-pack`
  depend on it instead of `pg-foma`.
- One `Backend` trait with three adapters replaces the dispatch matches; `BackendSelection` owns a
  single fail-closed admission decision; 28 research workbench modules are gated behind
  `test-support` and 31 examples collapsed into `examples/lab`.
- Partial-FST readiness is enforced across every backend and partial recall is measured rather than
  assumed; the templated route compiles pattern roots in token space and proposes every order of a
  small unordered rule set; plan-composed emits realizational allomorphs like any affix rule.
- The backend scoreboard runs over every fixture in both roots and lists each non-exact cell by name.

### Tooling

- `CLAUDE.md` rewritten as instructions with the mechanism moved to `docs/design/`; three gates keep
  the agent docs honest (paths resolve, skills never instruct bare Cargo, catalogue indexed).
- Managed builds: `gc` reaps orphaned scanners and governors and reclaims target dirs by effect, a
  backgrounded build is refused, the managed wait is bounded on tree liveness, and the tree is
  rustfmt-clean so builds stop reflowing it.
- `comment-hygiene.ps1` enforces the one-line implementation-comment cap and checked references;
  `parse_compare.py` gives a `CAP` status its own bucket.

## 0.2.0

The release theme is honesty made mechanical: the capability envelope, the C# founding oracle, and
the measurement gates that keep both from drifting.

### Backends and capability

- **The capability envelope is authoritative before any compile.** Every backend consults
  `refuse_unless_admitted` and refuses with typed `CapabilityDiagnostic`s naming the predicate,
  construct, and witness — never a free-text failure after the fact.
- **All five reference grammars now have an accepted backend** (previously Aweti and Mbugwe had
  none, and Sena's PlanComposed was refused). The unlocking fixes:
  - `REP_VARIANT_CAP` (a count cap that silently discarded root spellings) replaced by an advisory
    breadth threshold plus a byte budget, reported through `VariantLimit` — representability,
    readiness, and containment each answered separately. A complete enumeration is emitted in full
    however large; only an unbounded `*` shape or the byte budget can drop a spelling, and each is
    reported.
  - **Circumfix cross-product loading**: a FieldWorks circumfix entry (prefix-typed x suffix-typed
    halves) now builds one allomorph per pairing, with both halves' environments and positions
    unioned per C#'s own `HCLoader` behavior. Per-side conditioning works via per-run environment
    anchoring (W3.3). Open, documented: N-way cross-products sharing a literal half still
    over-generate versus C#'s disjunctive-allomorph re-check.
- Backend scoreboard (61+ fixtures x 3 backends) extracted from an example into
  `pg_foma::scoreboard` with typed per-cell outcomes, gated by a both-direction ratchet: a worsened
  count is a regression, an improved one fails until the constant is deliberately updated.
- Every capability predicate owes a negative witness (a fixture whose refusal cites it);
  the unwitnessed backlog is ratcheted and cannot grow.

### The C# founding oracle

- **The oracle hierarchy is now stated and enforced**: C# `hc.dll` is the founding oracle;
  HC-Rust (`pg_parse::Morpher`) is a port under test and never a source of truth. Every staged
  conformance fixture declares `# oracle-provenance:`; the rust-only backlog is ratcheted.
- `rust/tools/oracle-conformance.ps1` runs the C# self-check over both fixture roots with a
  commit-matched executable and a reasoned known-divergence baseline. Both this and the existing
  HC-Rust gate pinning the same committed `words.yaml` means HC-Rust and C# agree on the exact
  analysis set — over- and under-generation both caught.
- Found by that gate: six staged grammars the founding oracle rejects as schema-invalid (their
  correctness had never been knowable), and nine `filter-passes` fixtures the C# harness cannot
  discover — all named in the baseline rather than silently green.

### Conformance suite

- Submodule pin moved to `f42d9591` (`integrate-conformance-framework`), a squashed suite on
  mainline v3.9.3 — resolving the pin/tip schema incompatibility and the rebased-away pin.
- Conformance runs must claim their scope (`-Scope local|all`); fixture discovery panics on an
  unclaimed scope rather than guessing.

### Tooling

- `rust/tools/release.ps1`: gated release entry point (clean tree, zero hygiene violations,
  rustdoc, full suite, oracle differential) that stamps, tags, and builds — and never pushes.
- Comment-hygiene reaches zero violations and the release gate holds it there.
- `pg.ps1`: `-Mode check`/`quick`/`run`/`conformance-test`, memory-proportional spawn gates,
  kernel-enforced job objects, and the build-slot mutex fleet (see CLAUDE.md).

## 0.1.0

Initial tagged state: the `hc-*` to `pg-*` rename (`728ffd33`), the frozen HermitCrab model,
the foma-backed propose-and-confirm engine, and the four reference grammars.

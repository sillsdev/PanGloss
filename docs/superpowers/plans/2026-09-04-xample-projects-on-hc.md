# Lossless XAMPLE-Project Conversion to HC Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Convert XAMPLE-authored FieldWorks projects without semantic loss and execute them through the ordinary HC-Rust/FST pipeline, completing only the missing character substrate and refusing wrong, ambiguous, unsupported, or unrepresentable grammars.

**Architecture:** `pg-grammar` owns one snapshot-to-HC compiler. A substrate-completion step runs between character-table construction and natural-class construction; a typed issue collector records provenance, inference, and semantic-loss conditions. `ActiveParser` and XAMPLE caps remain source metadata for migration reports and the external XAMPLE comparator, never runtime grammar switches. Machine owns one canonical FieldWorks witness and a declarative mutation manifest beside each eligible conformance fixture; PanGloss owns the projector, temporary LibLCM mutator, XAMPLE adapter, and executable gates. Production entry points require lossless conversion; the real XAMPLE engine is a differential measurement harness, not an HC oracle.

**Tech Stack:** Rust, `pg-snapshot`, `pg-fwdata`, `pg-grammar`, `pg-rules`, `pg-parse`, `pg-foma`, `pg-cli`, `serde_yaml`, `quick-xml`, `unicode-normalization`, `unicode-general-category`, `libloading`; managed LibLCM/FieldWorks helper; all Rust builds and tests through `rust/tools/pg.ps1`.

---

Status: **AUTHORITATIVE PLAN, 2026-09-04**. It supersedes the unimplemented profile/cap portions of
the 2026-09-03 Plans 2–4. Plan 1's import work is retained. Read
`docs/research/xample-grammars-on-hc-rust.md` and
`docs/superpowers/specs/2026-09-03-xample-shape-grammars.md` before executing.

## File map

- Modify `rust/crates/pg-grammar/src/model.rs` and `load.rs` — remove the unwired
  `Grammar.analysis_caps` experiment.
- Create `rust/crates/pg-snapshot/src/conversion.rs` and modify `pg-fwdata` extractor owners —
  preserve graph-to-snapshot identities and fatal import issues through serialization.
- Create `rust/crates/pg-grammar/src/compile/issues.rs` — compiler-specific codes, report assembly,
  and fatal conversion error; shared serializable issue/provenance types live in `pg-snapshot`.
- Create `rust/crates/pg-grammar/src/compile/options.rs` — substrate and loss policy only; no
  parser profile.
- Add owner-published inventory recording throughout `pg-fwdata` and `pg-grammar` — end-to-end
  authored/represented/synthesized identities and bidirectional deltas.
- Create `rust/crates/pg-grammar/src/compile/substrate.rs` — deterministic completion from actual
  authored usage.
- Modify `rust/crates/pg-grammar/src/compile/chardef.rs` — expose the raw-table build seam and add
  completed definitions before table finalization.
- Modify `rust/crates/pg-grammar/src/compile/environment.rs` — expose literal-token collection from
  the owner parser; report invalid or weakened environments as loss.
- Modify all `rust/crates/pg-grammar/src/compile/*.rs` warning-and-drop sites — emit typed issues;
  production conversion refuses on semantic loss.
- Modify `rust/crates/pg-grammar/src/compile/mod.rs` and `lib.rs` — compile options/output and the
  single lossless orchestration path.
- Modify `rust/crates/pg-cli/src/main.rs` — substrate override and grouped conversion diagnostics.
- Modify `rust/crates/pg-foma/src/worker.rs` and every snapshot compilation caller — consume the
  same successful `CompileOutput`; never carry a parser profile.
- Create `rust/crates/pg-grammar/tests/conversion_inventory_gate.rs` — before/after differential.
- Create `rust/crates/pg-grammar/tests/lossless_conversion_gate.rs` — substrate and refusal gates.
- Create `rust/crates/pg-xample-oracle/`, a version-pinned FieldWorks-transform helper, and
  `rust/crates/pg-parse/tests/xample_migration_differential_gate.rs` — legacy comparison lane.
- Add `fieldworks/project.fwdata` and `fieldworks/phonology-mutations.yaml` beside each eligible
  fixture under `machine/conformance/{languages,edge-cases}/`; modify
  `machine/conformance/PROTOCOL.md` to define these as optional cross-engine source evidence.
- Create `tools/xample-projector/` — the managed helper that opens the FieldWorks project, applies
  typed mutations to a temporary LibLCM clone, and owns the real HCLoader and
  `M3ToXAmpleTransformer` calls; Rust communicates with it only through versioned JSON.
- Create `docs/adr/000N-xample-comparator-jurisdiction.md` — why XAMPLE measures migration but does
  not define HC semantics.

## Task 1: Remove runtime caps and preserve source metadata honestly

**Files:**

- Modify: `rust/crates/pg-grammar/src/model.rs:1078-1128`
- Modify: `rust/crates/pg-grammar/src/load.rs:550-569`
- Modify: `rust/crates/pg-grammar/src/compile/mod.rs:238-258`
- Modify: `rust/crates/pg-grammar/src/compile/tests.rs:23-34`
- Modify: `rust/crates/pg-snapshot/src/morphology.rs:372-394`
- Modify: `rust/crates/pg-fwdata/src/parser_params.rs`
- Modify: `rust/crates/pg-fwdata/src/extract/mod.rs`
- Modify: `rust/crates/pg-fwdata/src/lib.rs` and `pg-snapshot` serialization — preserve import
  issues and conversion provenance across every production path.

- [x] **Step 1: Remove the obsolete test**

Delete `xml_loaded_grammars_carry_no_analysis_caps`. The absence of XAMPLE caps from a runtime
grammar is structural after this task, not an optional-state behavior.

- [x] **Step 2: Remove runtime cap state**

Delete `AnalysisCaps`, `Grammar.analysis_caps`, and both `analysis_caps: None` initializers. Keep
`pg_snapshot::XAmpleParameters`; it is source provenance used by reports and comparison tooling.

- [x] **Step 3: Distinguish absent from malformed comparison metadata**

Before the search, change the `XAmpleParameters` documentation: absent values remain `None`; no HC
compiler applies XAMPLE defaults. Interpretation belongs only to migration/comparison tooling.

Add a parser test proving absent and malformed parameters are distinguishable in the import report:

```rust
#[test]
fn malformed_xample_cap_is_reported_not_silently_treated_as_absent() {
    let (params, issues) = parse_with_issues(Some(
        "<ParserParameters><XAmple><MaxPrefixes>many</MaxPrefixes></XAmple></ParserParameters>"
    ));
    assert_eq!(params.xample.max_prefixes, None);
    assert!(issues.iter().any(|i| {
        i.code == codes::INVALID_PARSER_PARAMETER && i.message.contains("MaxPrefixes")
    }));
}
```

Add the corresponding `ActiveParser` cases: absent resolves to `XAmple`; exactly `HC` and `XAmple`
resolve to those variants; malformed XML or any other non-empty value emits fatal
`invalid-source.active-parser` rather than defaulting to XAMPLE. This makes “XAMPLE-authored” mean
the current/defaulted source value, not an unknowable historical parser choice.

Have `extract::Ctx` append these issues to the existing `ImportReport`. Invalid legacy cap metadata
is non-fatal for normal HC because it does not participate in HC semantics, but it must remain
visible to the comparator.

- [x] **Step 4: Prove all references are gone**

Run:

```powershell
rg -n "AnalysisCaps|analysis_caps|ParserProfile|ProfileChoice" rust/crates
```

Expected: no production references. References in explicitly superseded Markdown plans do not
count; no Rust source may retain the experiment.

- [x] **Step 5: Check all targets without linking**

Run:

```powershell
& .\rust\tools\pg.ps1 -Mode check -Package pg-snapshot
& .\rust\tools\pg.ps1 -Mode check -Package pg-fwdata
& .\rust\tools\pg.ps1 -Mode check -Package pg-grammar
```

Expected: PASS.

- [x] **Step 6: Commit**

```powershell
git add rust/crates/pg-snapshot rust/crates/pg-fwdata rust/crates/pg-grammar
git commit -m "grammar: keep XAMPLE caps as reported source metadata"
```

## Task 2: Build the end-to-end before-change loss inventory

**Files:**

- Create: `rust/crates/pg-snapshot/src/conversion.rs`
- Modify: `rust/crates/pg-snapshot/src/lib.rs` and snapshot serialization tests
- Modify: `rust/crates/pg-fwdata/src/extract/mod.rs` and every extractor owner
- Modify: every `rust/crates/pg-grammar/src/compile/` owner
- Create: `rust/crates/pg-grammar/tests/conversion_inventory_gate.rs`
- Modify: `docs/superpowers/specs/2026-08-22-hc-stats-implementation-plan.md` and
  `docs/research/pangloss-stats-attribution-and-aggregation-spec.md` — migrate the documented
  snapshot hash contract from all serialized bytes to semantic source fields.

This task is measurement only. Do it before substrate or refusal behavior changes. The source side
begins at the FieldWorks graph, not at `Snapshot`: anything an extractor omits is otherwise
impossible for `pg-grammar` to detect.

Implement Task 2 as seven sequential, independently green commits: (1) snapshot schema and hash
contract, (2) raw-record census, (3) extractor-owner slices, (4) compiler-owner slices,
(5) compiler-finalizer accounting, (6) measured API, and (7) integration gate. Do not combine
these into one broad agent task. The task is complete only when all seven slices and the final gate
are complete; slicing is review containment, not a reduction in scope.

- [x] **Step 1: Add identity-based inventories at their owner seams**

Counts alone are insufficient because one authored object can expand to several compiled objects.
Use stable FieldWorks GUIDs plus attachment identities:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum InventoryKind {
    Entry,
    Sense,
    EntryReference,
    Msa,
    Allomorph,
    AffixProcess,
    Environment,
    Phoneme,
    BoundaryMarker,
    NaturalClass,
    FeatureDefinition,
    FeatureValue,
    FeatureStructure,
    PhonologicalContext,
    FeatureConstraint,
    MorphemeCoOccurrence,
    AllomorphCoOccurrence,
    PhonologicalRule,
    CompoundRule,
    PartOfSpeech,
    InflectionClass,
    StemName,
    RuleFeature,
    ParserSetting,
    StrataConfiguration,
    Template,
    TemplateSlot,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum InventoryIdentity {
    Object { guid: String },
    Attachment {
        owner_guid: String,
        target_guid: String,
        role: String,
    },
    Expansion {
        owner_guid: String,
        member_guids: Vec<String>,
        role: String,
    },
    Setting { name: String },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct InventoryKey {
    pub kind: InventoryKind,
    pub identity: InventoryIdentity,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversionInventory {
    pub authored: BTreeSet<InventoryKey>,
    pub considered: BTreeSet<InventoryKey>,
    pub selected: BTreeSet<InventoryKey>,
    pub represented: BTreeSet<InventoryKey>,
    pub rejected: BTreeSet<InventoryKey>,
    pub synthesized: BTreeSet<InventoryKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IssueClass {
    MalformedSource,
    InvalidSource,
    AmbiguousSource,
    UnrepresentableForHc,
    SubstrateUnresolvable,
    MigrationDifference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRef {
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversionIssue {
    pub code: String,
    pub class: IssueClass,
    pub source: Option<SourceRef>,
    pub fatal: bool,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversionProvenance {
    pub schema_version: u16,
    pub source_inventory_status: SourceInventoryStatus,
    pub source_census: RawSourceCensus,
    pub graph_to_snapshot: ConversionInventory,
    pub import_issues: Vec<ConversionIssue>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawSourceCensus {
    pub total_occurrences: u64,
    pub class_occurrences: BTreeMap<String, u64>,
    pub unhandled_class_occurrences: BTreeMap<String, u64>,
    pub ordered_header_sha256: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceInventoryStatus {
    ImportedComplete,
    ImportedWithFatalIssues,
    Synthetic,
    #[default]
    Unknown,
}
```

`RawGraph` retains an ordered `RawRecordHeader` occurrence for every `<rt>`, including unknown
classes, while the import is live. The persisted census keeps multiplicity by class plus a digest
of the ordered headers; tracked semantic GUIDs live in the inventory and duplicate/missing GUIDs
live in typed issues, so snapshots do not duplicate every unknown GUID. Detect duplicates across
recognized and unknown classes, keep the first recognized record for continued measurement, and
emit one fatal import issue naming every duplicate occurrence. A tracked record with a missing or
empty GUID is likewise fatal because no stable identity exists. Unknown classes contribute to the
census but not to `authored` or legacy warning counts.

The identity constructors in `pg-snapshot` are the only way to create attachment, expansion, or
setting identities; callers do not concatenate strings. An inventory atom has identical
granularity in every lifecycle set. A parent object is not `authored` while only child expansions
are `represented`: the owner publishes the expected child atoms before selection. This covers
environment attachments, exocentric compound output roles, circumfix cross-product members, and
other one-to-many constructs without a false notion of "partly represented." Nested source GUIDs
that disappear from the snapshot model are retained in the attachment/expansion lineage at their
extraction owner.

For every class in `ALLOWED_CLASSES`, document whether it maps to a tracked semantic kind or is a
carrier record whose meaning is represented by tracked owned atoms. There is no implicit
"tracked" bucket. `authored` means the tracked atoms declared by the graph/owning collection;
`considered` means the owner evaluated that same atom; disabled or inactive means considered but
not selected; selected invalid means rejected and never represented. A semantics-changing
fallback records the source atom as rejected and the fallback atom as synthesized. An ordinary
owner-defined default with no source atom is synthesized only.

Do **not** implement a central `from_snapshot` or `collect_usage` walker. Factor or expose a small
`SelectionRecorder` interface and have each extraction/compiler owner record the identities it
actually considered and selected. This reuses the owner's active/disabled, writing-system,
morph-type, and representation decisions. Record `rejected` when a selected identity cannot be
represented, and add a pending representation only after the owner creates its snapshot or grammar object.
Thus intentionally unselected/disabled data is distinguishable from selected-but-rejected data.
Key environment representation by
`allomorph-guid/environment-guid`, because attachment is the semantic fact. Record HC defaults and
other compiler-created constructs in `synthesized`.

Where substrate needs a decision before final object construction, split the owning builder into
`prepare` and `build` without moving its predicate: `prepare` returns an owner-specific `Prepared*`
value containing the selected IDs/forms; `build` consumes that value. The recorder observes the
prepared value. The orchestration module may sequence these calls, but may not select candidates.
Compiler finalizers consume owner-published lineage and either finalize or revoke those pending
representation atoms after reachability, co-occurrence, and natural-class compaction. They never
rediscover lineage by scanning the completed grammar.

- [x] **Step 2: Preserve graph-to-snapshot provenance**

Extend `ImportReport` with typed issues and the graph-to-snapshot inventory. Fatal extractor issues
must not remain ordinary warnings. Store a versioned `conversion_provenance` value in `Snapshot` so
import-to-JSON-to-compile cannot erase prior loss. Old snapshots with no provenance deserialize as
`source_inventory_status = Unknown`; Task 2 records that state without changing compilation.
Task 4 makes production lossless conversion refuse unknown provenance unless a caller explicitly
requests measurement-only legacy input. Missing provenance never means clean import.
Add `#[serde(default)] pub conversion_provenance: ConversionProvenance` to the snapshot root.
Missing provenance deserializes as schema version 0 plus `Unknown`. Current constructors use the
published version-1 constant: `Snapshot::new` creates `Synthetic`, and `pg-fwdata` is the only
production owner that may replace it with an imported status. Named validation rejects or
downgrades unsupported versions and impossible pairs such as version 0/`ImportedComplete` or
version 2/`ImportedComplete`; an unrecognized version never means clean provenance.

Build `ConversionProvenance` once in `pg-fwdata`, clone that exact value into `Snapshot` and
`ImportReport`, and assert equality for direct `.fwdata`, `.fwbackup`, and JSON round trips. Typed
issues are authoritative; legacy warnings are produced through the same owner mapping rather than
a second decision branch.

Adding provenance changes `to_json()` but must not make diagnostics part of compiled-grammar cache
identity. Replace `grammar_hash()`'s documented "all `to_json()` bytes" contract with an exhaustive
borrowed semantic projection that excludes only `conversion_provenance`. Destructure every
semantic snapshot field so adding a future field requires updating the projection. Test that two
snapshots differing only in provenance hash equally, every semantic-field mutation changes the
hash, and old missing-provenance JSON hashes the same after reserialization. Update both stats
documents named in this task; this is an intentional one-time digest migration, not an incidental
implementation detail.

The compiler merges graph-to-snapshot and snapshot-to-grammar identities and computes these set
differences: `authored - considered` (no owner classified it), `considered - selected` (the owner
left it inactive), `selected - represented - rejected` (silent loss), selected `rejected`
identities (explicit refusal), and synthesized identities. Do not infer decisions from warning or
error strings.

Audit `extract::Ctx::require` and every warning/skip in `pg-fwdata`. Add paired fixtures proving an
unresolved selected reference is recorded as fatal with its owning GUID, while an unselected stale
record is nonfatal. Cover at least lexicon/allomorph, MSA, environment, natural class, template/slot,
co-occurrence, compound, and phonological/affix-process extraction. This is the import half of the
lossless gate; Task 6 covers compiler-side loss.

- [ ] **Step 3: Expose measurement without changing behavior**

```rust
pub fn compile_project_measured(
    snapshot: &Snapshot,
) -> Result<(Grammar, Vec<String>, InventoryDelta), GrammarError>;
```

`compile_project` temporarily discards only the returned inventory. Task 4 folds it into
`CompileOutput`; it does not rebuild the measurement.

- [ ] **Step 4: Write and record the bidirectional gate**

Import each checked-in `.fwdata`/`.fwbackup` fixture afresh, compile it, and include one
serialize/reload round trip. Print the graph, snapshot, and grammar inventories and composed delta.
Assert dated `NoMoreThan` constants for every non-zero unclassified, silently omitted, rejected,
and synthesis category. The gate
asserts `compared > 0`, `authored > 0`, known import provenance, and coverage of every supported
inventory family. A missing fixture directory or empty census is failure.

```powershell
& .\rust\tools\pg.ps1 -Mode test -Package pg-grammar -TestTarget conversion_inventory_gate
& .\rust\tools\pg.ps1 -Mode check -Package pg-fwdata
& .\rust\tools\pg.ps1 -Mode check -Package pg-grammar
```

- [ ] **Step 5: Commit**

```powershell
git add rust/crates/pg-snapshot rust/crates/pg-fwdata rust/crates/pg-grammar
git commit -m "gates: measure FieldWorks conversion loss end to end"
```

## Task 3: Establish canonical source fixtures, phoneme mutations, and the executable XAMPLE baseline

> **Deviation 2026-09-05 — pilot fixture changed.** `prefixal-discontinuous-slot-dependency` cannot be
> backed out of FieldWorks: `HCLoader` assigns output MPR features only to derivational affixes
> (`LoadDerivAffixProcessRule`), while `mrModeTrans` is a template-slot affix that *sets*
> `mprHighTrans`, and `mrSubj` gates per subrule where FieldWorks gates per MSA. A scan of all 21
> `requires: []` fixtures against Machine's `fieldworks-producibility.tsv` `producible=No` rows plus
> three semantic checks (slot rule setting an MPR feature, per-subrule `requiredMPRFeatures`,
> `type="require"` co-occurrence — FieldWorks ad hoc rules are exclude-only) leaves sixteen
> producible fixtures. The pilot is `edge-cases/deep-optional-affix-nesting`: one template of twelve
> optional slots, `ncAny` as its only natural class, so both phonemes are unreferenced and the
> empty-inventory case's `inferred_segments` is `[x, k]`. Its C(12,k) analyses make it the natural
> containment probe as well: author the project's `<XAmple>` block with `MaxPrefixes` and
> `MaxAnalysesToReturn` high enough that the comparator's baseline is not capped, and record the cap
> status. Task 8's expansion candidates, in order: `diacritic-segments`, `disjunctive-recheck`,
> `free-fluctuating-allomorph-pair`, `strrep-identity`, `stem-name-restricted-root-allomorph`,
> `mpr-overwrite-order-dependence`, `mpr-group-overwrite-without-realizational`, `compounding-breadth`,
> `truncate-morphotactic`, `loader-isactive`, `loader-pattern-shapes`,
> `bistratal-overlapping-segment-representation`, `cross-table-root-respelling`,
> `process-morphology-in-place-mutation`. Not producible: `prefixal-discontinuous-slot-dependency`,
> `suffixing-evidential-adjacency-chain`, `fusional-realizational-morphology`,
> `morphotactic-attribute-breadth`, `loader-isactive-breadth`, `feature-gating-breadth`. Read every
> `prefixal-discontinuous-slot-dependency` path below as the pilot's.

**Files:**

- Modify in the Machine submodule: `machine/conformance/PROTOCOL.md`
- Create in the Machine submodule:
  `machine/conformance/languages/prefixal-discontinuous-slot-dependency/fieldworks/project.fwdata`
- Create in the Machine submodule:
  `machine/conformance/languages/prefixal-discontinuous-slot-dependency/fieldworks/phonology-mutations.yaml`
- Create: `rust/crates/pg-xample-oracle/Cargo.toml`
- Create: `rust/crates/pg-xample-oracle/src/{lib,fieldworks,fixture,ffi,result}.rs`
- Create: `rust/crates/pg-xample-oracle/src/bin/xample-oracle.rs`
- Modify: `rust/crates/pg-conformance-fixtures/src/lib.rs`
- Modify: `rust/crates/pg-cli/tests/fwdata_grammar_equivalence_gate.rs`
- Create: `tools/xample-projector/XampleProjector.csproj`, `Program.cs`, `build.ps1`, and
  `testdata/captured-response.json` — version-pinned managed FieldWorks project
  clone/mutation/HCLoader/XAMPLE-transform helper plus portable contract self-test.
- Modify: `rust/Cargo.toml`
- Create: `rust/crates/pg-parse/tests/xample_migration_differential_gate.rs`

This differential must produce a non-empty baseline before Tasks 5 and 6 change substrate or
refusal behavior. Task 2's structural inventory is necessary but not a substitute. Machine owns
the source evidence; all executable mutation and comparison code in this task belongs to PanGloss.

- [ ] **Step 1: Add one canonical Machine fixture source and a declarative mutation contract**

Do this work on a Machine branch first. Extend `conformance/PROTOCOL.md` without adding a Machine
runtime dependency: an eligible fixture may have `fieldworks/project.fwdata`, any project-side
writing-system files required to open it, and `fieldworks/phonology-mutations.yaml`. Machine does
not parse or execute these files. PanGloss consumes them through the pinned `machine` gitlink.

Back out a minimal synthetic FieldWorks project for
`languages/prefixal-discontinuous-slot-dependency`. Use supported LibLCM/FieldWorks authoring and
save paths; do not generate raw `.fwdata` XML. Its official HCLoader projection must be semantically
equivalent to the checked-in `grammar.xml`, and both must reproduce every existing `words.yaml`
analysis before the project is accepted.

Check in one project only. The mutation manifest is:

```yaml
version: 1
cases:
  - id: empty-phoneme-inventory
    operations:
      - remove_all_phonemes:
          require_unreferenced: true
    expect:
      xample_projection: same_as_base
      hc_analyses: same_as_base
      inferred_segments: [a, i, o, b, d, g, h, k, l, n, s, t, w, y]
```

After saving the project, run
`(Get-FileHash -Algorithm SHA256 fieldworks/project.fwdata).Hash.ToLowerInvariant()` and add its
exact output as the manifest's required `base_sha256` field. The inferred-segment list above comes
from the checked-in HC grammar and must also equal the representations the helper reports deleting.
Specific partial-removal cases use `remove_phoneme` with the exact stable GUID printed by the
project-inspection command, an `assert_representations` list copied from that same object, and
`require_unreferenced: true`.

V1 has only `remove_phoneme` and `remove_all_phonemes`. It has no representation-only selector,
free-form query, inferred `unused` selector, or fixture-local executable script. The manifest
asserts relationships to the unmodified baseline and does not duplicate `words.yaml` expectations.

- [ ] **Step 2: Define one source projection contract**

The comparator takes a `.fwdata`/`.fwbackup` path plus test words. It launches a version-pinned,
PanGloss-owned helper against the installed FieldWorks assemblies. The helper can project the
unmodified source, or it can clone the complete `fieldworks/` project directory to a fresh
temporary directory, apply one already-resolved typed mutation through LibLCM, save and reopen the
clone, then project it. The helper calls the real HCLoader and `M3ToXAmpleTransformer`; it writes HC
XML and XAMPLE control, dictionary, and grammar files plus a result manifest to another fresh
temporary directory. The HC and XAMPLE sides consume the **same source path** for a run. There is
no hand-written `Snapshot`-to-XAMPLE emitter and no persisted mutated project.

Define the helper JSON request/response schema and check in a captured-response test. The response
contains FieldWorks version, source SHA-256, generated-file digests, diagnostics, and generated
paths. A source-digest mismatch is a hard comparison failure. First inspect the installed
FieldWorks assembly metadata and its ParserCore project files. Set the helper `TargetFramework`,
platform, and explicit assembly references to those observed values; record the constructor/method
signatures and assembly version in `fieldworks.rs` and the research document.

`build.ps1 -Mode check|test` is the stable repository entry point. It invokes `dotnet build` for an
SDK-style target or the discovered Visual Studio MSBuild for a .NET Framework target, and
fails with the missing tool/reference named. `-Mode test` builds the helper and runs
`XampleProjector --validate-capture testdata/captured-response.json`, which validates the JSON
schema, digests, diagnostics, and generated-file manifest without requiring FieldWorks at runtime.
The live test separately opens the checked Machine fixture and asserts that HCLoader and the
XAMPLE transform emitted every required file. Before adding another comparison, extract the
existing semantic `Grammar` canonicalizer and both-direction multiset diff from
`pg-cli/tests/fwdata_grammar_equivalence_gate.rs` into `pg-conformance-fixtures`; make the existing
gate and this comparator call that single owner. Extend that owner when the pilot reaches a
previously uncovered construct rather than recreating equivalence from generated XML. Compare the
HCLoader baseline structurally through that shared seam and behaviorally through the shared
`words.yaml` replay; do not require byte identity for generated IDs, element ordering, formatting,
or comments.

- [ ] **Step 3: Materialize each mutation fail-closed through LibLCM**

Parse `phonology-mutations.yaml` in `pg-xample-oracle::fixture`, verify `base_sha256`, and send only
typed operations to the managed helper's versioned JSON interface. The managed result contains:

```rust
pub struct MutationResult {
    pub case_id: String,
    pub base_sha256: String,
    pub materialized_sha256: String,
    pub removed: Vec<RemovedPhoneme>,
    pub inbound_references: Vec<SourceReference>,
    pub reopened: bool,
}

pub struct RemovedPhoneme {
    pub guid: String,
    pub representations: Vec<String>,
}
```

Before deletion, resolve every specific GUID exactly once and compare its default-vernacular
representations with `assert_representations`. Ask the LibLCM owner/reference machinery for inbound
references; do not reconstruct reference rules from XML tags. With `require_unreferenced: true`,
any retained segment natural class, simple phoneme context, feature structure, feature constraint,
or phonological rule reference refuses the mutation with `mutation.referenced-phoneme` and names
the owner. A source mismatch, zero or duplicate targets, assertion mismatch, zero deletions,
save/reopen failure, or source-project modification is a hard error. The response must prove the
requested deletion count and `reopened == true`; a diagnostic claiming success is insufficient.

Hash the canonical source before and after the live test and assert it is unchanged. Run the same
case twice and compare the generated XAMPLE file digests and mutation ledger. Do not require raw
temporary `.fwdata` bytes to match: FieldWorks persistence metadata may legitimately change.

- [ ] **Step 4: Add runtime-only DLL discovery and exact bindings**

Use `libloading`; do not link `xample64.dll` at build time. Bind the exported functions and calling
convention from the installed header/wrapper, including `AmpleCreateSetup`, `AmpleDeleteSetup`,
`AmpleLoadControlFiles`, `AmpleLoadDictionary`, `AmpleLoadGrammarFile`, `AmpleSetParameter`, and
`AmpleParseText`. Wrap the setup pointer in RAII and copy returned strings before the next call.
Unit-test symbol/result parsing against captured engine output.

- [ ] **Step 5: Define comparable results precisely**

```rust
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AnalysisSignature {
    pub morphemes: Vec<String>,
    pub msa_ids: Vec<String>,
    pub category_id: Option<String>,
    pub surface_nfd: String,
}

pub struct XampleResult {
    pub analyses: BTreeMap<AnalysisSignature, usize>,
    pub reached_max_analyses: Option<usize>,
    pub engine_error: Option<String>,
}
```

All identity fields are stable LCM GUIDs. The map is a multiset: its value preserves the number of
indistinguishable derivations because duplicate analyses are a real differential signal. Normalize
HC analyses through `pg-parse`; never compare display strings or collapse to a set. Declare the
exact crate/dev-dependency edges. A capped XAMPLE result is recorded but is not a complete set.

- [ ] **Step 6: Run the pre-change baseline and mutation gates**

Use the checked-in Machine FieldWorks fixture with its non-empty lexicon and accepted test words.
Increment `compared` only after transformation succeeds, `engine_error == None`, the result is not
containment-capped, and at least one word ran. Print `XAMPLE_ONLY` and `HC_ONLY`; ratchet both with
dated, fixture-specific `NoMoreThan` constants.

Run the unmodified project first and retain its generated HC/XAMPLE digests and exact normalized
analysis multisets in memory. For `empty-phoneme-inventory`, require byte-identical XAMPLE control,
dictionary, and grammar files; exact XAMPLE and completed-HC analysis multisets equal to baseline;
and a substrate report whose featureless inferred segments equal the manifest's
`inferred_segments`. Raw HCLoader XML after deletion is expected to lack those character
definitions and is not byte-compared with the baseline. In the managed helper tests, create a
minimal temporary LibLCM project containing one phoneme referenced by a segment natural class;
request its removal and assert `mutation.referenced-phoneme` before either projector runs. This
proves that "remove all" cannot silently remove authored phonological meaning without adding a
second checked-in project to the pilot.

An active phonological rule is valid comparator input: the real transformer decides not to export
it while HC executes it. This is measured behavior, not `OutsideSubset`. Portable CI may explicitly
skip the live run when FieldWorks/XAMPLE is absent, but must exercise a checked captured-result
parser with source/generated-file digests. The configured research-machine acceptance run may not
skip.

- [ ] **Step 7: Check and commit in ownership order**

```powershell
& .\tools\xample-projector\build.ps1 -Mode test
& .\rust\tools\pg.ps1 -Mode check -Package pg-xample-oracle
& .\rust\tools\pg.ps1 -Mode test -Package pg-parse -TestTarget xample_migration_differential_gate
```

Expected on the research machine: PASS with `compared > 0`. Commit fixture names, versions, hashes,
both baseline counts, and the mutation ledger in the test documentation. First commit the fixture
data and protocol text to its Machine branch. Then update PanGloss's `machine` gitlink and commit the
PanGloss runner/helper changes; do not copy the source project into PanGloss.

```powershell
git -C machine add conformance/PROTOCOL.md conformance/languages/prefixal-discontinuous-slot-dependency/fieldworks
git -C machine commit -m "conformance: add FieldWorks source for XAMPLE comparison"
git add machine tools/xample-projector rust/Cargo.toml rust/crates/pg-xample-oracle rust/crates/pg-conformance-fixtures rust/crates/pg-cli/tests/fwdata_grammar_equivalence_gate.rs rust/crates/pg-parse/tests/xample_migration_differential_gate.rs
git commit -m "oracle: establish pre-change XAMPLE migration baseline"
```

## Task 4: Introduce orthogonal compile policies and typed issues

**Files:**

- Modify: `rust/crates/pg-snapshot/src/conversion.rs`
- Create: `rust/crates/pg-grammar/src/compile/options.rs`
- Create: `rust/crates/pg-grammar/src/compile/issues.rs`
- Modify: `rust/crates/pg-grammar/src/compile/mod.rs`
- Modify: `rust/crates/pg-grammar/src/lib.rs`
- Modify: `rust/crates/pg-grammar/src/compile/tests.rs`

- [ ] **Step 1: Write option-resolution tests**

```rust
#[test]
fn auto_completes_xample_authored_or_explicitly_accepted_unspecified_graphemes() {
    assert_eq!(
        SubstratePolicy::Auto.resolve(ActiveParser::XAmple, false),
        ResolvedSubstratePolicy::CompleteFromUsage
    );
    assert_eq!(
        SubstratePolicy::Auto.resolve(ActiveParser::Hc, true),
        ResolvedSubstratePolicy::CompleteFromUsage
    );
    assert_eq!(
        SubstratePolicy::Auto.resolve(ActiveParser::Hc, false),
        ResolvedSubstratePolicy::Strict
    );
}
```

Also assert that neither options nor outputs contain `ParserProfile` or XAMPLE cap state.

- [ ] **Step 2: Define policies**

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SubstratePolicy {
    #[default]
    Auto,
    Strict,
    CompleteFromUsage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedSubstratePolicy {
    Strict,
    CompleteFromUsage,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SemanticLossPolicy {
    #[default]
    Refuse,
    MeasureOnly,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompileOptions {
    pub substrate: SubstratePolicy,
    pub semantic_loss: SemanticLossPolicy,
}
```

`MeasureOnly` exists only for the structural inventory gate. Production CLI/worker and XAMPLE
result-comparator HC callers use `Refuse`.

- [ ] **Step 3: Define typed issues and output**

Use the serializable `IssueClass`, `SourceRef`, and `ConversionIssue` types introduced in Task 2.
Add the compiler/substrate result types:

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SubstrateReport {
    pub inferred_segments: Vec<InferredChar>,
    pub inferred_boundaries: Vec<InferredChar>,
    pub unresolved_uses: Vec<SourceRef>,
    pub ambiguous_uses: Vec<SourceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferredChar {
    pub representation: String,
    pub kind: CharDefKind,
    pub evidence: InferenceEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferenceEvidence {
    LdmlExemplar,
    AuthoredBoundary,
    SafeBoundaryTable { version: u16 },
}

#[derive(Debug)]
pub struct CompileOutput {
    pub grammar: Grammar,
    pub issues: Vec<ConversionIssue>,
    pub substrate: SubstrateReport,
    pub inventory: InventoryDelta,
}

#[derive(Debug, thiserror::Error)]
#[error("FieldWorks project cannot be converted to HC without semantic loss")]
pub struct ConversionError {
    pub issues: Vec<ConversionIssue>,
    pub substrate: SubstrateReport,
}
```

Define `IssueClass`, `SourceRef`, and `ConversionIssue` in `pg-snapshot::conversion` with serde so
`pg-fwdata` can populate them and snapshot JSON can preserve them without depending upward on
`pg-grammar`. Compiler-specific constructors/codes and `ConversionError` remain in
`pg-grammar::compile::issues`. Add the conversion variant to `GrammarError` and expose issues for
callers/tests:

```rust
#[error(transparent)]
Conversion(#[from] ConversionError),

impl GrammarError {
    pub fn issues(&self) -> &[ConversionIssue] {
        match self {
            GrammarError::Conversion(e) => &e.issues,
            _ => &[],
        }
    }
}
```

`InferredChar` deliberately has no feature-value field. The substrate unit test inspects the
generated `RawCharDef` and proves its `feature_values` vector is empty.

- [ ] **Step 4: Add the orchestration API without changing behavior yet**

Add:

```rust
pub fn compile_project_with(
    snapshot: &Snapshot,
    options: CompileOptions,
) -> Result<CompileOutput, GrammarError>;
```

Initialize the issue collector and inventory from `snapshot.conversion_provenance` before invoking
any compiler owner. Never replace or summarize away graph-to-snapshot failures.

For this task, adapt existing string warnings into non-fatal `ConversionIssue`s and preserve existing
grammar output. At the end, when `semantic_loss == Refuse`, return
`GrammarError::Conversion(ConversionError)` if any collected issue has `fatal == true`.
An imported fatal issue or `source_inventory_status = Unknown` is fatal under `Refuse`; the latter
uses `conversion.source-provenance-unknown`. `MeasureOnly` retains both in the output and must never
be exposed by a production entry point.

Keep the existing tuple API temporarily so this task compiles independently:

```rust
pub fn compile_project(snapshot: &Snapshot) -> Result<(Grammar, Vec<String>), GrammarError> {
    let out = compile_project_with(snapshot, CompileOptions::default())?;
    let messages = out.issues.iter().map(|issue| issue.message.clone()).collect();
    Ok((out.grammar, messages))
}
```

Task 7 moves production callers to the structured output. The tuple wrapper remains only for
source compatibility and is deprecated after that audit.

- [ ] **Step 5: Run focused tests and check**

```powershell
& .\rust\tools\pg.ps1 -Mode quick -Package pg-grammar
& .\rust\tools\pg.ps1 -Mode check -Package pg-grammar
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add rust/crates/pg-grammar
git commit -m "grammar: typed lossless conversion policies and issues"
```

## Task 5: Complete the character substrate from owner-published usage

**Files:**

- Create: `rust/crates/pg-grammar/src/compile/substrate.rs`
- Modify: `rust/crates/pg-grammar/src/compile/chardef.rs`
- Modify: `rust/crates/pg-grammar/src/compile/environment.rs`
- Modify: `rust/crates/pg-grammar/src/compile/mod.rs`
- Modify: `rust/crates/pg-grammar/src/compile/tests.rs`

- [ ] **Step 1: Write failing substrate tests**

Extend the existing `fixture()` in `compile/tests.rs`; it already creates a complete snapshot and
exposes the stem allomorph at `[0].allomorphs[0]`. Add these complete tests (import
`ActiveParser`, `CompileOptions`, and `SubstratePolicy`):

```rust
#[test]
fn xample_authored_project_infers_missing_exemplar_segment() {
    let (mut snapshot, _) = fixture();
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::XAmple;
    snapshot.project.exemplar_characters.push("q".to_string());
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "quma")];

    let out = compile_project_with(&snapshot, CompileOptions::default()).expect("lossless compile");
    assert_eq!(out.substrate.inferred_segments.len(), 1);
    assert_eq!(out.substrate.inferred_segments[0].representation, "q");
    assert_eq!(out.grammar.entries[0].allomorphs.len(), 1);
    assert!(out.grammar.char_tables[0].lookup_nfd("q").is_some());
}

#[test]
fn strict_hc_project_refuses_the_same_missing_segment() {
    let (mut snapshot, _) = fixture();
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::Hc;
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "quma")];

    let err = compile_project_with(
        &snapshot,
        CompileOptions { substrate: SubstratePolicy::Strict, ..CompileOptions::default() },
    ).expect_err("strict compilation must refuse q");
    assert!(err.issues().iter().any(|i| {
        i.code == "conversion.unsegmentable-form" &&
        i.source.as_ref().is_some_and(|s| s.id == "allo-stem")
    }));
}

#[test]
fn ambiguous_symbol_without_ldml_refuses_instead_of_guessing_boundary_or_segment() {
    let (mut snapshot, _) = fixture();
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::XAmple;
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "ku§ma")];

    let err = compile_project_with(&snapshot, CompileOptions::default())
        .expect_err("symbol role is not authoritative without LDML");
    assert!(err.issues().iter().any(|i| i.code == "substrate.classification-ambiguous"));
}

#[test]
fn accept_unspecified_graphemes_changes_the_effect_not_just_the_message() {
    let (mut snapshot, _) = fixture();
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::Hc;
    snapshot.morphology.parser_parameters.accept_unspecified_graphemes = true;
    snapshot.project.exemplar_characters.push("q".to_string());
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "quma")];

    let out = compile_project_with(&snapshot, CompileOptions::default()).expect("flag must act");
    assert_eq!(out.grammar.entries[0].allomorphs.len(), 1);
    assert!(out.grammar.char_tables[0].lookup_nfd("q").is_some());
}

#[test]
fn inferred_segment_uses_the_same_semantics_as_an_authored_featureless_segment() {
    let (mut snapshot, _) = fixture();
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::XAmple;
    snapshot.project.exemplar_characters.push("q".to_string());
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "quma")];
    add_feature_based_rule_that_can_match_unspecified_q(&mut snapshot);

    let inferred = compile_project_with(&snapshot, CompileOptions::default())
        .expect("ordinary HC unspecified-feature semantics is defined");
    assert!(inferred.issues.iter().any(|i| {
        i.code == "migration.inferred-segment-with-feature-rule"
    }));

    let mut explicit_snapshot = snapshot.clone();
    add_explicit_featureless_segment(&mut explicit_snapshot, "q");
    let explicit = compile_project_with(&explicit_snapshot, CompileOptions::default()).unwrap();

    for word in ["quma", "kuma"] {
        assert_eq!(analyze_direct(&inferred.grammar, word), analyze_direct(&explicit.grammar, word));
        assert_eq!(analyze_fst_confirm(&inferred.grammar, word), analyze_fst_confirm(&explicit.grammar, word));
    }

    let mut valued_snapshot = snapshot;
    add_explicit_feature_valued_segment(&mut valued_snapshot, "q");
    let valued = compile_project_with(&valued_snapshot, CompileOptions::default()).unwrap();
    assert_ne!(
        synthesize_direct(&inferred.grammar, "stem-with-q"),
        synthesize_direct(&valued.grammar, "stem-with-q")
    );
}
```

Implement `add_feature_based_rule_that_can_match_unspecified_q` beside the existing phonological-rule
test helpers by reusing their code-constructed closed feature and rewrite rule. The helper must add
a rule whose structural description is the feature class and whose structural change changes the
surface, so the ambiguity is observable. Also add a tokenizer-owner unit test in `environment.rs`
asserting that `literal_text_elements("/[V]q_#")` returns only `q`, never `V`, `_`, or `#`.

Add this unit test inside `substrate.rs`, where the completed raw definitions are visible:

```rust
#[test]
fn inferred_segment_has_no_authored_feature_values() {
    let q = inferred_raw_def("q", CharDefKind::Segment);
    assert!(q.feature_values.is_empty());
}
```

Add paired direct-HC and FST assertions for the feature-rule fixture. Compare the inferred `q`
grammar with a control grammar containing an explicitly authored featureless `q`: their normalized
analyses must be identical for the `q` word and a word not containing `q`. Add a second control with
an explicitly feature-valued segment and assert the rule produces a different observable surface.
Together with `feature_values.is_empty()`, this proves ordinary unspecified-lane behavior and proves
that no feature value/class membership was invented; checking the issue message alone is
insufficient.

Also add boundary-classification cases: an ASCII space is inferred from
`SafeBoundaryTable { version: 1 }`, while apostrophe, modifier-letter apostrophe, hyphen-minus,
non-breaking hyphen, and `§` refuse without authored/LDML evidence. The classifier is a committed,
exact scalar allowlist (the Unicode-version-pinned `Zs` set plus ASCII tab/CR/LF in v1), not a
runtime punctuation/category predicate.

The tests assert analyses, compiled allomorph counts, table contents, and typed issues—not warning
strings.

- [ ] **Step 2: Expose literal tokens from the environment owner**

Factor `environment::tokenize` so the same token stream supports both pattern construction and a
new `environment::literal_text_elements(representation)`. The latter returns literal grapheme text
only; it excludes `_`, `#`, optionality syntax, and natural-class names. Do not add a second parser.

- [ ] **Step 3: Publish actual usage from its owners**

Extend the `SelectionRecorder` introduced in Task 2 with `record_text_use(SourceRef, &str)`. Each
owner's `prepare` function calls it only after making its normal active/disabled, morph-type, and
writing-system selection. `substrate::complete` consumes this published stream; it does not walk
`Snapshot` or reimplement `best_ws`/`ws_forms`. The later `build` function consumes the same
`Prepared*` value, so collection and compilation cannot disagree. An owner that selects a construct
but cannot expose its text emits `conversion.unsupported-construct`; it is not silently ignored.

- [ ] **Step 4: Implement iterative completion**

Use the same segmentation logic as the final `CharDefTable`. On failure, identify the Unicode text
element at the failure offset, classify it from the selected vernacular LDML exemplar set, authored
boundary data, or the exact versioned safe-boundary table, append one `RawCharDef`, and retry.
Deduplicate by NFD representation. Apostrophes, hyphens, joiners, and punctuation outside that safe
table are ambiguous without authored evidence. A collision or ambiguity becomes a fatal issue.

Define the ID from normalized Unicode scalar values, so it is deterministic without a hash or
process-dependent state:

```rust
fn inferred_id(nfd_representation: &str) -> String {
    let scalars = nfd_representation
        .chars()
        .map(|c| format!("{:x}", c as u32))
        .collect::<Vec<_>>()
        .join("-");
    format!("inferred:{scalars}")
}

fn inferred_raw_def(original_representation: &str, kind: CharDefKind) -> RawCharDef {
    let normalized_representation = nfd(original_representation);
    RawCharDef {
        xml_id: inferred_id(&normalized_representation),
        kind,
        representations: vec![original_representation.to_string()],
        feature_values: Vec::new(),
    }
}
```

Use that helper rather than rebuilding inferred definitions at call sites. The character table keeps
the observed representation and performs its existing NFD lookup normalization.

- [ ] **Step 5: Wire the seam**

In `compile_project_with`, order the phases:

```rust
let prepared = prepare_all_by_calling_each_owner(snapshot, &mut recorder, &mut issues)?;
let phon_features = features::build_phon_features(&prepared.features, &mut issues)?;
let raw = chardef::build_raw(snapshot, &phon_features, &mut issues)?;
let completed = substrate::complete(
    &prepared.text_uses,
    &snapshot.project.exemplar_characters,
    raw,
    resolved_substrate,
    &mut issues,
)?;
let chardef = chardef::finalize(completed.raw_defs, completed.indexes)?;
let natural_classes = natclass::build(
    &prepared.natural_classes,
    &phon_features,
    &chardef.phoneme_of,
    &mut issues,
)?;
```

The pseudocode names orchestration, not a new decision owner: each `Prepared*` type and selection
function stays in its existing owning module. Preserve one decision owner for collision detection
and table finalization.

- [ ] **Step 6: Run the focused gate and check**

```powershell
& .\rust\tools\pg.ps1 -Mode test -Package pg-grammar -TestTarget lossless_conversion_gate
& .\rust\tools\pg.ps1 -Mode check -Package pg-grammar
```

Expected: PASS; the inventory gate shows fewer omitted allomorphs and no regression in the opposite
direction.

- [ ] **Step 7: Commit**

```powershell
git add rust/crates/pg-grammar
git commit -m "grammar: complete featureless character substrate from authored usage"
```

## Task 6: Replace semantic warning-and-drop paths with refusal

**Files:**

- Modify: every owner under `rust/crates/pg-grammar/src/compile/` found by the audit
- Create: `rust/crates/pg-grammar/tests/lossless_conversion_gate.rs`
- Modify: `rust/crates/pg-grammar/tests/conversion_inventory_gate.rs`

- [ ] **Step 1: Generate and commit the live loss-site inventory in the test module docs**

Run:

```powershell
rg -n "unsupported:|skipped|treated as absent|unrestricted|ignored|fallback" rust/crates/pg-grammar/src/compile
```

Classify every hit as `SemanticsNeutral`, `MigrationDifference`, or one of the fatal
`IssueClass` values. The classification lives in the owning module's test, not in a prose-only list.

- [ ] **Step 2: Write failing owner-effect tests**

At minimum cover:

- unsegmentable root and affix allomorph;
- invalid/unresolved environment formerly treated as absent;
- unresolved natural class;
- bracket-pattern reduplication;
- metathesis;
- unsupported circumfix cross-product;
- unresolved template slot;
- unsupported custom strata;
- unresolved compound side or output;
- a phonological rule that fails to compile;
- a dangling co-occurrence reference whose omission changes validity.

Each test asserts `GrammarError::Conversion`, stable issue code, source GUID, and that no successful
`Grammar` escapes.

For every refusal family add a paired non-refusal control proving the owner's gate is exact:

- disabled or unreachable unsupported rules do not refuse merely because their record exists;
- an unresolved stale object that is not selected by any active construct is ignored with recorded
  provenance, while the same reference on an active construct refuses;
- an environment whose owner parser proves semantics-neutral succeeds, while widening or dropping
  a restriction refuses;
- a representable template, compound rule, metathesis rule, and co-occurrence constraint succeeds.

These controls must call the same owner function as the refusal case. Do not build a broader
preflight predicate or decide from candidate-set membership.

- [ ] **Step 3: Change owners, not callers**

Replace each `warnings.push(...); continue`, `Vec::new()` fallback, or unrestricted-environment
fallback with a typed issue emitted by the module that owns the decision. Do not inspect diagnostic
text in `compile/mod.rs` to decide whether compilation should fail.

- [ ] **Step 4: Preserve measurement mode without calling it success**

`SemanticLossPolicy::MeasureOnly` may finish a partial grammar solely so the inventory gate can
measure today's loss. Its `CompileOutput` must contain fatal issues, and no CLI/FST production caller
may request this policy.

- [ ] **Step 5: Run focused checks, then the package suite**

```powershell
& .\rust\tools\pg.ps1 -Mode check -Package pg-grammar
& .\rust\tools\pg.ps1 -Mode test -Package pg-grammar -TestTarget lossless_conversion_gate
& .\rust\tools\pg.ps1 -Mode quick -Package pg-grammar
```

Expected: PASS. Existing tests that expected warning-and-drop are rewritten to expect refusal or
explicit `MeasureOnly` output.

- [ ] **Step 6: Commit**

```powershell
git add rust/crates/pg-grammar
git commit -m "grammar: refuse semantic loss during FieldWorks conversion"
```

## Task 7: Wire one lossless compiler through CLI and FST callers

**Files:**

- Modify: `rust/crates/pg-cli/src/main.rs`
- Modify: `rust/crates/pg-foma/src/worker.rs`
- Modify: all `compile_project` callers returned by `rg -n "compile_project" rust/crates`
- Modify/Create: focused CLI and worker tests beside existing tests

- [ ] **Step 1: Add a substrate-only CLI option**

Expose `--character-substrate auto|strict|complete`. Do not add `--parser-profile`. `auto` remains
the default and resolves from `ActiveParser`/`AcceptUnspecifiedGraphemes` only at the import seam.

- [ ] **Step 2: Print grouped diagnostics**

On success print inferred substrate and non-fatal migration differences separately. On refusal,
print every fatal issue with stable code and source reference, then return a non-zero exit status.
Do not print `XAMPLE PROFILE` or imply that XAMPLE caps were enforced.

- [ ] **Step 3: Audit every caller explicitly**

Production FieldWorks callers use `CompileOptions { semantic_loss: Refuse, .. }`. Only
`conversion_inventory_gate` uses `MeasureOnly`. HC XML loading remains a separate already-strict
path and does not acquire snapshot substrate completion.

Audit import-only commands as well: if extraction produced a fatal conversion issue, a command may
write a diagnostic snapshot for inspection only when it labels it non-runnable, and must return a
non-zero status. It must not serialize a fatal report and announce a successful import. Reloading a
snapshot preserves and re-enforces the same fatal issue.

- [ ] **Step 4: Keep worker transport semantic-free**

If the FST worker needs an override, serialize only `characterSubstrate`. Do not serialize
`ActiveParser`, `ParserProfile`, or XAMPLE caps into backend compile requests. A successful worker
receives an already completed `Grammar` or runs the same lossless compiler.

- [ ] **Step 5: Run check first and tests last**

```powershell
& .\rust\tools\pg.ps1 -Mode check -Package pg-cli
& .\rust\tools\pg.ps1 -Mode check -Package pg-foma
& .\rust\tools\pg.ps1 -Mode quick -Package pg-cli
& .\rust\tools\pg.ps1 -Mode test -Package pg-foma -TestTarget five_language_backend_reports_gate
```

Expected: PASS. The worker round-trip test proves omitted `characterSubstrate` resolves to `auto`.

- [ ] **Step 6: Commit**

```powershell
git add rust/crates/pg-cli rust/crates/pg-foma
git commit -m "cli: route FieldWorks projects through lossless HC conversion"
```

## Task 8: Expand and harden the XAMPLE migration comparator

**Files:**

- Create in eligible Machine fixtures:
  `machine/conformance/{languages,edge-cases}/{fixture}/fieldworks/{project.fwdata,phonology-mutations.yaml}`
- Modify: `rust/crates/pg-xample-oracle/src/{fieldworks,ffi,result}.rs`
- Modify: `rust/crates/pg-xample-oracle/src/fixture.rs`
- Modify: `rust/crates/pg-parse/tests/xample_migration_differential_gate.rs`
- Create: `rust/crates/pg-parse/tests/data/xample-differential/*.json`

- [ ] **Step 1: Expand from the Task 3 baseline without changing its projection**

Add a canonical FieldWorks witness to eligible existing Machine fixtures one family at a time.
Attempt every phonology-free fixture, but refuse back-out when HCLoader cannot produce the fixture's
constructs or when its official projection cannot reproduce the existing HC analyses. Every live
comparison starts from the one fixture-owned `.fwdata` and uses the real FieldWorks transformer for
XAMPLE plus `pg-fwdata` for HC. Do not add a second hand-written emitter, a PanGloss copy of the
project, checked-in XAMPLE projections, or checked-in mutated `.fwdata` variants.

Add mutation cases only where their assertion is meaningful. Literal-only morphology may carry an
empty-inventory case. A fixture such as `suffixing-evidential-adjacency-chain`, whose vowel and
consonant segment classes select allomorphs, instead carries a specific referenced-phoneme request
whose expected result is `mutation.referenced-phoneme`; it must not claim result invariance.

- [ ] **Step 2: Harden result integrity**

Require `engine_error == None`, matching source digest, complete generated-file digests, and a
non-empty exercised-word list before incrementing `compared`. Keep capped observations in a
separate `containment_limited` collection. Verify setup destruction on success and each error path;
an unavailable symbol, failed transform, or malformed result must be an explicit error/skip reason,
never a zero-analysis success.

- [ ] **Step 3: Add both-direction explanations**

For every exercised fixture report `XAMPLE_ONLY`, `HC_ONLY`, normalized signature differences,
cap status, and migration categories. Stage both directions with `NoMoreThan` ratchets and a
reviewed per-word explanation file. Never raise a ratchet without updating that explanation.

- [ ] **Step 4: Run the expanded comparator gate**

```powershell
& .\rust\tools\pg.ps1 -Mode check -Package pg-xample-oracle
& .\rust\tools\pg.ps1 -Mode test -Package pg-parse -TestTarget xample_migration_differential_gate
```

Expected: PASS and `compared > 0` on the configured Windows research machine; portable CI either
runs live or validates captured manifests/results while explicitly reporting that they are replay.

- [ ] **Step 5: Commit**

```powershell
git -C machine add conformance
git -C machine commit -m "conformance: add FieldWorks witnesses and phoneme counterfactuals"
git add machine rust/crates/pg-xample-oracle rust/crates/pg-parse/tests
git commit -m "oracle: measure XAMPLE to HC migration differences"
```

## Task 8b: Make every conformance fixture FieldWorks-producible or deprecate it

**Decision (2026-09-05):** the conformance suite's coverage claim must hold from the surface PanGloss
ships — FieldWorks projects. A fixture that no FieldWorks user could produce proves conformance with
`hc.dll` and nothing about a grammar a user can hand us. For each fixture that the producibility scan
(Machine's `fieldworks-producibility.tsv` `producible=No` rows plus the three usage-level checks:
a template-slot rule setting MPR features, per-subrule `requiredMPRFeatures`, `type="require"`
co-occurrence) marks non-producible, do exactly one of:

1. **Convert** — re-author the same phenomenon with the mechanism FieldWorks actually uses, then
   re-derive `words.yaml` against the C# founding oracle and back the fixture out with `author`.
   Only when the phenomenon is meaningful from a FieldWorks perspective and the coverage survives.
2. **Deprecate** — mark it in the suite's manifest as HC-engine-only, spend no further FieldWorks
   work on it, and at the end of this plan either delete it (coverage fully duplicated by producible
   fixtures) or convert it (unique coverage worth keeping).

**Files:** each fixture's `words.yaml` front matter gains `fieldworks_producible: true|false` (plus
`fieldworks_producible_notes` naming the offending constructs when `false`), mirroring the existing
`requires:` convention — NOT `fixtures.csv`, which the triage found stale (25 rows against 36 fixture
directories) and which nothing re-derives. `PROTOCOL.md` states the policy;
`pg-conformance-fixtures::discover` exposes the field so PanGloss gates can report the two
populations separately and never let an `hc-only` fixture stand in for FieldWorks coverage.

**Triage outcome (Step 1, scratchpad `fixture-producibility-triage.md`).** Two cross-cutting
findings not in `fieldworks-producibility.tsv`: `RealizationalRule` is never constructed by HCLoader
(`HCLoader.cs:977` carries the TODO; four fixtures use it, zero producible witnesses exist), and
`MprFeatureGroup` is fixed at the three hardcoded groups with `Output` never set (custom groups and
`outputType="append"` unreachable). Per fixture:

| fixture | blocker | decision |
|---|---|---|
| `edge-cases/feature-gating-breadth` | `RealizationalRule` only | **CONVERT now** (rrPast → ordinary rule; 3 words oracle re-derived) |
| `edge-cases/morphotactic-attribute-breadth` | U1 + U3 + `RealizationalRule` + custom MPR groups | deprecate now; CONVERT at the end (~20/26 words survive) |
| `languages/fusional-realizational-morphology` | `family` + `RealizationalRule` + per-subrule compound MPR | deprecate now; CONVERT family-blocking third at the end (mpr-gated-exception pattern) |
| `languages/prefixal-discontinuous-slot-dependency` | U1 + U2 | **DEPRECATE** (premise is template-internal MPR dependency) |
| `languages/suffixing-evidential-adjacency-chain` | U3 ×4 | **DEPRECATE** (require-polarity has no FieldWorks path in principle) |
| `edge-cases/loader-isactive-breadth` | `isActive` on 12/13 kinds has no HCLoader analog | **DEPRECATE** (XmlLanguageLoader regression fixture) |

**Whole-corpus marking (Step 2, Machine branch `conformance/fieldworks-witnesses`, commits
`43af40e4` + `e2688c66`): 17 of 36 fixtures are `fieldworks_producible: false`**, not six. Checking
every grammar rather than the `requires: []` subset found four more non-producible classes:

| class | fixtures | HCLoader evidence |
|---|---|---|
| custom-named `MorphologicalPhonologicalRuleFeatureGroup` | `mpr-group-overwrite-without-realizational`, `mpr-overwrite-order-dependence` | only the three hardcoded groups (`HCLoader.cs:168-192`) |
| two `CharacterDefinitionTable`s across strata | `bistratal-overlapping-segment-representation`, `cross-table-root-respelling`, `rewrite-analysis-feature-neutralization`, `synthesis-stratum-render-stale-table` | one table per grammar reused by every stratum (`HCLoader.cs:204, 227-233, 374, 2669-2742`) |
| second inactive `PhonologicalFeatureSystem` | `loader-isactive` | one feature system per grammar (`HCLoader.cs:198`) |
| `isActive="no"` decoys on kinds with no `Disabled` filter | `compounding-breadth`, `feature-system-breadth` | one synthesized `CompoundingSubrule` per rule (`:1842-2001`); no filter on features/values/classes/entries |
| already known | `suffixing-extension-slot-ordering`, `loader-default-symbol` | `RealizationalRule`; `SymbolicFeature.defaultSymbol` never set (`:2650-2667`) |

`cross-table-root-respelling` is the fixture the FST backend's newest capability was named for; a
FieldWorks project cannot produce it. Decision for these eleven: deprecate now; the two-table
premise, the custom-group premise, and the `isActive` decoys have no FieldWorks re-authoring, so
they are delete-candidates at Step 4 unless the engine-only regression they pin is wanted by
Machine itself. `feature-gating-breadth` is converted and `true` (oracle self-check mode over all
36 fixtures; the three affected words' signatures unchanged). The Machine-side loader
(`WordsYamlLoader`, `words.schema.json`) had to learn the two keys because it refuses unknown
front-matter keys; a `false` verdict requires non-empty notes at parse time.

- [x] **Step 1: Triage** — per non-producible fixture: offending constructs with HCLoader citations,
  coverage claims, whether each claim is duplicated by a producible fixture, and whether a faithful
  FieldWorks re-authoring exists. Recommendation per fixture: convert / deprecate / delete-candidate.
- [x] **Step 2: Mark (Machine)** — front-matter field on all 36 upstream fixtures plus PROTOCOL §9,
  branch `conformance/fieldworks-witnesses` (`43af40e4`, follow-ups through `7a4ec947`).
- [ ] **Step 2 (PanGloss)** — `pg-conformance-fixtures` reads the field as
  `FieldworksProducibility { Producible, EngineOnly{notes}, Unmarked }`; the coverage gates and
  `pangloss coverage` print the three buckets; `producibility_marking_gate` ratchets `Unmarked`
  (62 today: 33 upstream at the current pin + 29 `conformance-staging/**`). Branch
  `research/xample-task8b` (`20bfc3d3`), under review.
- [ ] **Step 2b: Mark the 29 `conformance-staging/**` fixtures** with the same triage (they are
  this repo's own and were never scanned), then advance the `machine/` gitlink to the witness
  branch once it is pushed, so the ratchet can fall to 0.
- [ ] **Step 3: Convert** the fixtures the triage marks convertible, one at a time, each with oracle
  re-derivation and an `author` round trip proving producibility.
- [ ] **Step 4: Retire** — at the end, delete delete-candidates (with the duplicating fixture named
  in the commit) or convert the rest. No fixture stays `deprecated-hc-only` past the end of this plan.

## Task 9: Resolve known semantic questions with focused fixtures

**Files:**

- Create: focused fixtures under the repository's conformance-fixture location
- Modify: `rust/crates/pg-parse/tests/xample_migration_differential_gate.rs`
- Create: `docs/adr/000N-xample-comparator-jurisdiction.md`

- [ ] **Step 1: Pin allomorph ordering**

Create one entry where reversing `AlternateForms` and `LexemeForm` changes the winning analysis.
Run the real FieldWorks XAMPLE transform/DLL and HC. Change importer ordering only if the owner source
and observed result agree. The gate asserts behavior, not a comment about ordering.

- [ ] **Step 2: Pin default compounding**

Create no-authored-rule, one-authored-rule, and category-restricted compound fixtures. Determine
what the baseline XAMPLE `\cr W W` admits versus HC's left/right-headed defaults. Record the
difference; normal HC semantics remains the production policy.

- [ ] **Step 3: Pin expected authored-rule divergence**

Use the same project twice: one version with an active phonological rule and one without. Assert
XAMPLE results are unchanged while HC results change. This proves that retaining rules is deliberate
migration behavior, not accidental parity loss.

- [ ] **Step 4: Pin containment differences**

Exercise each XAMPLE positional cap and `MaxAnalysesToReturn`. Assert the comparator reports cap
status while the HC grammar and validity path contain no XAMPLE-cap predicate.

- [ ] **Step 5: Write the ADR**

Record:

- shared FieldWorks source, different engine projections;
- ordinary HC semantics as production authority;
- XAMPLE as a migration comparator only;
- typed transform/engine failure, with `OutsideSubset` limited to pinned-transformer or captured
  artifact infrastructure—not authored phonological rules the real transformer intentionally omits;
- both-direction differential ratchets;
- why containment values are not grammar validity.

- [ ] **Step 6: Run the local conformance scope**

```powershell
& .\rust\tools\pg.ps1 -Mode conformance-test -Scope local
```

Expected: PASS with at least one fixture compared by both engines.

- [ ] **Step 7: Commit**

```powershell
git add docs/adr rust
git commit -m "conformance: pin XAMPLE to HC migration boundaries"
```

## Task 10: Final verification and document synchronization

**Files:**

- Modify if evidence changed: `docs/research/xample-grammars-on-hc-rust.md`
- Modify if contract changed: `docs/superpowers/specs/2026-09-03-xample-shape-grammars.md`

- [ ] **Step 1: Search for superseded architecture in active docs and code**

```powershell
rg -n "XAMPLE PROFILE|ParserProfile|ProfileChoice|analysis_caps|caps-enforced|rules-dropped" rust docs `
  --glob '!docs/superpowers/plans/2026-09-03-xample-shape-2-*' `
  --glob '!docs/superpowers/plans/2026-09-03-xample-shape-3-*' `
  --glob '!docs/superpowers/plans/2026-09-03-xample-shape-4-*'
```

Expected: no active implementation or current-document reference **asserting** the superseded
design. Explanatory removal checks and the research decision history are reviewed, not treated as
violations. Plan 1 is intentionally included; only explicitly superseded Plans 2–4 are excluded.

- [ ] **Step 2: Run all-target checks before linking tests**

```powershell
& .\rust\tools\pg.ps1 -Mode check
```

Expected: PASS.

- [ ] **Step 3: Run focused conversion, CLI, FST, and comparator suites**

```powershell
& .\tools\xample-projector\build.ps1 -Mode test
& .\rust\tools\pg.ps1 -Mode test -Package pg-grammar
& .\rust\tools\pg.ps1 -Mode test -Package pg-cli
& .\rust\tools\pg.ps1 -Mode test -Package pg-parse -TestTarget xample_migration_differential_gate
& .\rust\tools\pg.ps1 -Mode test -Package pg-foma -TestTarget five_language_backend_reports_gate
```

Expected: PASS; record exact test counts in the implementation handoff.

- [ ] **Step 4: Review the differential effects**

Confirm from fresh output that:

- authored-to-compiled omissions decreased or became explicit refusals;
- no synthesis category increased without a documented reason;
- direct HC and FST-confirm use one completed grammar;
- XAMPLE-only and HC-only counts are both reported;
- a capped XAMPLE result is never called complete parity.

- [ ] **Step 5: Update the research document from measured evidence only**

Move resolved questions from §9 to the relevant verified section. Keep unresolved questions visible.
Do not turn a single fixture observation into a universal engine claim.

- [ ] **Step 6: Commit**

```powershell
git add docs rust
git commit -m "docs: record verified XAMPLE to HC conversion behavior"
```

## Self-review notes

- Spec §3 one HC path: Tasks 4, 7.
- Spec §4 substrate completion: Task 5.
- Spec §5 refusal: Task 6.
- Spec §6 expected differences: Tasks 3, 8–9.
- Spec §7.1 Machine/PanGloss ownership and temporary phoneme counterfactuals: Tasks 3, 8.
- Spec §8 differential before change and both directions: Tasks 2–3.
- XAMPLE caps are classified as resource containment and never placed on `Grammar`.
- No task drops authored rules, synthesizes NegSECs, recreates PC-PATR, or selects an XAMPLE runtime
  profile.
- Production conversion cannot return a partial grammar after a fatal issue; `MeasureOnly` is
  limited to the inventory gate.
- The plan contains no unresolved implementation placeholders. Questions requiring evidence are
  concrete oracle experiments in Task 9.

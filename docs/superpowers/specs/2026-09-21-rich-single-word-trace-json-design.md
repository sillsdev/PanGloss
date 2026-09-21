# Rich single-word trace JSON

PanGloss will expose the diagnostic data behind FieldWorks' Try a Word experience as structured
JSON. The output will preserve the parser's attempted paths, explain why each path stopped, and
measure the time spent in each detour. Existing parse and trace output will remain unchanged unless
the caller explicitly requests the richer format.

## Scope and authority

This change applies only to `pangloss parse`, which accepts exactly one word. The command gains an
explicit `--trace-details` flag. The flag requires both `--trace` and `--trace-format=json`; using it
without either condition is an error. `batch` will not accept the flag or synthesize rich traces.

The founding trace contract comes from Machine `a4b29742b6274a01c7398ca3e800a19fa6d2c9aa`.
Machine's base `Trace` stores node type, source, subrule, input/output words, and failure reason.
FieldWorks `bc316bca705ab30dd7ed22663c3b685ae221a0ac` adds the data shown by Try a Word: selected
allomorphs, morphemes, required and actual features, stem names, environments, production
restrictions, competing allomorphs, co-occurrence restrictions, and successful analyses.
PanGloss `37b3b42fa0687df12c6076ce22f23a55febc3793` already emits the base tree but reduces word
snapshots to surface strings and omits the parser's completion state.

The work is representational and diagnostic. It must not change candidate admission, rule order,
deduplication, parse identities, or the step budget. Every diagnostic value must come from the
module that already makes the corresponding decision. A renderer must never infer a failure detail
from diagnostic text, a candidate set, or a successful path.

Motif storage, commands, UI, and handoff formats are outside this design.

## Chosen interface

The supported invocation is:

```text
pangloss parse <grammar> <word> --trace --trace-format=json --trace-details
```

`--trace=<path>` continues to choose the JSON destination. Without `--trace-details`, the existing
tree-only JSON remains byte-compatible. Rich output uses a versioned envelope because completion,
analyses, aggregate counts, and whole-search timing describe the parse as a whole rather than one
trace node.

Two alternatives were rejected. Adding optional fields to the current root cannot represent
whole-search termination or analyses cleanly. A new `try-word` command would duplicate grammar
loading, option parsing, parse behavior, and output ownership already present in `parse`.

## JSON contract

The first version has this shape. Field order is illustrative; consumers bind by field name.

```json
{
  "schemaVersion": "pangloss.trace-details.v1",
  "word": "sagd",
  "search": {
    "termination": "completed",
    "completed": true,
    "steps": 17,
    "elapsedNs": 218430
  },
  "result": {
    "signature": "32+PAST",
    "analyses": []
  },
  "counts": {
    "traceNodes": 14,
    "ruleAttempts": 3,
    "successfulPaths": 1,
    "failedPaths": 1
  },
  "trace": {
    "type": "MorphologicalRuleSynthesis",
    "source": "ed_suffix",
    "sourceRef": {
      "kind": "morphologicalRule",
      "index": 0,
      "authoredId": "mrEd",
      "name": "ed_suffix"
    },
    "subrule": 0,
    "attemptedAllomorph": {
      "index": 0,
      "authoredId": "subEd",
      "morpheme": { "authoredId": "PAST" }
    },
    "input": {},
    "output": {},
    "failure": null,
    "timing": {
      "startedNs": 43120,
      "elapsedNs": 58170,
      "selfElapsedNs": 12740
    },
    "children": []
  }
}
```

`search.termination` is a closed value derived from the authoritative parse outcome:
`completed`, `stepCap`, `timeout`, or `invalidShape`. A completed search with zero analyses remains
`completed`; parse success and search completion are separate facts. `steps` is the parser's budget
counter, not a count of trace nodes.

`result.analyses` contains the successful structured analyses already produced by `pg-parse`, with
their ordered morpheme and allomorph references. The representation may share an internal builder
with other structured parse output, but the rich trace schema owns its serialized field names.

`counts.traceNodes` counts every node. `counts.ruleAttempts` counts morphological, phonological,
and compounding application or unapplication nodes, including failed attempts. The two counts must
never be presented as synonyms. Successful and failed path counts come from explicit terminal
nodes.

Every source reference states its kind, compiled-grammar index, optional authored identifier, and
display name when the grammar retains one. The compiled index is stable only for that compiled
grammar. PanGloss must not fabricate a FieldWorks HVO when the source format supplies none.

### Word snapshots

`input` and `output` are diagnostic snapshots rather than serialized Rust `Word` values. A snapshot
contains the rendered shape, stratum reference, ordered morphs, root reference, syntactic feature
structure, realizational feature structure, MPR features, obligatory syntactic features, and the
partial/final-template state needed to understand the attempt. Each morph contains its morpheme and
allomorph references, surface order, attribution status, and runtime-root provenance where present.

Snapshots omit implementation-only ownership links, cached source chains, alternative trees, and
reference-counting structure. The trace tree already records derivation ownership; serializing
those internals would duplicate the tree and expose unstable implementation details.

### Failure details

Each failed node retains the existing `failureReason` name and adds a typed `failure` object. The
object's `kind` identifies its payload. Payloads cover required versus actual syntactic features,
required or excluded MPR features, stem names, environments, production restrictions, the attempted
and competing allomorphs, allomorph and morpheme co-occurrence restrictions, surface mismatch,
template ordering, partial parse, bound root, and maximum application count.

The rule, validity, and environment modules will publish these facts at their existing refusal
seams. The trace layer records them, and the CLI renders them. The CLI may resolve grammar IDs to
names; it may not recompute why the refusal happened. A one-way debug assertion may verify that a
published detail agrees with its failure-reason category.

## Timing contract

Rich tracing measures time with Rust's monotonic `Instant`; it records no wall-clock timestamp.
Every node reports its start offset from the root, inclusive elapsed time, and self elapsed time.
Inclusive time measures the complete subtree—the cost of the detour a reader is examining. Self
time subtracts the non-overlapping direct-child intervals and shows work performed by the node
itself. The root's elapsed time also becomes `search.elapsedNs`.

The module that owns an attempted operation starts and stops its timer around that operation. The
renderer does not estimate durations from sibling order. Detailed timing is active only when the
caller supplies `--trace-details`; ordinary parsing and ordinary tracing do not read the clock or
maintain timing spans.

Nanoseconds are the integer transport unit so the schema does not lose short operations. They do
not promise nanosecond accuracy. Traced parsing disables equivalent-analysis merging and adds
instrumentation, so these values describe this diagnostic search. They are unsuitable as ordinary
parse benchmarks and should not be compared to batch timing as if both executed the same search.

If a process is killed externally, PanGloss cannot finish or serialize the envelope. If PanGloss
terminates the search itself through a step cap, timeout, or invalid shape, it writes the retained
tree and the matching non-complete termination value.

## Internal boundaries

`pg-rules` owns trace event types, typed failure details, diagnostic word snapshots, and optional
timing spans. Its no-op path stays allocation-free and clock-free. `pg-parse` owns search outcome,
budget steps, completion, successful analyses, and the root parse timer. `pg-cli` validates the
flag combination, resolves grammar references, builds the versioned envelope, and serializes JSON.

The rich serializer should use typed serializable records instead of extending the current
hand-written JSON concatenation. The existing compact renderer stays in place for the default
tree-only format.

## Verification

Tests will establish the contract in four layers:

1. CLI argument tests reject `--trace-details` without single-word JSON tracing and confirm that
   `batch` has no such mode.
2. Unit tests exercise typed failure payloads at the decision seams. Each test first fails because
   the detail is absent, then passes when that owner publishes it. Fixtures cover representative
   feature, environment, allomorph, co-occurrence, and application-cap failures from both analysis
   and synthesis.
3. A golden rich-JSON test verifies the envelope, structured analyses, source identities, snapshots,
   attempt counts, and terminal paths. Numeric timing values use structural assertions: nonnegative
   offsets, child intervals contained in parents, `selfElapsedNs <= elapsedNs`, and root elapsed time
   equal to search elapsed time.
4. Differential tests run the same word with ordinary parse, ordinary JSON trace, and rich JSON
   trace. Signatures and complete analysis-identity multisets must match. The ordinary JSON trace
   remains byte-for-byte unchanged. A small-cap test must report `stepCap` with `completed: false`
   and retain its partial tree rather than silently reporting completion.

Managed verification starts with focused `pg.ps1 -Mode check` and test-target runs, then uses
`pg.ps1 -Mode test` for the affected crates. Because instrumentation touches parse-path call sites,
the final gate includes `pg.ps1 -Mode conformance-test -Scope all` and before/after parse-multiset
comparison. Timing values are never golden numbers.

## Acceptance criteria

The feature is complete when the explicit flag emits valid versioned JSON for one word; the JSON
contains successful analyses, every retained attempted path, typed failure evidence, authoritative
completion and budget data, and inclusive/exclusive timing for each detour. Default parsing and
default tracing remain unchanged. No renderer re-derives a parser decision, no batch trace mode is
introduced, and all differential and managed gates report complete evidence.

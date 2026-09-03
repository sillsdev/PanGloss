# PanGloss project configuration, acknowledgements, and advice

Status: design draft for grilling; no implementation decision has been made.

## Finding

PanGloss does not currently have a unified project configuration. Configuration is split across
command-line flags, built-in policies, and report/package artifacts:

- `batch` and `parse` select `--engine=default|foma`;
- assessment selects `--pipeline=foma-confirm|hermitcrab`;
- `make-report` can load a standalone readiness `--policy` JSON file;
- FST backend preference is a fixed implementation policy;
- a capability override is durable only when embedded in a built `.pgpack` manifest.

These mechanisms do not provide a project-owned answer to either “which analysis route should this
project normally use?” or “which previously reviewed observations have we accepted for now?”

## Proposed separation

Use three separately versioned concepts. They may be presented together, but they should not share
one data model.

1. **Project configuration** selects named execution profiles and report policy.
2. **Acknowledgement ledger** records reviewed observations without changing the observations.
3. **Advice catalog** supplies unordered approaches that may be investigated, with conditions,
   tradeoffs, gotchas, and validation requirements.

The raw readiness, health, assessment, and profiling reports remain immutable evidence. A derived
attention report joins compatible evidence, attaches acknowledgement state, and selects applicable
advice.

## Proposed location

The current working proposal is a project-owned, version-controlled directory:

```text
.pangloss/
  project.toml
  acknowledgements.json
```

The configuration is discovered relative to the grammar/project source. An explicit `--project`
or `--config` path is required when automatic discovery would be ambiguous. Machine-specific
settings, if needed, belong in an uncommitted local layer and must not silently change the semantic
analysis pipeline.

An alternative is a visible root-level `pangloss.toml`. The unresolved product question is whether
discoverability or keeping project metadata together is more important. In either layout, the
acknowledgement ledger remains separate from execution configuration.

## Named execution profiles

The configuration should use `pipeline` for the end-to-end analysis route and reserve `backend` for
an implementation strategy inside a pipeline. This normalizes the current command-specific
`--engine` and `--pipeline` vocabularies.

Illustrative shape:

```toml
schema_version = 1
default_profile = "development"

[profiles.development]
pipeline = "foma-confirm"

[profiles.reference]
pipeline = "hermitcrab"

[reports]
acknowledgements = "acknowledgements.json"
advice_catalogs = ["builtin"]
```

Resolution should be explicit CLI values, then the selected project profile, then project defaults,
then built-in defaults. Every report must record the effective resolved configuration, its digest,
the selected pipeline/backend, and any CLI differences. There must be no silent backend fallback.

## Acknowledgement semantics

“I am okay with this” is not a capability override and does not make a finding pass. It changes only
the default attention view.

An acknowledgement records at least:

- its own stable ID;
- finding code, phase, metric, and typed affected construct keys;
- originating model fingerprint and pipeline/backend/profile;
- corpus or suite digest when the observation depends on test data;
- accepted value and severity;
- author, time, and rationale;
- scope and optional review/expiry date;
- explicit conditions that make it stale.

The derived state is one of `active`, `acknowledged`, `stale`, or `expired`. The default queue may
hide `acknowledged` items, but it must show their count and provide a “show acknowledged” view.
Original reports and findings are never deleted or rewritten.

An acknowledgement resurfaces when relevant evidence changes. Candidate triggers include a changed
model, profile, pipeline/backend, corpus/suite, construct set, finding identity, threshold policy,
worse severity, value beyond the accepted bound, or the review date. Missing or incomparable
context fails closed: the old acknowledgement is shown as stale rather than suppressing new
evidence.

Current `HealthFinding.affected` values are free-form strings and `HealthReport` lacks model,
corpus, and execution context. Reliable cross-run acknowledgement therefore depends on the same
`EvidenceContext` and typed `ConstructKey` work needed to join health, assessment, and rule-profile
evidence.

## Prefabbed linguistic advice

The existing health `Remedy` type is ranked “recommended first,” so it is not the right contract for
neutral linguistic advice. A separate `AdviceOption` catalog should be unordered and keyed by stable
finding code, metric, construct kind, and applicability predicates.

Each option should say:

- when it may apply;
- what could be investigated or changed;
- the possible computational effect;
- linguistic and engineering tradeoffs;
- gotchas and invalidating cases;
- whether linguistic equivalence must be checked;
- how to validate the result against assessment/golden and performance evidence;
- which catalog version supplied the text.

The presentation is “possible approaches to investigate,” not “recommended fix.” For example, a
high-cost affix rule might surface three structurally different options without ranking them:
constrain its applicability, split an interaction, or move work to a different pipeline stage. Each
would describe the linguistic assumptions it risks and the tests needed afterward.

Acknowledgements attach to findings, not advice options. The fact that a person considered or tried
one option may be included in the acknowledgement rationale, but does not make that option generally
correct.

## Report interaction

- **Health** contributes compiler/apply/backend cost observations.
- **Readiness** contributes policy and workload thresholds; acknowledgement never changes its
  underlying verdict.
- **Assessment and golden evidence** contribute completeness and correctness evidence; cases are
  never suppressed or reclassified by acknowledgement.
- **Rule profiling** contributes authored-rule and per-word work attribution when execution context
  matches.
- **Attention report** groups compatible observations by construct and analytical cohort, applies
  acknowledgement state, and attaches applicable advice options.

## Smallest coherent first release

1. Establish project configuration with named profiles and resolved-config provenance.
2. Establish shared evidence context and typed finding/construct identity.
3. Add a separate acknowledgement ledger and matcher.
4. Produce an attention view without mutating source reports.
5. Add a small built-in advice catalog for findings with reliable identities and evidence.

Do not begin with generic prose matching, permanent blanket suppression, or automatic grammar
rewrites. Those would make an old judgement silently apply to evidence it was never based on.

## Open decision for the grill

Should acknowledgements be shared project decisions committed beside the grammar, with optional
personal UI dismissals kept separate, or should acknowledgements be personal by default?

Recommended answer: shared project decisions. They are part of the grammar team's analytical
history and should be reviewable. A personal dismissal may reduce one person's UI noise, but must
not suppress a finding for the project or CI.

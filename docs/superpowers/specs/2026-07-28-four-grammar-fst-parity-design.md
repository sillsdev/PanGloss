# Four-grammar FST parity through recipe composition

## Status

Approved by the owner on 2026-07-28. This design closes the compiled/Foma path's four currently
refused Machine conformance grammars and graduates four missing construct witnesses into Machine's
canonical coverage. It defines parity as matching correct analyses. PanGloss must not reproduce
Machine's documented infinite-loop crash in `simultaneous-epenthesis-cascade`.

## Goal and definition of done

The complete Rust engine, compiled/Foma proposer-confirm engine, and Machine conformance oracle must
agree on every non-pathological expected analysis in the combined Machine and PanGloss-staging
fixture set. The compiled path must no longer refuse:

1. `languages/fusional-realizational-morphology`;
2. `languages/metathesis-phase-isolation`;
3. `languages/suffixing-extension-slot-ordering`;
4. `languages/templatic-root-modification`.

Machine's generated coverage must exercise all 28 in-scope construct rows, including the four
currently missing rows:

1. `CharacterDefinitionTable: more than one table, one per stratum`;
2. `RewriteRule direction (Dir): left-to-right`;
3. `RewriteRule direction (Dir): right-to-left`;
4. `RewriteSubruleDef gating: required/excluded POS or MPR at the subrule level`.

Completion additionally requires:

- every staged `grammar.xml` to be accepted by a standards-compliant XML reader and the canonical
  Machine conformance driver;
- the Machine self-check and parity check to pass;
- both PanGloss adapters to pass every correct-analysis fixture;
- the one pathological crash fixture to be reported as a documented semantic divergence rather
  than counted as a wrong-answer failure;
- full Rust tests, construct coverage, structural-witness liveness, Plan interaction coverage, and
  formatting checks to pass;
- no grammar-name-specific compiler branch.

## Design principles

PanGloss's executable Plan language remains closed: `Leaf`, `Compose`, `Union`, `Gate`, and
`Replace`. The missing behavior is expressed as reusable recipes over those primitives, not new
Plan node kinds or runtime tuning switches.

The FST is a proposer and HermitCrab is the confirmer. A recipe may over-propose, but it may not
drop an oracle analysis. Capability evaluation may move a configuration from `Refuse` to
`ConfirmOnly` only after a structural construction and proposer-to-oracle containment test exist.

Every implementation follows red-green-refactor. Each red test must fail at the current refusal or
missing-analysis boundary before production code changes.

## Recipe 1: structural-affix

### Purpose

Handle regular material-dropping, circumfix, and templatic output actions currently refusing
`fusional-realizational-morphology` and `templatic-root-modification`.

### Plan shape

```text
Compose(
  Leaf(templated-lexicon),
  Replace(structural-allomorph-relations)
)
```

### Construction

Generalize the existing Aweti `structural_allomorph` layer from its affine adjacent-tail recipe
into a bounded allomorph-local relation. For an allomorph whose input parts and output actions can
be represented regularly:

- preserve each input part's table-qualified pattern;
- assign an allomorph-owned opaque marker at lexc emission;
- compile the ordered `Copy`, `InsertSegments`, and supported finite `Modify` actions into one
  marker-consuming relation;
- compose the relation after templated lexc and before phonology;
- retain the existing literal/fallback alternative so widening remains recall-safe;
- guarantee that no structural marker survives the completed pipeline.

The compiler must classify support from the allomorph's structure, never its grammar name or
ordinal. Unsupported actions remain explicit capability witnesses until their own recipe lands.

### Evidence

Focused tests must identify the formerly refusing allomorphs, prove marker elimination, compare the
compiled candidate multiset with the complete engine for discriminating words, and demonstrate
that unrelated Aweti and circumfix fixtures are unchanged.

## Recipe 2: MPR overwrite partition

### Purpose

Implement exact history-dependent `MprGroupOutput::Overwrite` semantics for
`fusional-realizational-morphology` and `suffixing-extension-slot-ordering`.

### Plan shape

```text
Gate(
  Compose(
    Leaf(mpr-state-specific lexicon),
    Replace(mpr-state-legal morphology)
  )
)
```

### Construction

Treat the current MPR-group value as a finite build-time state. Derive the finite state set from the
grammar's declared group outputs and rule requirements. A transition:

- appends for `Append`;
- replaces the prior group value for `Overwrite`;
- is admitted only when its required/excluded MPR predicates match the incoming state;
- carries the resulting state into the next gated subtree.

Materialize only reachable states and transitions. Equivalent subtrees retain content-addressed
Plan identity. The external candidate tag sequence remains unchanged; MPR state is construction
metadata, not a new user-visible morpheme.

This changes `MprGroupOverwrite` from unconditional `FailClosed` to a configuration predicate:
finite, fully represented state machines become `ConfirmOnly`; malformed, unbounded, or
unrepresentable configurations remain `Refuse`.

### Evidence

Tests must pin overwrite-after-append, repeated overwrite, required/excluded gating after overwrite,
and the two real Machine grammars. A sabotage test must show that treating overwrite as append
causes a real parity failure.

## Recipe 3: expanded metathesis

### Purpose

Compile the complex metathesis pattern in `metathesis-phase-isolation` without weakening the
existing mirror-and-reverse semantics.

### Plan shape

```text
Compose(
  Leaf(morphology),
  Replace(metathesis-cascade)
)
```

### Construction

Extend metathesis lowering only for regular pattern forms whose candidate relation is finite or has
a native regular expression representation:

- table-qualified fixed and natural-class slots;
- finite and genuinely unbounded alpha-free quantifiers;
- boundary anchors where positional reversal preserves meaning;
- existing left-to-right and mirror-and-reverse right-to-left switch-index handling.

`slot_candidates` must no longer require eagerly enumerating a repeated slot. The metathesis
compiler should render regular slot expressions around the two switch regions and enumerate only
the finite switch candidate cross-product. Alpha-bearing or otherwise non-regular patterns remain
explicit refusals.

### Evidence

The real Machine grammar is the primary containment gate. Smaller tests independently pin
quantified middle context, anchors, direction reversal, and deliberately misaligned multi-table
symbols.

## Four construct witnesses and canonical fixture hygiene

The existing PanGloss staged witnesses already exercise the four missing constructs. Graduate
minimal, independently meaningful witnesses into Machine's `conformance/` tree or add equivalent
words to an existing Machine language when that is the cheaper canonical representation.

For every graduated witness:

- `grammar.xml` and `words.yaml` follow `machine/conformance/PROTOCOL.md`;
- `exercises` uses the exact `constructs.txt` spelling;
- the Machine oracle verifies authored signatures and traced rule identities;
- the PanGloss complete and compiled engines match those signatures;
- `coverage.csv` and `rules.csv` are regenerated by the canonical command.

Normalize all staged XML comments so no comment body contains `--` and no comment ends in `-`.
This is syntax-only and must not alter grammar semantics. Normalize the conformance shell scripts
to repository-supported LF endings so the documented command runs under the supported Bash.

## Parity semantics and pathological behavior

`edge-cases/simultaneous-epenthesis-cascade` records a Machine implementation bug: the C# oracle
throws an infinite-loop exception. PanGloss correctly terminates and produces the expected empty
analysis set. The conformance result model must distinguish:

- analysis parity;
- an expected reference-engine crash;
- an unexpected engine crash;
- a capability refusal.

The all-green aggregate for this project means every analysis expectation passes and the known
reference crash is classified as a documented divergence. It does not require PanGloss to crash.
No other failure or refusal may be filtered as known.

## Search and ranking integration

Each recipe receives a registry entry and grammar-specific binding containing:

- feature predicates and interaction edges;
- finite parameters and canonical Plan template;
- hard recall/capability/Plan constraints;
- build time, states, arcs, payload bytes, node reuse, and latency measurements;
- exact commands and cross-grammar results.

The initial bindings are selected by bounded constraint-guided enumeration: baseline, one recipe at
a time, and legal combined recipes. Correctness gates filter candidates before performance
ranking. Feasible candidates are ranked by Pareto dominance and the declared lexicographic policy:
recall, capability completeness, p95 proposal latency, wall-clock build, then payload bytes. This
work implements reusable bindings and evidence; it does not add a user-facing runtime optimizer.

## Error handling and budgets

- Invalid Plan shapes fail before FST construction.
- Any unsupported allomorph or metathesis node names the exact structural reason.
- MPR-state enumeration has a declared finite-state cap and returns a typed budget error before
  materialization.
- Candidate enumeration and confirmation retain existing path/candidate/compose budgets.
- A timeout or watchdog expiry is inconclusive evidence, never an empty analysis set.

## Verification sequence

1. Focused red-green tests for each recipe and each real formerly refused grammar.
2. Machine self-check with pathological fixtures.
3. Machine coverage generation and `parity-check.py`, requiring 28/28 in-scope rows.
4. Canonical driver over `machine/conformance` using PanGloss default and Foma adapters.
5. Canonical driver over `conformance-staging` using both adapters.
6. Combined in-process discovery over both roots, with zero compiled refusals.
7. Conformance coverage, citation liveness, exercise-tag liveness, structural witness, Plan
   interaction coverage, and readiness certification gates.
8. Full affected-crate and Rust workspace tests, formatting, diff check, and wasm check.
9. Recipe registry validation and evidence review.

Only after all commands pass may the implementation be committed to PanGloss `main`, the Machine
fixture/coverage commit be pushed to its PR branch, the submodule pointer be updated, and PanGloss
`main` be pushed.


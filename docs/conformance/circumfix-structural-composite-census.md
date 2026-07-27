# `CircumfixOutputAction` census — which circumfix-shaped allomorphs miss the structural-composite route

Closes the blocking item on `openspec/changes/plan-construct-coverage-completion` §D2 row 10 /
tasks.md 4.3: *"the first task is the census itself — the specific gap shapes are not yet named
anywhere in-repo."* This document names them.

Census performed 2026-07-25 by reading the selection chain end to end. Read-only; no code changed.

## The chain, and where the gap actually is

```
capability::CircumfixStructuralCompositePredicate
  → emit::is_structural_rule(g, mid)          emit.rs:1923-1938
      → emit::rule_role(g, mid)               emit.rs:555-560   ← THE GAP IS HERE
          → emit::classify_affix(&allo.rhs)   emit.rs:397-453
  → emit::structural_candidate_rules(g)       emit.rs:1975
  → emit::build_structural_composites(...)    emit.rs:2363
      → struct_extend → pg_rules::morph::synthesize(g, base_word, rule)   emit.rs:2272
```

**The mechanism is allomorph-complete; the candidate *selection* is not.** Once a rule reaches
`build_structural_composites`, every one of its allomorphs is covered, because `struct_extend`
delegates to the real confirm engine (`pg_rules::morph::synthesize`, `emit.rs:2272`) rather than
re-deriving anything from a role classification. So there is no overclaim risk inside the mechanism.

Every gap below is therefore a **candidate-selection** gap, and every one fails in the
**over-refusal** direction — a circumfix-shaped allomorph that misses selection is reported
`uncovered` by `emit_rule_allomorphs` (`emit.rs:1451-1456`, `role != zone_role` → an `UncoveredItem`
carrying `role.label()`), and the predicate reports `structural_composite_attempted == false` →
`Refuse`. That is honest and fail-closed, not silent recall loss. But each is a real hole, and each
fix is cheap.

## Named gap shapes

### C1 — `rule_role` classifies a rule by its FIRST allomorph only

`emit.rs:555-560`:

```rust
allomorphs_of(g, mid).first().map(|a| classify_affix(&a.rhs)).unwrap_or(Role::None)
```

A rule whose allomorph **0** is `Prefix`/`Suffix`/`None`/`Infix`/`Reduplication`/`Process` but whose
allomorph **1..n** is circumfix-shaped never gets `Role::CircumfixPrefix`, so `is_structural_rule`
falls into its `Role::None | Prefix | Suffix` arm — where the *only* route to `true` is
`rhs_drops_lhs_material` on some allomorph (`emit.rs:1893-1906`), a completely different structural
property. A circumfix allomorph that copies all of its LHS parts fails that test and the rule is
never a structural candidate.

Note the asymmetry: the *opposite* order is safe. If allomorph 0 IS circumfix-shaped, the whole rule
becomes a structural candidate (`emit.rs:1935`, unconditional `true`) and its ordinary
prefix/suffix siblings ride along — over-inclusion, which the mechanism handles correctly because it
delegates to the real engine.

**This is the highest-priority of the four** — it is the only one where a purely mechanical fix
(`any` instead of `first`) closes a whole class, and it is order-of-declaration-dependent, which
makes it the kind of gap that appears and disappears as a grammar author reorders allomorphs.

**Fix:** `is_structural_rule` should ask "does ANY allomorph of this rule classify as
`CircumfixPrefix`", not "does allomorph 0". `rule_role` itself has other callers
(`crate::peel`, `build_deriv_chain`, `emit_rule_allomorphs`) whose first-allomorph semantics may be
deliberate, so prefer adding an allomorph-wise helper over changing `rule_role`'s contract.

### C2 — `classify_affix`'s reduplication check preempts the circumfix check

`emit.rs:408-414` returns `Role::Reduplication` as soon as any `Copy` part repeats, **before** the
leading/trailing-insert test at `:441-453` ever runs. A circumfix that also reduplicates (leading
insert + repeated `Copy` + trailing insert) classifies `Reduplication` and is not a structural
candidate.

This one **legitimately interacts with row 11's own carve-out** (`Reduplication`'s peel-eligibility
split) and should not be fixed in isolation: whichever role wins determines which mechanism claims
the allomorph, and both mechanisms' recall arguments would have to be re-checked. Do not treat this
as mechanical.

### C3 — `classify_affix`'s infix check preempts the circumfix check

`emit.rs:434-440` returns `Role::Infix` as soon as a non-`Copy` action sits strictly between the
first and last `Copy`, **before** the leading/trailing test. An RHS that is *simultaneously*
circumfixing and infixing (leading insert, an interior insert between two copies, trailing insert)
classifies `Infix`, so it is routed to `crate::preexpand` (whose own module doc scopes it to
Infix/Prefix/Suffix) rather than to `build_structural_composites` — and `emit.rs`'s own comment at
`:1928-1934` states plainly that the concatenative model is "unconditionally unrepresentable" for a
morpheme wrapping the root on both sides.

**Fix:** test leading-AND-trailing insert *before* the interior-action test, i.e. let
`CircumfixPrefix` win over `Infix` when both hold. Needs its own recall argument (the mechanism is
strictly more general, so this should be safe, but it must be argued, not assumed).

### C4 — `Role::Process` short-circuit: checked, NOT a gap

`emit.rs:427-433` returns `Role::Process` when the RHS has a `Modify` and **no** `Copy` at all,
regardless of leading/trailing inserts. This is correct to exclude: with no `Copy` action there is no
copied root material for a circumfix to wrap, so the shape is not a circumfix in the sense
`build_structural_composites` constructs. Recorded so it is not re-diagnosed.

### C5 — `has_unemittable_action` on the ordinary path: checked, NOT a circumfix gap

`emit.rs:1442-1449` skips any allomorph whose RHS carries `Modify`/`InsertContext`, reporting
`kind: "process-morph"`. This is on `emit_rule_allomorphs` (the ordinary literal-lexc path), not on
the structural-composite path, and it is honest reporting rather than a silent skip. A
`CircumfixPrefix` rule reaching `build_structural_composites` is unaffected by it. Recorded so it is
not re-diagnosed.

## Status 2026-07-27 — C1, C2, and C3 are all BUILT

- **C1 — closed.** `is_structural_rule` now admits a rule when **any** allomorph classifies
  `Role::CircumfixPrefix`, not only allomorph 0. `rule_role`'s own contract was left alone, per this
  census's recommendation, because its other callers may want first-allomorph semantics. Pinned by
  `tests/circumfix_candidate_selection.rs::circumfix_allomorph_selection_is_order_independent` — the
  invariant the bug actually violated was order-*dependence*, so the test declares the same rule with
  its allomorphs in both orders and requires identical selection. Fixture:
  `conformance-staging/edge-cases/circumfix-non-first-allomorph-selection`.
- **C3 — closed.** `classify_affix`'s leading-AND-trailing test now runs **before** the
  interior-action test, so a simultaneously-circumfixing-and-infixing RHS classifies
  `CircumfixPrefix` rather than `Infix`. Two reasons were recorded rather than one: `Infix` is simply
  the wrong label regardless of downstream consequences, and `build_structural_composites`'
  `CircumfixPrefix` admission is unconditional whereas routing through `preexpand`'s `Infix` path
  would make the predicate refuse on grammars `preexpand` cannot serve. The mechanism hand-off was
  checked explicitly — `preexpand`'s candidate set selects on `rule_role` matching
  `Infix`/`Prefix`/`Suffix`, so it drops the rule cleanly rather than both mechanisms claiming or both
  dropping it (pinned by `circumfix_infix_ownership_handoff_is_clean`). Fixture:
  `conformance-staging/edge-cases/circumfix-infix-interior-action-precedence`.
- **C2 — RESOLVED 2026-07-27** (`plan-construct-coverage-completion` task 4.3c). Decided jointly with
  row 11's `Reduplication` carve-out, as flagged. **Reachability checked first**:
  `MorphologicalOutput` is declared `(CopyFromInput | InsertSimpleContext | ModifyFromInput |
  InsertSegments)*` (`HermitCrabInput.dtd:420`, an unconstrained repeated-choice group) and
  `pg_grammar::load` places no uniqueness constraint on `CopyFromInput`'s `index` (`load.rs:1896-
  1901`, existence-only check) — so an RHS that is simultaneously circumfix-shaped and
  reduplication-shaped is fully DTD-legal and loader-legal, not a vacuous case. **`CircumfixPrefix`
  now wins** (mirrors C3's own resolution), and unlike C3 this closes a REAL recall gap, not merely
  an ownership relabeling: `crate::peel::ReduplicationPeeler`'s four scan kinds (module doc:
  prefix-copy, suffix-copy, separator+tail-copy, separator+suffix-peel) are each a ONE-SIDED
  surface-string match, and none of them searches for a repeated span with independent material
  wrapping it on BOTH sides — so before this fix, an `AffixProcessRule` with this combined shape was
  claimed by the peel (peel-eligible by rule kind) but the peel's own scans could not actually
  recall the wrapped reduplicated surface. `build_structural_composites` handles it correctly
  instead, by the same "shape-agnostic replay of `pg_rules::morph::synthesize`" argument C1/C3
  already established (confirmed here too: `pg_rules::morph::synth_process_allomorph`'s own
  per-action loop over `allo.rhs`, plus that crate's "Tier-2 #8 (reduplication morph attribution)"
  handling of a repeated `Input` part). **Row 11's carve-out is unchanged and still faithful**:
  `crate::peel::is_reduplication_rule` excludes a `RealizationalRule` on rule KIND alone, checked
  BEFORE `classify_affix` is even consulted — orthogonal to C2's Role-shape distinction, which
  applies identically regardless of rule kind. **No code change was needed in `peel.rs`**:
  `is_reduplication_rule`'s own `.any()` scan already calls `classify_affix` per allomorph, so once
  that function stops returning `Role::Reduplication` for this combined shape, the peel relinquishes
  it automatically — mechanically identical to how C3 closed the `crate::preexpand` handoff by only
  touching `classify_affix`. Pinned by `tests/circumfix_candidate_selection.rs`'s C2 section:
  `circumfix_reduplication_recall_parity` (full proposer-to-confirm containment),
  `peel_relinquishes_circumfix_reduplication_cleanly` (`ReduplicationPeeler::has_redup_rules() ==
  false` for this grammar), and `c1_and_c3_selection_is_unperturbed_by_the_c2_fix` (C1/C3 re-verified
  unchanged). Fixture: `conformance-staging/edge-cases/circumfix-reduplication-precedence`.

## Verdict

Row 10's **PROVABLE** verdict stands, and the census sharpens it into three concrete work items of
very different sizes:

| Gap | Fix shape | Size | Independent? |
|---|---|---|---|
| **C1** first-allomorph-only selection | allomorph-wise `any` in `is_structural_rule` | small, mechanical | yes |
| **C3** infix preempts circumfix | reorder `classify_affix`'s tests; argue recall | small, needs an argument | yes |
| **C2** reduplication preempts circumfix | reorder `classify_affix`'s tests (redup after circumfix); `CircumfixPrefix` wins | small, needed its own joint argument | **RESOLVED 2026-07-27** — decided jointly with row 11, closes a real recall gap (not just relabeling) |
| C4, C5 | none needed | — | checked, not gaps |

Each of C1, C2, and C3 got its own fixture per the standard Stage-2 kit (a rule whose non-first
allomorph is circumfix-shaped; a simultaneously-circumfixing-and-reduplicating RHS; a
simultaneously-circumfixing-and-infixing RHS), all synthetic and delanguaged, ground truth derived
by running the engine. C2's own resolution is recorded above, jointly with row 11's `Reduplication`
carve-out boundary (re-checked there, found unchanged and still faithful to C#).

## Delanguaging leak found in passing

`emit.rs:1932-1933` still cites four actual-language example words in a comment
(`"keadilan"`/`"gelobt"`/`"gelobth"` and their morph segmentations) attached to two fixture names that
have themselves already been delanguaged (`metathesis-phase-isolation`,
`fusional-realizational-morphology`). Not touched here — `pg-foma` was in flight during this census —
but it should be swept: the fixture-name citations carry all the information the words were there to
provide.

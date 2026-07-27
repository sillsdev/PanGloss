# `MultiTable`'s shared-representation split — design

Plan for `openspec/changes/plan-construct-coverage-completion` §D2 row 13 / tasks.md 4.4, the one
PROVABLE row that had no plan at all. Row 13 flags itself as "larger in scope than the other
structural extensions (touches the shared token-space design) — flagging for explicit
prioritization, not asserting it is small."

Authored 2026-07-25 from a read of the token space, the predicate, and the confirm engine's own
class-matching. **The headline finding is that the fix the plan assumed is the wrong one, and the
right one is simpler.**

## What is actually refused today

`MultiTableFaithfulThreadingPredicate` returns `Refuse` when any two `CharacterDefinitionTable`s
share a normalized representation (`multi_table_detail`, `capability.rs:1055-1059`). So the
shared-representation case is honestly fail-closed, not silently miscompiled — this is a **coverage
gap, not a live bug**, which is why row 13 is PROVABLE rather than a defect.

The token space is one codepoint per char-def:

```rust
// replace.rs:259, :275-277
const PUA_BASE: u32 = 0xE000;
pub fn token(&self, cd: CharDefId) -> char {
    char::from_u32(PUA_BASE + cd.0).expect("char table too large for the PUA token scheme")
}
```

`cd.0` is a **per-table** index (`CharDefTable { defs: Vec<CharDef>, .. }`,
`pg-grammar/src/chardef.rs:119-136`), and `SegAlphabet` holds only `table: &CharDefTable` — it does
not know *which* table it is. So the token is a pure function of a raw index, table-blind. That is
exactly what `MultiTableFaithfulThreadingPredicate`'s own doc says (`capability.rs:1916-1917`).

## The plan's assumed fix would make things worse

Row 13 proposes "a PUA-style disjoint-token-range encoding across tables (assign each
`CharacterDefinitionTable` its own reserved token range rather than relying on natural
disjointness)". Trace what that does to the actual failure mode.

Take tables A and B that both spell a segment `s`, at index 3 in A and index 7 in B.

- **Today (table-blind index):** a root drawn from A's lexicon encodes `s` as `U+E003`. A
  table-B-resolved rule whose natural class contains `s` renders the atom `U+E007`. The atoms differ,
  so **the rule never fires on that root**, so any analysis requiring it is **never proposed**. Under
  the propose-and-confirm invariant that is the one unrecoverable error: the proposer may
  over-approximate freely, but an omission can never be recovered by confirm.
- **Under disjoint ranges:** A's `s` becomes `PUA + 0·STRIDE + 3`, B's becomes `PUA + 1·STRIDE + 7`.
  Still different. The non-match is now *guaranteed by construction* rather than incidental.

So disjoint ranges address the wrong direction. They would harden the proposer against a
**false-positive** risk (a table-B rule accidentally matching a table-A token that merely shares a
raw index), which is precisely the risk that propose-and-confirm already licenses and that
`pg_rules::rewrite` already prunes — the predicate's own `ConfirmOnly` rationale says so in as many
words. Meanwhile they entrench the **false-negative** risk, which is the one that actually matters.

## Why over-firing is safe here, and under-firing is not

`MultiTableFaithfulThreadingPredicate` lands at `ConfirmOnly`, never `Admit`: the compiled net is a
proposer, and `pg_rules::rewrite` — which resolves every rule's table via an explicit `TableId` with
no PUA collapsing at all — is the oracle that prunes. A proposer that fires a rule the real engine
would not fire costs candidates, which confirm discards. A proposer that fails to fire a rule the
real engine *would* fire loses the analysis outright.

This asymmetry is the whole design constraint, and it points the fix in the opposite direction from
range separation: the proposer should treat a shared spelling as **the same token in every table**,
deliberately over-approximating, and let confirm sort out whether the segments' feature bundles
actually license the match.

Supporting evidence that feature bundles, not char-def identity, are what the real engine matches
on: `pg_rules::bridge::nat_class_lanes` (`bridge.rs:260-300`) builds a natural class's match
criterion from **phonological feature lanes**, and its own doc records that `pg_fst::Segment`
"carries only phonological lanes, no char-def/`StrRep` dimension," naming char-def-level membership
discrimination as a known, separately-tracked gap. So cross-table *spelling* identity is not what the
oracle keys on either — one more reason the proposer should not try to enforce a distinction the
oracle does not make.

## The recommended fix: cross-table representation aliasing

Keep the token keyed by `(table, char-def)` as today. Add, at rule-rendering time only, an alias
union: **when a normalized representation appears in more than one table, every atom that renders
that char-def renders as a union over every table's token for that same spelling.**

For the A/B example, a table-B rule's atom for `s` renders `[U+E007 | U+E003]` instead of `U+E007`.
The rule now fires on A-originated material; the extra firings are over-approximation; confirm
prunes.

### Why this shape rather than the two obvious alternatives

- **Not "canonicalize the token by representation" (one global token per spelling).** A single
  char-def can legitimately carry SEVERAL representations (the reference-corpus case of one char-def
  spelled two ways is real, and `surface_variants` already takes the cartesian product of a
  char-def's representations). Keying tokens by representation would split one char-def into several
  tokens and would require every char-def-keyed site to become representation-keyed — a far wider
  change, and it would break the "one char-def, one atom" invariant the whole rendering path rests
  on.
- **Not "make `CharDefId` globally unique at load time".** That is a `pg-grammar/src/model.rs`
  shape change, which R1 freezes and `coverage_ledger.rs`'s own "A future model-shape change" note
  requires reopening the coverage audit to touch. The aliasing fix lives entirely inside `pg-foma`
  and needs no model change at all — a decisive advantage.

### Concrete work items

1. **A representation→`(TableId, CharDefId)` multimap**, built once per grammar (mirroring how
   `CharDefTable::lookup` already maps normalized representation → `CharDefId` within one table).
   Normalization must be the same NFD normalization `CharDefTable::lookup` and
   `emit::surface_variants` already use — a second normalization convention here would be its own
   bug class.
2. **Thread `TableId` into `SegAlphabet`.** It currently holds only `&CharDefTable` and so cannot
   name itself. This is the mechanical bulk of the change: every `SegAlphabet::new` call site must
   pass the table's own id. Prefer a constructor that takes both over a defaulted one — a
   defaulted `TableId` here would be the same class of mistake as the `char_tables[0]` implicit
   default that `owning_table` was introduced to remove.
3. **Alias-expand at render time**, in `lower::render_slots`' `Slot::Fixed`/`Slot::Union` arms —
   NOT in `class_members`. Keeping resolution per-table (which `owning_table` made correct) and
   applying aliasing only at the text-rendering boundary means the alias set is a rendering concern,
   not a semantic one, and no existing per-table resolution is disturbed.
4. **Query/shape encoding stays single-token.** `encode_shape`/`encode_query` encode a *concrete*
   shape whose char-defs come from one known table; they must not alias, or a query word would
   become ambiguous. Aliasing belongs only on the *pattern* side, which is what needs to match
   material from elsewhere.
5. **Flip the predicate's shared-representation arm** from `Refuse` to `ConfirmOnly`, and rewrite
   `MultiTableFaithfulThreadingPredicate`'s doc — including the "Why representation-disjointness is
   the proof obligation" section, whose stated rationale is about the false-positive direction and
   should be corrected to name the false-negative direction as the real one.
6. **Fixture:** a two-table grammar where the tables genuinely share a spelling AND a rule in the
   second table's stratum must fire on material spelled via the first — i.e. a fixture that FAILS
   (loses an analysis) under today's table-blind tokens and passes with aliasing. Synthetic,
   delanguaged, ground truth derived by running the engine. `bistratal-overlapping-segment-
   representation` already covers the *refusal* side; this is its recall-side counterpart.

### The proof obligation

A containment test in the standard Stage-2 shape: for a shared-representation two-table grammar,
every analysis `pg_parse::Morpher` finds must appear in the proposer's candidate set. Aliasing is
recall-safe by construction (it only ever adds alternatives to an atom, never removes one), so the
test is a check on the implementation rather than on the argument — but it must exist, because
"only ever adds" is exactly the kind of claim that a stray `class_members` change could silently
falsify later.

## Status 2026-07-26 — BUILT, and the recall loss was confirmed by measurement

Implemented as designed. The one thing this document deliberately left open — whether the recall loss
is *reachable* rather than merely structural — is now settled by demonstration rather than argument:

- **The loss is real.** `tests/two_table_shared_representation_recall.rs::
  pre_fix_equivalent_rule_never_fires_on_table_a_originated_material` builds a pre-fix-equivalent rule
  net (bare `SegAlphabet::token`, no aliasing) and shows it leaves table-A-originated material
  **unchanged** — the rule silently fails to fire. Its sibling
  `current_compile_fires_on_table_a_originated_material` shows the aliased path now rewrites it. So the
  false-negative direction this document argued for was correct, and it was reproduced before being
  closed.
- Fixture: `conformance-staging/edge-cases/two-table-shared-representation-recall` — a root on an inner
  stratum's table and an obligatory rule on an outer stratum's table, sharing a spelling at a
  deliberately **misaligned raw index**, which is the precise condition that makes the tokens differ.
- Containment holds across the table boundary, compared at the `structured` morpheme-id level (the
  methodology `two_table_symbol_divergence.rs` already established).
- `encode_query` verified to stay single-token, per item 4 — a query must not become ambiguous.
- The predicate's shared-representation arm is `Refuse` → `ConfirmOnly`, and its "why disjointness is
  the proof obligation" section is rewritten to name the false-negative direction as the real risk.

Implementation note worth recording: threading `TableId` cost **one** changed call site rather than
the ~40 the design feared. `SegAlphabet` gained an optional aliasing field plus a `with_table_id`
constructor, leaving `SegAlphabet::new` untouched, and only `compile_rewrite_rule_subset` — which
already resolves the rule's owning table — builds the alias-aware alphabet. No public signature
changed.

### Residual gap this fix does NOT close — CLOSED 2026-07-27

`compile_metathesis_swap_net` rendered tokens **directly** rather than through `render_slots`, so the
alias expansion never reached it. A `Metathesis` rule in a grammar whose tables share a normalized
representation remained exposed to exactly the false negative aliasing fixed everywhere else. The
exposure was advisory-only (the predicate is `ConfirmOnly` and `CompileDecision` is check-only), but
the honest boundary had moved without this path being covered, and the exposure widened slightly when
right-to-left metathesis began compiling more shapes.

**Closed by routing metathesis through the alias-expanded path, not by text-level unioning.** A
first-principles re-check found that reusing `render_slots`'s own render-time union (a bracketed
`[τ_A | τ_B]` at each position) would have been UNSAFE for metathesis specifically, even though it is
safe for ordinary rewrite rules: a metathesis swap must reproduce the *same* value at its (possibly
transposed) output position, and independently rendering a union at both the matched position and its
swapped destination would let the compiled transducer pair a matched alias with a *different* alias's
token — a new correctness bug strictly worse than the false negative being fixed. (Ordinary rewrite
rules don't have this hazard: a rewrite's RHS is a genuine re-specification, always resolved via the
rule's own owning table regardless of which alias matched on the LHS, and an environment/context
position is passed through as literal identity by foma's own replace-rule semantics regardless of
which literal alias satisfied it — neither role requires "the output must echo exactly what matched
elsewhere," the property a metathesis swap uniquely needs.)

The actual fix lives one level down: `crate::replace::slot_candidates` (the function that resolves
each switch position's *concrete* candidate `CharDefId`s for the pre-existing per-branch
cross-product construction) now expands every member to every `(table, cd)` pair sharing its own
normalized representation, via the SAME `RepresentationAliasMap`. The pre-existing per-branch
construction — built originally so that a multi-member natural class's own matched value transposes
correctly, never nondeterministically cross-pairing with another class member — needed no change at
all: each branch still fixes exactly ONE concrete candidate per position (now possibly drawn from
another table, but never a union), and the swap still only *permutes* that literal assignment vector.
Switch-position identity therefore holds by the same argument that already covered ordinary
(non-aliased) multi-member classes, extended one level: "candidate member" now ranges over aliased
`(table, cd)` pairs instead of only the rule's own table's char-defs, but the enumeration shape that
keeps the swap identity-preserving is unchanged.

Verified: `rust/crates/pg-foma/tests/multi_table_metathesis_shared_representation.rs` reproduces the
pre-fix loss directly (a hand-rendered, pre-fix-equivalent swap net never fires on table-A-originated
material), confirms the fix closes it over the real production compile path, and exhaustively checks
every combination of aliased/non-aliased candidates at both switch positions never substitutes a
different alias at the transposed output position. Fixture:
`conformance-staging/edge-cases/multi-table-metathesis-shared-representation`.

**A separate, out-of-scope finding surfaced while verifying this.** `pg_parse::Morpher` itself (via
`pg_rules::metathesis`/`pg_rules::bridge`, not `pg_foma::replace`) currently fails to analyze a
genuinely cross-table metathesis case at all — confirmed reproducible, narrowed to raw-index
misalignment (not solely `pg_rules::metathesis`'s own hardcoded `TableId(0)` at
`metathesis.rs:497,646`, since making that hardcoding coincidentally correct did not fix it either),
but not fully root-caused within this task's `pg-foma`-only boundary. This is unrelated to, and does
not block, the fix described above — the fixture's own `STAGING.md` has the full account, mirroring
the "discovered, out-of-scope, reported not hidden" precedent `two-table-shared-representation-recall`
already established for its own unrelated finding.

## What this design does NOT settle

- **Whether the false-negative I describe is reachable today.** It is currently unreachable *by
  refusal* — the predicate refuses before any such grammar compiles. What is not verified is whether
  any grammar in hand actually exhibits shared representations across tables with differing indices;
  the fixture in item 6 has to be authored to create the case deliberately. So the recall-loss
  argument above is a **structural** one about the token function, and it is deliberately not
  presented as a measured observation.
- **Alias-set size.** If some future grammar shares many spellings across many tables, atoms grow
  as a union over tables. That is a cost question for `ComposeBudget`, not a correctness one, and it
  should be measured on a real multi-table grammar rather than guessed at (per the standing rule that
  sample measurements may motivate research but never narrow a design).
- **PUA capacity.** Untouched by this design, because tokens stay per-`(table, char-def)` exactly as
  today. Worth noting only because the disjoint-range alternative *would* have raised it: the BMP PUA
  is 6,400 codepoints, so a stride-based scheme would have needed the supplementary planes and a
  check that the FST backend round-trips non-BMP scalars. Aliasing avoids that question entirely.

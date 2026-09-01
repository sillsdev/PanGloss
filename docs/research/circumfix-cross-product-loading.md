# Loading a FieldWorks circumfix entry

Why `compile/affixes.rs` builds a cross-product for an unconditioned circumfix and refuses a
conditioned one. Every claim here was read out of the data or the conformance suite, not inferred.

## How FieldWorks stores a circumfix

Read from `samples/data/mbugwe.fwdata`, entry `577b6780`, which has three `MoAffixAllomorph` records:

| record | form | `MorphType` guid | meaning |
|---|---|---|---|
| LexemeForm `f2774ad4` | `kaa- -iyE` | `d7f713df` | circumfix -- a combined DISPLAY string, `IsAbstract` |
| AlternateForm `f252f2b2` | `kaa` | `d7f713db` | prefix half |
| AlternateForm `7da95189` | `iyE` | `d7f713dd` | suffix half |

The halves are already ordinary prefix- and suffix-typed allomorphs. The circumfix-typed form carries
no material of its own, so "cross-product" means prefix-typed x suffix-typed -- 1x1 for this entry.

## Why the cross-product is accepted

`conformance-staging/edge-cases/circumfix-cross-product-and-infix-drop` settles this, and it is the
right authority: it is the engine's own conformance suite, not a guess about FieldWorks.

Its `mrCross` declares **four** `MorphologicalSubrule`s realizing a 2-prefix (`pa-`/`ma-`) x 2-suffix
(`-an`/`-in`) combination -- one subrule per pairing, which that fixture's `STAGING.md` describes as
"mirroring the shape a FieldWorks/LCM `MoAffixProcess` cross-multiplies from two separately-declared
allomorph sets." It records that every subrule classifies `Role::CircumfixPrefix` (leading AND
trailing `InsertSegments` around the copied stem) and that all four "are individually admissible into
`build_structural_composites` today".

So the shape needs nothing new from the emitter. `subCrossBA`/`subCrossBB` are also deliberately
declared 3rd and 4th, pinning the "declared later" reachability concern, and four further fixtures
cover overlap and precedence: `circumfix-reduplication-precedence`,
`circumfix-infix-interior-action-precedence`, `circumfix-in-template-slot`,
`circumfix-non-first-allomorph-selection`.

The RHS the loader must build is therefore:

```
lhs = [Pattern { nodes: any_plus }]
rhs = [ insert("kaa+"), Copy(PartRef::Input(0)), insert("+iyE") ]
```

## Corrected: a conditioned half IS representable (the earlier reasoning above was backwards)

The two "abandoned" attempts below were real, and their empirical failures were real, but the
*theory* attributed to attempt 2 had the anchoring direction backwards, and the fix follows directly
from reading the actual check rather than the module doc's summary of it.

`pg-rules/src/validity.rs`'s `environments_ok` (W3.3) is `envs.iter().any(...)` -- **at least one**
of an allomorph's declared environments must hold at a given run, evaluated independently per
contiguous `MorphRecord` run, and `allomorphs_valid_impl` requires **every** run to pass (an AND
across runs, an OR within one run's env list). A circumfix's combined allomorph produces exactly two
runs (the leading insert, the trailing insert; W3.3's own per-contiguous-run split). So the correct
encoding is **the union of both halves' environments on one allomorph**, not a single one-sided
environment:

- At the prefix run, only the *prefix* half's own environment can ever hold (there is nothing to the
  run's other side to satisfy a suffix-side environment) -- so the union's `.any()` reduces to
  exactly the prefix's own condition there.
- At the suffix run, symmetrically, only the suffix half's environment can hold.
- Both runs must pass (the across-run AND), which is exactly "stem starts with X AND stem ends with
  Y" -- the genuinely combinatorial semantics the Aweti pairing needs.

Attempt 2's failure was authoring only **one** side's environment on the combined allomorph (e.g. a
lone `RightEnvironment` meant to gate the prefix side) rather than the union of both. With only one
environment declared, `.any()` has nothing else to fall back on at the *other* run, so it correctly
(not backwards) reports no satisfier there -- the fixture's failure was real, but the conclusion
drawn from it ("there is no way to declare this applies to only one piece") does not follow: the
mechanism does not need a way to scope an environment to one piece, because each run already sees
only its own neighboring context, and a union naturally partitions itself across runs by which side
each piece touches. `pg-grammar/src/compile/affixes.rs::build_circumfix_allomorphs` now builds this
union directly (calling the same guid-to-`EnvironmentDef` conversion `build_root_allomorph` already
used, extracted into `compile/environment.rs::resolve_environment_defs`), and
`conformance-staging/edge-cases/circumfix-conditioned-halves/` pins the corrected mechanism -- see
its own STAGING.md for the differential-loading caveat (that fixture, and every
`conformance-staging`/`machine/conformance` fixture, loads via `pg_grammar::load`'s native HC-XML
path, which never reaches `build_circumfix_allomorphs` at all; that function is reachable only from
`pg_grammar::compile_project`'s `Snapshot`/fwdata-import path, so the regression pin for this
specific function lives in `pg-grammar/src/compile/tests.rs` instead).

`positions` (`MoAffixAllomorph.PositionRS`) is unioned in exactly the same way: C#'s
`HCLoader.GetAffixAllomorphEnvironments` (HCLoader.cs:1167-1170) concatenates `PositionRS` with
`PhoneEnvRC` into one `Environments` collection before any of this checking happens, and the
non-circumfix loader already does the same (`build_affix_allomorphs_for`'s `combined_env_guids`), so
there is a real C# analog and no separate refusal is warranted for it.

## What the reference grammars actually contain

| grammar | entry | halves | conditioned? | outcome |
|---|---|---|---|---|
| Mbugwe | `577b6780` | `kaa` x `iyE` | no | **built**, 1x1 |
| Mbugwe | `ebd1bcb7` | `a` x `iyE` | no | **built**, 1x1 |
| Aweti | `1efc0d2b` | `t`,`i` x `tu`,`ytu` | **yes** | **built**, 2x2 (union of per-side environments) |

Aweti's is the interesting one, and it is genuinely the hard case -- each half is conditioned on the
side that faces the stem:

| half | form | environment |
|---|---|---|
| prefix | `t` | before vowel `/_[V]` |
| prefix | `i` | before consonant `/_[C]` |
| suffix | `tu` | after vowel `/[V]_` |
| suffix | `ytu` | after consonant `/[C]_` |

That is a 2x2 whose cells are not independent choices, exactly the "genuinely combinatorial" framing
the fixture's own notes use. Each cell's combined allomorph carries the union of its own prefix
half's and suffix half's environments; per the corrected reading above, no scoping mechanism is
needed because each of the two runs a cell produces only ever sees its own side's context.

**Caution for the next reader:** these environments are NOT visible as `PhoneEnvRC` elements in a
naive window-grep of the `MoAffixAllomorph` records in `aweti.fwdata` -- that search returns nothing
and is misleading. Read the imported snapshot (`pangloss import`) instead; the loader reads the
snapshot, so the snapshot is the authority.

## A separate, oracle-confirmed limitation: the disjunctive-allomorph re-check

Fixing the refusal is necessary but **not sufficient** to make a full N-way cross product (N>1
pairings sharing a literal half) analyze every cell correctly, in EITHER engine. `conformance-
staging/edge-cases/circumfix-conditioned-halves/` pins this empirically: a 2x2 cross product (2
prefixes x 2 suffixes, each pairing's combined allomorph correctly carrying the union of its own two
environments) built from 4 sibling allomorphs of ONE rule only analyzes its FIRST-declared cell;
oracle-checked (`rust/tools/oracle-conformance.ps1`) against the C# founding oracle, the other three
cells fail **identically in C# and in `pg_parse::Morpher`** -- this is not a Rust bug.

The mechanism is `pg-rules/src/validity.rs`'s disjunctive-allomorph re-check (W3.2, ported from
`Allomorph.cs:127-152`): for each morph occurrence of the CHOSEN allomorph, every earlier-indexed
"passed-over" allomorph of the same rule is rejected as a competing analysis unless it free-fluctuates
with the chosen one OR its own environments fail to hold AT THAT SAME MORPH SPAN. Two cross-product
cells that share a literal half (e.g. both use prefix "t") by construction share that half's own
environment too, so at the shared half's run, the earlier-declared sibling's per-run check ALSO
passes -- and since the two cells' FULL environment sets differ (their other, non-shared half
differs), they do not free-fluctuate, so the later-declared cell is rejected. This is a real property
of the algorithm, faithfully ported, not an anchoring bug: it fires per actual morph occurrence of the
chosen candidate, and a multi-run (circumfix) allomorph's "other" run -- where the earlier sibling's
own competing material would need to sit -- never actually exists in the analyzed word, so there is no
way for that run to independently refute the earlier sibling.

Whether the real Aweti FieldWorks entry actually hits this (i.e. whether the loader ever puts more
than one T3.3-conditioned pairing sharing a literal half into the same rule, and in what declaration
order) is **not yet verified** -- this finding is pinned on a synthetic grammar, per this repo's
synthetic-data rule, and generalizing it to Aweti's specific data needs its own investigation.

## The open gap (narrowed, not closed)

The environment-carrying-half refusal this section used to describe is gone: `build_circumfix_
allomorphs` no longer drops a conditioned pairing. The remaining refusal in that function --
`prefixes.is_empty() || suffixes.is_empty()` (an entry with no loadable half on one side) -- is still
a WARNING with no record on `Grammar` of what was dropped, so `pangloss fst-health` can still report
`representability=WithinLimits` while a rule that needed a missing half is silently absent -- a
control that cannot act, in the loader rather than the envelope.

Closing it means giving `Grammar` a dropped-construct record the capability layer can read, and a
predicate that turns it into `CannotRepresent`. `Grammar` has ~20 struct-literal construction sites
and no `Default`, so this is a real refactor rather than a one-line addition, and it is deliberately
not bundled with the loader change.

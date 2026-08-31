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

## Why a conditioned half is refused instead

A half may carry a `PhoneEnvRC` environment. Per-side conditioning **cannot be represented**, and
this is proven rather than assumed -- the same fixture's "Deviations from the original sketch"
records two attempts, both abandoned against the real `pg_parse::Morpher`:

1. LHS-embedded `SegmentNaturalClass` constraints on the stem's first/last segment: every subrule
   with a real class constraint failed to parse at all, while the unconstrained one parsed.
2. `RequiredEnvironments`/`LeftEnvironment`/`RightEnvironment` on an unsplit stem. `pg-rules/src/
   validity.rs`'s W3.3 discontinuous-morph fix explains why this cannot work: a rule inserting both a
   prefix AND a suffix produces **two separate contiguous `MorphRecord` runs for the same
   allomorph**, and `environments_ok` is checked independently against each run's own span. A
   `RightEnvironment` meant to gate the prefix side is also evaluated against the suffix run --
   checking past the end of the word -- and fails there. There is no way to say "this environment
   applies to only one of a circumfix's two pieces".

Attaching the environment anyway would produce a rule that silently never fires: under-generation,
which ADR-0001 forbids. So the loader refuses and says so.

## What the reference grammars actually contain

| grammar | entry | halves | conditioned? | outcome |
|---|---|---|---|---|
| Mbugwe | `577b6780` | `kaa` x `iyE` | no | **built**, 1x1 |
| Mbugwe | `ebd1bcb7` | `a` x `iyE` | no | **built**, 1x1 |
| Aweti | `1efc0d2b` | `t`,`i` x `tu`,`ytu` | **yes** | refused |

Aweti's is the interesting one, and it is genuinely the hard case -- each half is conditioned on the
side that faces the stem:

| half | form | environment |
|---|---|---|
| prefix | `t` | before vowel `/_[V]` |
| prefix | `i` | before consonant `/_[C]` |
| suffix | `tu` | after vowel `/[V]_` |
| suffix | `ytu` | after consonant `/[C]_` |

That is a 2x2 whose cells are not independent choices, exactly the "genuinely combinatorial" framing
the fixture's own notes use. Representing it needs a way to scope an environment to one piece of a
discontinuous morph, which W3.3 says does not exist today.

**Caution for the next reader:** these environments are NOT visible as `PhoneEnvRC` elements in a
naive window-grep of the `MoAffixAllomorph` records in `aweti.fwdata` -- that search returns nothing
and is misleading. Read the imported snapshot (`pangloss import`) instead; the loader reads the
snapshot, so the snapshot is the authority.

## The open gap

The refusal is a WARNING, and `Grammar` carries no record of what the loader dropped. So
`pangloss fst-health` still reports `representability=WithinLimits` for Aweti while a morphological
rule it needs is absent -- a control that cannot act, in the loader rather than the envelope.

Closing it means giving `Grammar` a dropped-construct record the capability layer can read, and a
predicate that turns it into `CannotRepresent`. `Grammar` has ~20 struct-literal construction sites
and no `Default`, so this is a real refactor rather than a one-line addition, and it is deliberately
not bundled with the loader change.

# pg-grammar compile/natclass.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-grammar/src/compile/natclass.rs` implementation
comments so the source can carry a one-line pointer instead of the full argument.

## Eager extraction vs. HCLoader's mixed eager/lazy population

`pg-fwdata` extracts every declared `PhNCSegments`/`PhNCFeatures` object from the project
unconditionally (the "keep the full authored data" principle). `HCLoader` does not include every
declared natural class in its exported grammar, but it is not a pure lazy/reference-only filter
either — two separate mechanisms both feed `m_language.NaturalClasses`:

1. **Named classes, eagerly, regardless of use.** `LoadCharacterDefinitionTable`
   (HCLoader.cs:88-90, 2736-2741) builds `m_naturalClassLookup`, a `Dictionary<string,
   IPhNaturalClass>` keyed by `Abbreviation.BestAnalysisAlternative.Text` — every declared natural
   class gets inserted here, last-write-wins on a key collision — then unconditionally calls
   `TryLoadNaturalClass` on every distinct key in that dictionary, because root-allomorph `[Abbr]`
   bracket notation can reference any declared class by name whether or not any rule/environment
   also does. A class with a real, non-empty abbreviation is therefore always present in the final
   grammar — this has nothing to do with reachability.
2. **Unnamed classes, lazily, only on actual reference.** An unnamed class's abbreviation is `""`;
   `m_naturalClassLookup` keeps only the last-declared one at that key (every earlier unnamed class
   is silently overwritten and never reachable through this dictionary at all). Every unnamed
   class — including the ones this dictionary lost — can still end up loaded the other way
   `TryLoadNaturalClass` gets called: on-demand, by direct LCM object reference, from
   `SimpleContext`/pattern/rewrite-rule/`MoInsertNC`/`MoModifyFromInput` resolution elsewhere in
   the loader (`TryLoadNaturalClass`'s per-object memoization cache, `m_naturalClasses`). So an
   unnamed class survives iff it is the dictionary's last-declared `""` entry, or something in the
   grammar actually references it — never merely because it was declared.

Confirmed directly against live data: Amharic's `.fwdata` has three distinct, unnamed
`PhNCFeatures` objects with identical `cons=+,syl=+` content — one is referenced by an enabled
rewrite rule (kept either way); the other two are referenced by nothing and are not the project's
last-declared unnamed class, so HCLoader's own export never surfaces them, while `pg-fwdata`'s
eager extraction does.

## `compact_to_referenced`: how the reconciliation works, and its one known gap

`compact_to_referenced` reconciles this the same way `build` already reconciles the
synthetic-boundary-vs-real-inventory difference for phonemes: build every declared natural class
up front (every other compile step still needs the full `by_guid`/`by_name` lookup to resolve a
reference, wherever in the snapshot it's authored), then, once the whole `Grammar` is otherwise
fully assembled, keep (a) every named class, (b) the synthetic "Any" class, (c) the last-declared
unnamed class (`NatClassBuild::last_unnamed`), and (d) every other natural class actually
referenced somewhere structurally (every `SimpleContext` — patterns, environments, phonological
rewrite/metathesis rules, compounding subrules, root-allomorph environments) — dropping the rest
and remapping survivors to dense ids.

One known, narrow gap in (d): `crate::segment::segment_with_patterns`'s `[ClassName]`-in-a-root-
shape bracket notation resolves a natural class straight into a raw `CdSet` at segmentation time
rather than leaving a `NatClassId` anywhere in the compiled `Grammar` for this sweep to find — an
unnamed natural class reachable only that way would be incorrectly dropped here (a named one is
safe regardless, per (a) above). Confirmed inert for both reference corpora: neither Sena's nor
Amharic's snapshot has a single root-allomorph form containing a literal `[` (grepped), so this
bracket path is never exercised by real data today; worth revisiting if a future corpus does use
it.

## `NatClassBuild::last_unnamed`: why last-declared, and why it survives regardless of reference

`m_naturalClassLookup` (HCLoader.cs:88-90) is a `Dictionary<string, IPhNaturalClass>` keyed by
abbreviation text, last-write-wins, so exactly the last-declared unnamed class ends up at key `""`
and gets eagerly force-loaded alongside every other distinct abbreviation — independent of whether
anything actually references it. Every other unnamed class is only kept if something references
it.

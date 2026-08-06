# P10: the `StrRep` identity lane (`pg_rules::bridge`)

## What the lane is

C#'s `CharacterDefinitionTable.Add` puts `StrRep = {reps}` on every char-def feature struct;
`SegmentNaturalClass` unions member FSs, so its `StrRep` is the member-rep union
(`SegmentNaturalClass.cs:16-26`). Each rep string belongs to exactly one char-def — duplicate reps
are a load error — so rep-set intersection is equivalent to char-def-set intersection.
`Feature`-kind classes carry no id bits: C# `FeatureNaturalClass` FSs have no `StrRep` value at
all (`NaturalClass.cs:7-15` adds only `Type=Segment`).

`PatternBridge::id_lane` (a bool field, opt-in via the `id_lane()` builder method) is the flat-lane
port of this: when on, and the table fits in one `u64` (see `id_lane_width`), `Segments`-kind class
constraints and concrete char-def constraints carry an extra synthetic lane at index
`feature_width()` holding a char-def membership bitset.

Default `false`: only the morphological-LHS compile sites (`pg_rules::morph::compile_parts` /
`build_analysis_lhs`) opt in, and only they feed these FSTs id-lane-bearing inputs (`segs_of`) — an
FST compiled WITH id-lane constraints must only ever receive id-lane inputs, or determinized
*negated* arcs would reject inputs C# accepts (`MatchInput::matches`'s `!flat_unifiable(seg, neg)`
with an absent input lane treats the neg's id bits as intersecting). The rewrite/metathesis
pipelines keep the flag off and stay byte-identical to their pre-P10 behavior: their inputs may
carry the extra lane harmlessly, since against a lane-less constraint an extra input lane is
absent = unconstrained on the constraint side, so both pos and neg tests reduce to the pre-P10
comparison.

## The residual gap `nat_class_lanes` still has, and why it's scoped out

For `NaturalClassKind::Segments`, the lane-wise **union** of members' feature bundles
over-approximates real membership when matching an *existing* concrete segment against the class
in a pattern (a rule LHS or an environment): a segment unifiable with the union but not itself a
member still matches. On a zero-phonological-feature grammar (Sena) every member's lanes are
`&[]`, so the union degenerates to "matches any segment" — the same mechanism that motivated the
output-side fix in `pg_rules::morph::InsertSimpleContext` / `pg_parse::surface::matching_str_reps`.
Sena's own grammar exercises this path for real (`nc1` appears directly in `mrule1`'s LHS
patterns), so it is a real, unfixed contributor to Sena's over-generation, not a theoretical gap.

P10's identity lane now largely closes this for bridges compiled with `id_lane` on and tables
≤64 char-defs (the morphological LHS + allomorph-environment paths — exactly the paths where the
residual bit Sena). Still open in principle for id-lane-off consumers (phonological
rewrite/metathesis) and >64-def tables (Amharic), but P7 measured and censused that residual as
inert on every reference grammar: all `Segments`-kind class unions in Indonesian and Amharic are
exact (their rich feature systems fully pin every char-def), the only unifiable char-def pairs are
unreachable, Sena has no rewrite/metathesis rules at all, and no grammar has any metathesis rule.
Executable evidence: `tests/p7_segments_union_census.rs` (asserts the closure conditions,
self-skips without the sample grammars). End-to-end: Indonesian 121/121 byte-identical, Amharic
673/673 zero-DIFFERENT (V1b), Sena 7121-word zero-DIFFERENT (V2b). Re-scope only if that census
fails on new grammar data (e.g. a FLEx-authored grammar with underspecified phonemes).

Not fixed by P10: `pg_fst::Segment` (the frozen FST's per-position match unit) carries only
phonological lanes, no char-def/`StrRep` dimension, so discriminating by real membership there
needs either a `pg-fst` representation change (a frozen contract this port does not edit) or a
positional post-match membership filter analogous to the alpha-variable agreement check
(`node_vars`/`pattern_var_occurrences`) threaded through every FST consumer. Scoped out for
effort/risk reasons, not silently left wrong.

## The `FeatureNaturalClass` `Type` lane pin

C#'s `NaturalClass` ctor (`NaturalClass.cs:9-13`) stamps every `FeatureNaturalClass`'s feature
struct `fs.AddValue(HCFeatureSystem.Type, HCFeatureSystem.Segment)` at construction (unless already
frozen — never true for an author-loaded `<FeatureNaturalClass>`, `XmlLanguageLoader.cs:702`), so a
bare natural-class pattern node can only ever match a real Segment annotation, never a Boundary
one — even though a `Boundary`-kind shape node's other phonological lanes are all-unconstrained and
would otherwise unify trivially with any authored feature pair.

Without this pin, `nat_class_lanes` left the synthetic `Type` lane at `UNCONSTRAINED`, so a plain
`<SimpleContext naturalClass=...>` environment constraint could spuriously *directly* match a
`Boundary` node wherever one sits at a position an anchored environment check lands on with no
legitimate skip available (e.g. a boundary as the very last matcher-stream entry, where
`pg_fst::traverse::Transduce::initialize`'s start-anchor-and-optional skip-arm has nothing beyond
it to skip to — `TraversalMethodBase.cs:203-222`, which `initialize()` mirrors exactly and is not
itself a bug).

Confirmed independently real by direct instrumentation: a `RightEnvironment=[ncHighV]` check
anchored exactly at root 19's ("b+ubu") internal boundary node returned a spurious direct match
pre-fix — even though on that specific site the downstream symptom turned out to be masked by the
legitimate skip arm succeeding too. The true decisive bug for `csharp_port_rewrite.rs::
epenthesis_rules` sub-cases (2)/(5) was a separate, independent site-enumeration bug in
`pg_rules::rewrite::syn_epenthesis`.

`NaturalClassKind::Segments` needs no equivalent pin: its lanes are already the union of real
member char-defs' own `feature_lanes()`, each of which carries its own genuine `Type` pin, and a
`SegmentNaturalClass` only ever lists real `<Segment>` members, never boundaries, so that union
already comes out Segment-only, matching C#'s equivalent exactly.

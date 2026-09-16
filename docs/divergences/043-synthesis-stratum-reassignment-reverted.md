# 043 — Synthesis-direction stratum reassignment diverged from hc.dll's surface rendering

## Kind
Behavioural.

## Status
Reverted-in-rust.

## C# site
`SynthesisStratumRule.Apply` (`SynthesisStratumRule.cs`) never reassigns `Word.Stratum`. Only
`AnalysisStratumRule.Apply` does (`AnalysisStratumRule.cs:126`, `input.Stratum = _stratum;`) — the two
directions are NOT symmetric on this field.

## Rust site
`pg_rules::stratum::synthesize_stratum_traced` (`rust/crates/pg-rules/src/stratum.rs`), consumed by
`pg_parse::Morpher::surface_of` (`rust/crates/pg-parse/src/morpher.rs`), which renders a word's
signature surface half against `g.strata[w.stratum.0].table`.

## What differs
Rust's synthesis direction used to mirror the analysis direction's own unconditional `.stratum`
assignment (`nw.stratum = stratum;` on stratum entry), reasoning that a later table lookup keyed on
`.stratum` must resolve the stratum the word actually reached, not the one it entered at. This was
believed a correctness fix (it corrected an empty-surface-half rendering bug for a root synthesized
past its own entry stratum) until the C# founding oracle was run directly on the same grammar: hc.dll
itself renders that word's surface half as EMPTY (`"ROOT1|"`), matching what Rust produced BEFORE the
assignment was added, not after. The assignment was a faithful implementation of a plausible-looking
but wrong hypothesis about C#'s own behavior — C#'s asymmetry (`SynthesisStratumRule.Apply` never
touches `Word.Stratum`) is deliberate, not an oversight `AnalysisStratumRule`'s own symmetric
assignment would suggest.

## Can it change a parse?
Yes, and it did: with the assignment in place, HC-Rust rendered `"ROOT1|y"` for a word hc.dll renders
as `"ROOT1|"` — a genuine signature mismatch on the exact field (`Morpher::signature()`'s surface
half) every parity diff compares on.

## Evidence
`conformance-staging/edge-cases/two-table-shared-representation-recall/`'s own STAGING.md records the
full arc: the assignment was added, believed to fix an empty-surface-half bug, and `words.yaml` was
authored against post-fix Rust output (`"ROOT1|y"`). Re-running the fixture against the C# founding
oracle (`hc-conformance.exe` self-check) surfaced the divergence directly: hc.dll's own trace
(`[prXtoY]`) still produces `"ROOT1|"`. Per this repo's oracle hierarchy, hc.dll is ground truth:
`words.yaml`'s `y` entry now records `"ROOT1|"`, and the assignment
(`nw.stratum = stratum;` in `synthesize_stratum_traced`) was removed (`addbdba7`), restoring the
pre-"fix" behavior as the correct one. `pg_parse::Morpher::surface_of`'s own doc comment was corrected
to say the table is rendered against `w.stratum`'s table, "never reassigned during synthesis."

## Upstream
None needed — hc.dll's asymmetry between the two stratum-rule directions is itself the correct,
deliberate behavior; there is nothing to propose changing.

## Notes
This is the twin of entry 041's own investigation: both were found the same way (a staged grammar
becoming loadable by hc.dll for the first time exposed a Rust-side assumption nothing had previously
tested against the founding oracle). Where entry 041 and 042's fixes brought Rust closer to a
correctness hc.dll already had, this entry's fix instead REMOVED a Rust change that had, in fact,
moved Rust further from hc.dll while looking like an improvement — the scenario the oracle hierarchy
in CLAUDE.md exists to catch.

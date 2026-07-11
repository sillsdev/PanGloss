# Known gaps — `hc-hybrid` (F0-F9 port of the C# `fst-advisor` hybrid FST analyzer)

This is the honest, consolidated catalogue of everything the F0-F9 port left open, closed only
partially, or discovered but did not fix. Most entries were already recorded in
`docs/fst-plan/HYBRID_FST_RUST_PLAN.md`'s per-milestone "Actually shipped" sections; this file
gathers them in one place (per F9's own scope item) plus records what F9 itself closed, narrowed,
or newly found. Read the plan doc's own milestone entries for full citations/context — this file
summarizes, it does not replace them.

## 1. `ForwardSynthesisProposer` has no real implementation

Flagged since F6, still true after F9. `CompositeAnalyzer`'s `forward_synthesis` flag exists and is
wired at the correct proposer-order slot (between the bare FST and `ReduplicationProposer`, matching
C#'s `CompositeProposer.ForLanguage` `Insert(0, ...)` — see `composite.rs`'s module doc), but the
proposer itself is a permanent no-op stub. No milestone ever claimed ownership of building it for
real. **Not exercised by any gate**: `forwardSynthesis` is off by default on all three reference
grammars, so no golden comparison depends on it doing anything.

## 2. Beam-overflow undercount vs C# on sibling-proposer traffic

Found during F8 review. C#'s `BeamOverflows`/`LastBeamOverflowWord` in `FstCoverageProbe` is a
running delta on a *shared* `FstTemplateAnalyzer` instance that `ReduplicationProposer`/
`InfixProposer`/`ComposedPhonologyProposer` also walk through (`FstCoverageProbe.cs:91-93`), so one
corpus word can increment the overflow counter more than once, and the "last overflow word" can end
up being a sibling-proposer's internal variant string, not the corpus word itself. This Rust port
(`probe.rs`) counts at most one overflow per corpus word and always reports the corpus word — an
undercount relative to C# whenever a sibling proposer independently overflows on the same word.
**Diagnostic-only impact, zero on all three real grammars today** (confirmed: `BeamOverflowCount`
is 0 on Indonesian/Sena/Amharic at the default budget in every stats golden).

## 3. `GrammarFstAdvisorTests.cs`'s 8 methods never ported as Rust unit tests

`advisor.rs` has no `#[cfg(test)]` module at all — every claim about its correctness rests on
byte-identical golden comparison (`f8_fst_stats_gate.rs`'s advisor-only gate) on the three real
grammars, not on hand-authored toy-grammar unit tests the way `probe.rs`/`composite.rs`/etc. have.
An independent Fable review at F8 verified several branches by line-by-line inspection (the Tier 2⁺
verdict string/ordering, the probe-able tag path, the bounded-reduplicant `regular=true` arm, the
non-regular unbounded-rewrite tail, metathesis/`ModifyFromInput` `Info` advisories, the
many-allomorphs `> 8` threshold) but none of these have a dedicated Rust test pinning them — only
the three real grammars' golden coverage, which does not necessarily exercise every branch (e.g.
none of the three grammars has a bounded reduplicant, so that arm is inspection-only, never
golden-tested). Not attempted in F9 (would need new hand-authored toy-grammar fixtures, not a
mechanical fix) — recorded here again rather than left to silently persist.

## 4. hc-cli: `fst-batch`/`fst-candidates` remain unwired (F9 wired `fst-stats` only)

F1 through F8 never wired ANY `hc-rs fst-*` subcommand — every milestone gated exclusively through
direct Rust library/integration-test calls. **F9 closes the cheapest, highest-value third**:
`hc-rs fst-stats <grammar.xml> [out.txt]` now exists (`hc-cli/src/main.rs::run_fst_stats`), reusing
`stats::assemble_lines` directly so its output is, by construction, the same text
`f8_fst_stats_gate.rs`/`f9_full_battery_gate.rs` already verify byte-identical. `fst-batch` (per-word
propose→verify TSV dump) and `fst-candidates` (per-candidate TSV dump, composite emission order) are
**still not wired** — both would need batch iteration over a word list plus (for `fst-batch`) the
watchdog/timeout plumbing this milestone added to `replay::confirm_checked`, which is straightforward
to lift into a CLI loop but was not done here given time. `composite::batch_lines`/`candidate_lines`
already produce the exact row shapes both commands would need — wiring either is "call these
functions in a loop over a word-list file with a `BufWriter`," the same shape `hc-cli`'s existing
`run_batch` already has for the plain-engine `batch` command. Left honestly open rather than rushed.

## 5. Metathesis rule compilation is an unconditional `IdentitySkip` stub

`compiler.rs`'s `RuleInverseCompiler` never attempts a real inverse for a `MetathesisRuleDef` —
every metathesis rule reports `IdentitySkip` regardless of its actual shape. Confirmed inert on all
three reference grammars and both hand-authored toy fixtures (none declares a `<MetathesisRule>`),
so no gate anywhere is masking a real gap today — but a future grammar with one metathesis rule
gets zero chain-side contribution for it. Unchanged by F9 (out of scope; would be new compiler work,
not a battery/docs task).

## 6. The "hard Amharic precondition" (plan §5.3) — structurally closed, but empirically UNEXERCISED

The plan flagged, post-F5, that `replay.rs` allegedly never wires `RealizationalAffixProcessRule`/
mrule gating on the synthesis side, and called this a blocking precondition for any Amharic word
going through `VerifiedFstAnalyzer`. F9 investigated this directly (code-read plus corpus check):

- **Code-level**: it IS wired, uniformly. `hc-rules::morph.rs` dispatches `synth_realizational`/
  `ana_realizational` generically for `MorphRuleDef::Realizational` (both the synthesis-side
  `SynthesisRealizationalAffixProcessRule.Apply` and analysis-side
  `AnalysisRealizationalAffixProcessRule.Apply` C# equivalents are ported). `hc-hybrid::replay.rs`'s
  `rule_filter`/`build_morpheme_owners` already treat `MorphRuleDef::Realizational` identically to
  `MorphRuleDef::AffixProcess` under one `RuleRef::MRule`/`MorphemeOwner::MRule` space (this was
  closed transparently by the underlying engine port's own W5 milestone, before this plan's F5 even
  started — the Fable reviewer's concern predates confirming this).
- **Corpus-level**: `grep -c "<RealizationalRule" samples/data/{indonesian,sena,amharic}-hc.xml`
  is **0/0/0** — none of the three reference grammars this port gates against defines a single
  `RealizationalRule`. This makes the concern **moot for every gate this whole plan runs**: no
  golden comparison on any of the three grammars can exercise (or fail to exercise) realizational
  gating at all.

**Honest conclusion**: the code path exists and is structurally sound by construction (same
generic dispatch as every other `MorphRuleDef` variant, no special-casing that skips it), but it is
**untested territory** — zero gates, on zero grammars, in this entire F0-F9 plan ever run a word
through a `RealizationalRule`. A future grammar that adds one is exercising genuinely new ground,
not a "should already be covered" case. Do not report this as "closed and verified" — report it as
"implemented, plausible by code inspection and generic-dispatch symmetry, never empirically
exercised."

## 7. Sena: full-corpus scope is VERIFIED-parity only, not candidate-parity

F9's headline gate widens Sena's **verified** batch parity from the guarded 60-word slice
(F6's scope) to the full 7,121-word corpus (see `f9_full_battery_gate.rs`). Sena's **candidate**
parity (`f6_composite_gate.rs`'s `sena_slice60_composite_candidates_match_golden`) remains
slice-60-scoped — no full-corpus `candidates-composite.tsv` golden exists for Sena (only
`candidates-bare-full.tsv`, the BARE-walker-only candidates, generated as an F4 follow-up). This is
not a regression or an oversight introduced by F9: it reflects the goldens actually available, and
composite candidate-set correctness on the full corpus is already implied transitively by full
verified-parity plus slice-60 candidate-parity (a systematic candidate-emission bug would need to
be pathologically confined to words outside the slice AND to candidates that still verify correctly
by coincidence, which the soundness argument makes implausible, but this is inference, not a direct
gate — recorded honestly rather than silently promoted to "done").

## 8. Full-corpus Sena verified-parity results (F9) — NOT a gap, recorded for completeness

Measured (`f9_full_battery_gate.rs::sena_full_corpus_verified_matches_golden_watchdogged`, release
build): all 7,121 words, **0 pathological/timed-out, 0 mismatches** — byte-identical against
`sena/batch-chainoff-full.tsv` with no exclusions at all. Total wall time 1,263.4s (~21 minutes;
faster than the C# manifest's own 30-40 minute estimate for the equivalent run). The 60s/word
watchdog never fired once. See the plan doc's F9 entry for the full write-up.

## 9. Amharic verified-parity: FULL 673/673, no exclusion list needed (F9) — a positive surprise

Per plan §5.3, verified-set parity on Amharic was expected to be gated on the intersection of words
where the Rust ENGINE is already at parity with C# (the engine-port's own prior measurement,
`docs/history/rust-optimizations-phase2.md` §V1b, found 13 words where the UNRESTRICTED engine
times out). This milestone determined the ACTUAL exclusion set empirically, per the plan's own
instruction, rather than importing V1b's list wholesale — and found it to be **empty**:
`f9_full_battery_gate.rs::amharic_full_corpus_verified_matches_golden_gated_subset` ran all 673
words with a 60s/word watchdog and got **0 pathological/timed-out, 0 mismatches**, byte-identical
against `amharic/batch-chainoff.tsv`, in 3.4s total wall time. Restricted verify (a single pinned
root + a few rules) is a strictly easier search than the unrestricted engine analysis V1b measured,
exactly as plan §5.2 predicted ("collapses the search that currently caps out") — none of V1b's 13
unrestricted-engine timeout words turned out to be reachable/relevant as a hybrid-verify candidate
in a way that reproduces the timeout. Two other previously-ungated Amharic goldens were also closed
this milestone: `amharic_full_corpus_composite_candidates_match_golden` (full 673-word candidate
parity — a hole no F1-F8 test had covered) and `amharic_negatives_all_verify_empty` (the 50-word
soundness battery — likewise never gated before). All three: green.

## 10. rustfst — evaluated, not adopted (not a gap, recorded for completeness)

§7.0's bounded F1 investigation confirmed rustfst's concrete-`u32`-symbol arc model cannot represent
this crate's unification-arc (`FeatureStruct`-labeled) trie without first concretizing to a quotient
alphabet — the same "quotiented chain" idea already deferred to plan §12 item 5 (post-parity). Its
lazy-composition design is a read-worthy reference for `walk.rs`'s `ChainClosure`/`CascadeSymbol`,
not a dependency. See `docs/fst-plan/F1_QUIRK_AUDIT.md`'s "rustfst evaluation" section.

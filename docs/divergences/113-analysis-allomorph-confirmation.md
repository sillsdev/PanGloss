# 113 — Analysis-selected affix allomorphs guide synthesis

Kind: efficiency and behavioural alignment.
Status: optimized-in-rust; Machine [#548](https://github.com/sillsdev/machine/pull/548) merged at `c2454760`; the cutoff is proposed in [#560](https://github.com/sillsdev/machine/pull/560) at `f902cf87`.

Machine records the allomorph unapplied by each affix-process application. Synthesis matches earlier allomorphs in order, so an earlier unconstrained match still blocks a later choice, but emits only the recorded allomorph. Allomorphs after it cannot produce a retained result and are not tried. Generation without an analysis-selected allomorph retains the ordinary allomorph search.

HC-Rust records this choice along the rule trail, includes the trail in word identity, and replays it through expanded alternatives. `pg-rules/src/morph.rs` applies the same filter and cutoff in cached and uncached synthesis. The rule trail itself remains the authority for application order; the allomorph trail cannot substitute a different rule.

The intended recall argument is that each successful confirmation must replay its own analysis derivation; earlier applicable forms are still considered for disjunctive blocking. The filter also prevents a different allomorph from being emitted as if it were the analyzed one. This is an upstream-alignment argument, not a proof of equivalence for every grammar. Capped, skipped, missing or timed-out rows do not demonstrate parity.

`pg-parse/tests/csharp_port_affix_process.rs` covers the recorded first allomorph, an earlier constrained match and an earlier unconstrained blocker. On PanGloss base `14975991`, the cutoff regression failed because indices `[0, 1, 2]` were attempted; after the port it passed. The tests are code-constructed unit/integration fixtures, not a shared conformance grammar. Broader measurements are reported separately in the work report.

The pinned Sena and Amharic worst-word lists are absent and are not covered by this run. New 30-word stride samples are not substitutes for those lists.

The 2026-10-27 comparison used five existing FieldWorks corpora and NEW 30-word stride samples at a one-million-work-step cap. The full Mbugwe corpus had 23 completed words with changed analysis multisets; all changes removed one or more pre-port analyses. Fifteen Mbugwe words changed from capped to completed and twelve from completed to capped; one Aweti word changed from completed to capped. Other jointly completed words had identical multisets. The 23 changed Mbugwe words were checked against C# at Machine `c2454760` with the HC XML export (`mbugwe-548-oracle.md`). Three words (`miiro`, `miikalɔ`, `shaamiikalɔ`) are also dropped by C# master. Nineteen lost analyses C# retains because this port exposes the known alpha-environment matching gap ([115](115-alpha-environment-gap.md)); a follow-up must fix it. `kɔɔna` was also a pre-existing divergence (12 analyses in PanGloss before, 14 in C#). The other corpora’s changed analyses were not compared with C#. See the work report for the counters and status totals.

The existing code-constructed `inferred_q_analyzes_like_an_authored_featureless_q_and_unlike_a_valued_one` CLI fixture previously expected a feature-valued `qtu` analysis. With the earlier `ta` allomorph applicable to front-valued `q`, the later `tu` is disjunctively blocked; the regression expectation now reflects that. This is a synthetic rule-level argument, not a C#-recorded FieldWorks observation.

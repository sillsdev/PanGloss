# 114 — Realizational rule synthesis application cap

Kind: behavioural and termination alignment.
Status: fixed-in-rust; [Machine #474](https://github.com/sillsdev/machine/pull/474) merged as `ba0e245a`.

Machine caps each realizational rule at one synthesis application per word. A blockable rule with no applicable family member could otherwise repeatedly apply to its own output. HC-Rust's guided synthesis already consumes recorded rule applications; the direct synthesis entry points nevertheless allowed repeated application. Both cached and uncached paths now record the rule's owning morpheme on each successful output and reject a second application of that rule. Compiled grammar rules have distinct owning morphemes. The state survives cloning and segment deletion and distinguishes word keys; the cached rejection emits `MaxApplicationCount`.

The code-constructed `realizational_rule_synthesizes_at_most_once_per_word` regression fails on the preceding PanGloss commit and passes with the cap. It checks a second rule and an independent word as well as the rejected repeat. `cached_realizational_synthesis_stops_after_first_application` exercises the production cached path using an XML-compiled grammar. These are not shared C#-oracle fixtures; they do not claim to reproduce a full-parser hang in HC-Rust.

The pinned Sena and Amharic worst-word lists are absent and are not covered by this run. New 30-word stride samples are not substitutes for those lists.

The 2026-10-27 before/after measurements on the five available reference corpora and NEW stride samples had identical work-step totals, statuses and completed analysis multisets for this cap. Capped and skipped rows are not completed parity evidence; see the work report for totals and counts.

`pg-foma-backend/tests/closure_unbounded_realizational.rs` separately checks that the regular FST proposal loop remains able to over-propose repeated forms while HC confirmation rejects a second application. Its suffix and prefix controls accept one application. This retains the backend's proposal containment without claiming the extra proposals are valid analyses.

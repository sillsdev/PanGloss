# Containment-fix campaign — state and triage (2026-08-10)

Goal (user): fix all containment failures + unit 3; Fable review at plan (done, verdict
proceed-with-changes) and at end (pending); accuracy vs HC C# is the bar, speed a tiebreaker.
DONE = Fable's six falsifiable criteria (see the planning-review report), headline: the ratchet
file `rust/crates/pg-foma/tests/faithfulness_expected_failures.txt` deleted-because-empty and
`FaithfulnessRequirement::NoFailures` flipped, with no accuracy-by-retreat on the shipping backend.

## Landed (all verified by coordinator review + test evidence)
- Unit 3 (Infix-with-drop structural capability, census C4): 904/906 suite, predicate ConfirmOnly
  flip pinned both ways, ownership handoff both sites, uncovered-clearing unioned. DONE.
- Gate hardening: load-failure honesty, observed==discovered, expected-failures ratchet with
  tighten-only contract; falsified all four ways. 6/6 + 13/13. DONE.
- New fixtures with REAL C#-oracle provenance (dotnet wrapper actually run): circumfix-in-template-slot
  (red on templated: talodien — NOT tuned; structural_candidate_rules scans all mrules, so tuned
  holds at depth-1) and cross-stem-material-determination (all three backends Held — over-proposal
  already safe). DONE.
- Units 1-2 of cover-circumfix (pg-grammar cross-product port; staged fixture) DONE earlier.

## Ratchet triage (25 triples: plan-composed 0, tuned 1, templated 24)
Mechanism buckets (fix per bucket, red-first, delete ratchet lines in the same change,
re-verify with fix reverted per repo rule):
1. TUNED+TEMPLATED shared: two-table-shared-representation-recall word `y` (both backends) —
   suspect shared table-representation handling; PRIORITY 1 (shipping backend).
2. Templated two-sided insertion (~7): pabatidan talodien kebzatan kemitan ketamtaman gelobt
   semitide — fix = marker-layer generalization in structural_allomorph.rs: marker at suffix
   position; right half realized at marker, left half edge-inserted conditioned on downstream
   marker ([..] -> pfx || .#. _ ?* MARKER), marker realized last; nests in slot order
   (agreement-locality: sides local, pairing stays HC's).
3. Templated truncation/drop: sa pat bat — marker-conditioned deletion (extends the same layer).
4. Templated boundary/metathesis/RTL: mu+i(x2) xw ey.
5. Templated interdigitation/templatic: katabit kpfotab ndpat des — candidates for honest
   CannotRepresent (Process-family; "emits nothing" must be literally true) — ROW CHANGES NEED
   DECIDER-SESSION SIGN-OFF (ownership boundary).
6. Templated reduplication/stratal: kuuukuuu(x2) yxkib; table-binding: g.

## Remaining sequence
fixes per bucket (Sonnet, bounded; coordinator reviews each diff) -> real-grammar corpus rerun
(mbugwe via pg.ps1, zero Morpher-found-never-proposed for its hard-coded backend; any miss ->
new fixture) -> NoFailures flip + delete ratchet -> comment-hygiene sweep -> Fable CODE review
over full branch diff -> user decides merge.
Worktree: .claude/worktrees/circumfix-cross-product (branch circumfix-cross-product; cherry-pick
180d6f4 = the sweep; units 1-3 + gate hardening + 4 fixtures uncommitted on top).
Decider session owns: strategy_coverage rows, backend_selection, BACKEND_PREFERENCE. This
session owns: measured facts, fixtures, emit.rs/structural_allomorph.rs/faithfulness files.

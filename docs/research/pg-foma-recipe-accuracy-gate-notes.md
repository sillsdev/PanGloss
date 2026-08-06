# The fast accuracy path, gated against the slow one (`tests/recipe_accuracy_gate.rs`)

`pg_foma::recipe_runtime::assess_accuracy_with_cache` answers "did we undergenerate?" by
admission-key set containment against the run's already-shared oracle result, performing NO
full-HC confirmation per candidate. This gate pins the three claims that make it worth having, and
the one it must never make.

1. **It agrees with certification about accuracy.** On a fixture whose candidates certify, the
   accuracy verdict is `NoLoss`; the two mechanisms answer the same accuracy question.
2. **It really is confirmation-free.** The same fixture's certification path does non-zero full-HC
   work (`Score::confirmation`, `Score::confirmation_steps`), and the accuracy path's counters for
   those SAME quantities are zero — a comparison, not an assertion about a field nobody feeds.
3. **The check executes.** `AccuracyCounters::membership_tests` is non-zero on a real fixture and
   zero on a path where the check could not run. A mechanism that is merged but never fires is the
   exact failure a reverted per-candidate proposal budget already produced once.
4. **It never reports a pass it did not earn.** A refused corpus is `NotDetermined`, not `NoLoss`;
   and a real recall failure is detected and named.

What it deliberately does NOT claim: that the accuracy verdict may select a candidate. Selection
still requires full-HC confirmation, and `Score` is untouched — pinned here too, because a change
that moved a winner would be a defect in the change rather than a finding about the grammar.

# pg-foma RewriteMode::Simultaneous: fixture design and why no mode-blindness workaround is needed

Moved out of `rust/crates/pg-foma/tests/phase_c_simultaneous.rs`'s module doc so the source can
carry a short pointer instead of the full argument.

## What the fixtures cover

`RewriteMode::Simultaneous` compiles via `replace.rs`'s ordinary sequential-compose machinery,
unchanged, for any rule the simultaneous-subrule-overlap predicate proves pairwise
non-overlapping — because non-overlapping simultaneous application is equivalent to sequential
application. A rule the predicate cannot clear stays gated rather than being compiled wrong.

- `sim-trivial`: a single, ungated, environment-free subrule. Vacuously admitted (no peer subrule
  for the pairwise overlap check to ever examine), and this fixture proves it actually compiles
  end to end, not merely that the predicate detects it.
- `sim-nonoverlap-env`: two subrules whose right environments are mutually exclusive natural
  classes with no shared segment. Proven non-overlapping via the real lowered-span intersection,
  exercised end to end: compiles, and a proposer-to-confirm containment check against the full-HC
  oracle holds exactly.
- `sim-overlap-env`: two subrules whose right environments genuinely overlap (share a segment).
  The overlap predicate must refuse, and the rule must stay honestly unsupported — reported
  skipped, never a wrong compile.

## Why no oracle mode-blindness workaround is needed here

A related rewrite direction (`Dir::RightToLeft`) needed a recall-preserving safety-net union
because the confirm oracle turned out to be blind to rule direction. `Simultaneous` vs `Iterative`
does not have the same problem: `pg_rules::rewrite` is not mode-blind for it — it dispatches to
genuinely distinct synthesis functions per mode and wraps analysis in a self-opaquing-gated
repeat-until-fixpoint loop. The overlap predicate's own self-opaquing refuse path, plus a stricter
lone-self-opaquing refusal in the capability layer, keeps every rule these fixtures compile
outside the region where that kind of asymmetry could ever bite. So exact oracle equality — not a
superset union — is the right bar for `sim-nonoverlap-env`, and it holds.

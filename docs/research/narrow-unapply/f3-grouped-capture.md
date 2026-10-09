# F3 — grouped capture for narrowing unapplication

## Change

`ana_narrow_general` now uses one named target capture per RHS position for true narrowing
rules (`LHS node count > RHS node count`). It recovers the physical segment positions from the
captures and marks only those nodes Optional before applying the existing LHS reconstruction.
Uncached, cached, and traced analysis share this behavior. Expansion and pure deletion retain
their existing paths; the RHS target constraints, environments, bindings, and reconstruction are
unchanged.

The grouped-position helper uses the direction-fresh capture edge, matching the established
feature-analysis approach. Missing or out-of-range target captures fail loudly.

## Regression fixture

Fixture: `conformance-staging/edge-cases/interposed-optional-multi-rhs/` (T4).

Forward derivation: lexical `ABC` has shape `abc`; `merge` maps `abc` to `de`; deletion of `x`
between `d` and `e` is vacuous, so synthesis yields `de` with analysis `ABC|de`. During analysis,
unapplying deletion can insert Optional `x` between the two real RHS positions. The narrowing
matcher must capture `d` and `e` and leave `x` Optional.

The fixture uses `forward-synthesis` provenance. Its four negative controls are `abc`, `dex`,
`dxe`, and `dd`. The task brief reports that C# loses the positive parse; C# was not run locally.
The boundary-inside-RHS variant was omitted because analysis target matching uses
`MutShape::segs(false)`, which excludes physical boundary nodes from capture positions.

Before the code change, the already-built base binary was run with:

```sh
/home/johnm/work/narrow539/bin/pangloss-main batch \
  conformance-staging/edge-cases/interposed-optional-multi-rhs/grammar.xml \
  /home/johnm/work/narrow539/f3-words.txt \
  /home/johnm/work/narrow539/f3-main.tsv \
  --threads 1 --word-timeout-ms 5000
```

The TSV recorded all five rows as `ok`; `de` and all four negative controls had signature `-`.
The evidence files are under `/home/johnm/work/narrow539/`.

## Verification

The first fresh-ID submissions stopped in preflight with exit 41 because the new private helper
had a two-line implementation comment. It was shortened, and the following new-ID runs were
submitted after the queue restart. All four final runs completed with exit 0. The all-scope run's
`pg-parse` `all_discovered_fixtures_match_oracle` test passed; that replay discovers both
`conformance-staging/` and Machine fixtures, including T4.

| Queue ID | Requested gate | Result |
| --- | --- | --- |
| `f3-v4-check-20261009-1713` | `-Mode check` | PASS (exit 0; clippy all targets completed) |
| `f3-v4-pg-rules-20261009-1713` | `-Mode test -Package pg-rules` | PASS (exit 0; 187 passed, 0 failed, 8 ignored) |
| `f3-v4-pg-parse-20261009-1713` | `-Mode test -Package pg-parse` | PASS (exit 0; 247 passed, 0 failed, 11 ignored) |
| `f3-v4-conformance-all-20261009-1713` | `-Mode conformance-test -Scope all` | PASS (exit 0; all discovered fixture replays passed) |

No timing comparison was made; the evidence is parse signatures and gate outcomes.

## Divergence tracking

Entry 083 is indexed in `docs/divergences/README.md` and `docs/divergences/by-module.md`.
Machine fix PR: [Machine PR #540](https://github.com/sillsdev/machine/pull/540).

## Commit

Not created. Git could not stage files because the worktree metadata path
`/home/johnm/work/PanGloss/.git/worktrees/narrow-f3-grouped-capture/` is read-only in this
session (`index.lock`: Read-only file system). A restart with writable worktree Git metadata is
needed to commit this change.

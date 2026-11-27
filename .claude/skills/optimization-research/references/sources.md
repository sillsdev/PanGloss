# Where the prior work is

## Ledger
- `docs/research/optimization/README.md` (index and entries); `docs/research/optimization/drafts/` (unmerged results from other agents).

## Issues (search all states; closed means rejected or landed)
- `gh issue list -R sillsdev/machine --label performance --state all --limit 200`
- `gh issue view 601 -R sillsdev/machine` (dead ends, do-not-re-propose) and PanGloss #103 (port check).
- PanGloss: `gh issue list -R sillsdev/PanGloss --state all --search "perf OR speedup OR memo OR slow"`; open ports: #66, #23; related: #90, #15, #144, #260.

## Existing research and design notes (`docs/research/` unless noted)
- `analysis-memo-explosion.md`, `2026-09-10-analysis-memo-fixes-for-machine.md`: the analysis memo OOM, three causes, what to port.
- `memo-is-a-net-cost.md`, `memo-sena-bisect.md`, `memo-entry-work-value.md`, `memo-eviction-tuning.md`, `docs/divergences/045-memoization-removed.md`: memo value and removal.
- `per-word-search-work-cap.md`, `step-cap-default-measurements.md`: the work cap (containment, not correctness).
- `live-frontier-memory-bound.md`, `word-memory-trace.md`: where per-word memory lives.
- `circumfix-cross-product-loading.md`: why a circumfix entry loads as a cross-product (loading cost and correctness).
- `pg-foma-typology-speedup.md`: per-construct engine timing harness (FST engine; context only, no FST work without the owner).
- `search-tree-failure-attribution.md`: attribution of failure to objects in a shared-prefix search tree.
- `docs/design/parse-cost.md`: the cost formula and where instructions are measured.
- Landing and sweeps: `.claude/skills/land-an-optimization/`; weights: `.claude/skills/retune-parser-work/`.
- Earlier sweep records and the grammar inventory live in the owner's lane tree (`optimization-workflow/`), not in the repo.

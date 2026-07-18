# `hc-cli verify` — grammar regression report

**Goal.** One command that answers "is the candidate grammar better?" so editing tools
(flexicon/GramTrans change-set pipelines) can gate a change on a machine-readable verdict.
PanGloss stays a pure function: it consumes two grammars and a word list, never change-sets.

```
hc-cli verify <baseline> <candidate> <words.txt> [--report out.json] [--all-words]
```

`<baseline>` / `<candidate>` accept anything `load_grammar` accepts (`.fwdata`, snapshot `.json`;
`.xml` only until the HC-XML sunset completes).

## Semantics

1. Load both grammars via the existing `load_grammar` dispatch (hc-cli/src/main.rs).
2. Parse every word with both `Morpher`s, honoring the existing `word_timeout`, and compute the
   PROTOCOL.md signature multiset per word (same algorithm `batch` already uses — reuse, don't
   duplicate).
3. Classify each word by comparing multisets:
   - `unchanged` — identical signature multisets (including both `-`).
   - `gain` — candidate has analyses baseline lacks, and lost none.
   - `loss` — baseline analyses missing from candidate (includes parse → `-`). **Regression.**
   - `changed` — both gained and lost. **Regression** (counts as loss for exit code).
   - `status_changed` — ok vs SKIPPED/crash flipped. **Regression** when candidate degrades.
4. Exit code: `0` = no regressions, `1` = at least one regression, `2` = load/run error.
   Pipelines gate on the exit code; humans read the report.

## Report format (`--report`)

```json
{
  "reportVersion": 1,
  "tool": {"name": "hc-cli", "version": "..."},
  "baseline": {"path": "...", "sha256": "..."},
  "candidate": {"path": "...", "sha256": "..."},
  "summary": {
    "words": 0, "unchanged": 0, "gains": 0, "losses": 0, "changed": 0, "statusChanged": 0,
    "coverageBefore": 0, "coverageAfter": 0,
    "meanAnalysesBefore": 0.0, "meanAnalysesAfter": 0.0
  },
  "words": [
    {"word": "...", "verdict": "loss", "before": ["MORPH+CHAIN|shape"], "after": ["-"]}
  ]
}
```

- `words` lists only non-`unchanged` entries by default (`--all-words` to include everything) —
  keeps reports small on large corpora.
- `coverage*` = count of words with ≥1 analysis. Mean analyses = ambiguity proxy: a drop with
  equal coverage usually means over-generation was pruned (a *gain* in quality, so it is surfaced
  in the summary, never inferred as a regression by itself).
- Without `--report`, print the summary plus the first N regressed words as text.

## Implementation

- One new module `hc-cli/src/verify.rs` plus a subcommand arm in `main.rs`. Extract the
  signature-building used by `batch` into a shared helper if it isn't already.
- Deterministic output (sorted words in report), so reports themselves are diffable.
- Estimated size: ~300–400 lines including the report serializer. No new crates
  (sha2 + serde_json are likely already in the tree; if sha2 isn't, hash is the only new dep).

## Tests

1. Golden pair: an existing sample grammar vs a mutated copy (e.g. one allomorph environment
   removed) with a small word list — assert exact verdicts and exit code 1.
2. Identity: grammar vs itself → all `unchanged`, exit 0.
3. Determinism: two runs produce byte-identical reports.

## Steps (one small PR)

1. Extract shared signature helper; add `verify` subcommand with text summary + exit codes.
2. Add `--report` JSON with the schema above; add the three tests.
3. Document in rust/README.md as the integration point for editing tools.

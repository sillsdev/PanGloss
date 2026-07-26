# spellcheck-research

Research harness for comparing PanGloss spell-check / word-prediction model families
(surface n-gram, class/factored n-gram, CRF-style reranker, bounded neural ablation) — see
`docs/research/spellcheck/PLAN.md` (D1–D15) in the main repo for the design this supports,
and `docs/research/spellcheck/18-research-harness.md` for how this package fits in.

This is a **separate, self-contained Python project** — a sibling of `rust/`, `docs/`, and
`openspec/` at the repo root — so it can be set up and run without touching the Rust
toolchain at all.

**No torch, no transformers, no heavy ML stack.** Per D5 (`PLAN.md`), anything neural is a
bounded, later ablation, not the design this harness exists to support today. Dependencies
are deliberately small: `numpy` plus `pytest` for testing.

## What's here

```
research/
  src/spellcheck_research/
    interchange.py     # the Rust-analyzer <-> Python interchange format (JSONL)
    models/
      base.py           # SpellcheckModel: the plug-in interface every model family implements
      ngram_baseline.py # a surface-wordform trigram with Stupid Backoff smoothing
    synthetic/
      profiles.py       # named corpus shapes (ambiguity x feature-richness), NOT real languages
      generator.py      # generates a synthetic Corpus from a profile, zero real-language data
    eval/
      metrics.py        # recall@k, MRR, keystroke savings rate, perplexity
      harness.py         # held-out splitting + the full evaluate() entry point
    cli.py              # `spellcheck-research run` — the end-to-end demo
  docs/
    interchange-format.md  # the format, documented, with a worked example
  data/local/            # gitignored — real corpora go here when they exist (see below)
  tests/                 # fast (<1s) unit + end-to-end tests
```

## Setup

### Plain `venv`

```sh
cd research
python -m venv .venv
# Windows (PowerShell): .venv\Scripts\Activate.ps1
# Windows (Git Bash):   source .venv/Scripts/activate
# macOS/Linux:          source .venv/bin/activate
pip install -e ".[dev]"
```

### `uv` (faster, and what this package was developed/tested with)

```sh
cd research
uv venv
uv pip install -e ".[dev]"
```

Either way, Python **3.11+** is required (no other version pins — nothing here depends on
a specific patch release).

## Run the tests

```sh
# venv already active:
pytest -q

# or, without activating, via uv:
uv run --python .venv pytest -q
```

All tests run in a few seconds and require no network access, no real corpus, and no Rust
build.

## Run the end-to-end demo

Generates a synthetic corpus for one or more named profiles, fits the baseline n-gram
model, evaluates it, and prints a results table (plus writes `runs/results.json`):

```sh
spellcheck-research run --profile high_ambiguity_moderate_richness --n-sentences 2000
# or, every profile at once (the default):
spellcheck-research run
```

Add `--dump-corpus` to also write each generated corpus out in the interchange JSONL
format, e.g. to inspect it or feed it to a different tool.

## Bringing in a real corpus

Real corpora are never committed to this repo (see the root `.gitignore` and the standing
"synthetic conformance only" rule this project already follows for the Rust conformance
suite). If a real, licensed corpus becomes available, drop its interchange-format JSONL
file(s) under `research/data/local/` (gitignored — see `.gitignore`'s `/research/data/local/`
entry) and point `spellcheck_research.interchange.read_jsonl` at it directly; nothing else
in this package assumes synthetic data specifically.

## Adding a new model family

Implement `spellcheck_research.models.base.SpellcheckModel` (`fit`, `score`,
`predict_next`, optionally `update`) and it plugs into `eval.harness.evaluate` exactly like
the shipped baseline. See `models/ngram_baseline.py` for the smallest complete example.

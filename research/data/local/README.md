# `research/data/local/`

This directory is **gitignored** (see the root `.gitignore`'s `/research/data/local/`
entry) — a local-only drop point for real corpora, exactly like `samples/data/` at the
repo root.

Nothing under this project assumes a specific file lives here. When a real, licensed
corpus is available, convert it to the interchange JSONL format
(`docs/interchange-format.md`) and place it anywhere under this directory, e.g.:

```
research/data/local/some-project-interlinear.jsonl
```

then load it directly:

```python
from spellcheck_research.interchange import read_jsonl
corpus, meta = read_jsonl("research/data/local/some-project-interlinear.jsonl")
```

Real language data is never committed to this repository — see the root `.gitignore`'s
comment ("Real language grammars + word lists are DESIGN-ONLY local inputs — never
committed") and this project's own synthetic-fixture convention
(`research/src/spellcheck_research/synthetic/`). This directory exists so a real corpus can
be used locally without any risk of it being committed by accident.

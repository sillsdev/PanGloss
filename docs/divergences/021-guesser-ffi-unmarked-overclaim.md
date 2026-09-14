# 021 — Guesser FFI symbols could return an unmarked guessed analysis

## Kind
Behavioural (an FFI-boundary overclaim, not a parse-algorithm divergence from C#).

## Status
Fixed-in-rust.

## C# site
Not applicable — this defect has no C# counterpart; it is purely about what PanGloss's own wire
format could misrepresent about the underlying (correctly-computed) result.

## Rust site
`pg_lexicon::analysis` and the `hc_parse_word`/`hc_parse_batch` FFI symbols (outside the
`pg-rules`/`pg-parse` core the rest of this catalogue focuses on, but recorded here because it is a
documented, resolved overclaim in shipping code).

## What differs
`pg_lexicon::analysis` retried via the guesser (`ParseOptions::default().with_guess_only(true)`)
**unconditionally** whenever a word produced zero analyses and the shape was valid — there was no
on/off switch. `UnifiedAnalysis` and each `WordAnalysis` both correctly carried the
`guessed`/`provenance` fact internally, so the information survived all the way to the FFI boundary —
and was then **discarded** there, because `hc_parse_word`/`hc_parse_batch`'s wire format has no
guessed field. A caller of those two symbols therefore received guessed analyses byte-indistinguishable
from confirmed, lexicon-backed ones.

## Can it change a parse?
Not a parse-correctness issue in the engine sense — the underlying analysis was computed correctly.
It is a **provenance** overclaim: a native caller of the old FFI symbols could not tell a guessed
analysis from a real one, which is arguably worse for a downstream consumer (e.g. FieldWorks) than a
missing analysis, since it presents fabricated structure as confirmed fact.

## Evidence
`docs/hermitcrab-rust-port-audit.md` §3a documents this in full under "OVERCLAIM in shipping code."
Fix (2026-07-25): `hc_parse_word`/`hc_parse_batch` now behave as guess-OFF (return no guessed
analyses at all), since they structurally cannot mark what they return; callers wanting guessing use
the additive `_opts` symbols, whose format carries an explicit `guessed` byte at both word and
analysis level. `pg-wasm` opts back in explicitly at both of its call sites, since the default flip
would otherwise have silently stopped its (deliberately guessing) demo behavior.

## Upstream
None, not applicable.

## Notes
Named directly in CLAUDE.md's "A control that cannot act must say so" section as an instance of the
same defect class as several unrelated infrastructure bugs — the decision rule stated there
("a dropped guess is a recoverable disappointment, an unmarked guess is a false claim") is the
general principle this specific fix instantiates.

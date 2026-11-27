# 126: A phonological pattern member that cannot be converted refuses the allomorph or rule in PanGloss; HCLoader drops the member

Kind: behavioural (loader only; malformed input).
Status: permanent (accepted-divergence). Decided 2026-11-03: no capability is built for malformed input; PanGloss keeps its refusal and reports it.

## What differs

A pattern member is unconvertible when it names a phoneme, natural class or boundary marker the grammar does not hold, or (affix process input only) carries alpha variables, which an `MoAffixProcess` has no scope for.

| Site | C# `HCLoader` | PanGloss |
|---|---|---|
| Sequence, iteration and simple contexts in `LoadPatternNode` (`HCLoader.cs:2367-2438`) | returns false for the member and the caller omits it; a sequence keeps its other members, an iteration whose member fails disappears | the whole pattern fails |
| Rewrite-rule left side, right side and contexts (`LoadRewriteRule`, `HCLoader.cs:2085-2120`) | a failing context is skipped; the rule loads with a weaker pattern | the rule is refused: `grammar.rule.build-failed` |
| Affix process input parts (`LoadAffixProcessAllomorph`) | the same `LoadPatternNode` drop | the allomorph is refused: `grammar.allomorph.process-build-failed` |

C# therefore keeps a rule or allomorph whose shape differs from the one the author wrote, and may match words the author's pattern would not (a dropped constraint) or lose a repetition. PanGloss loses the whole rule or allomorph and says so. The HC XML loader throws on the same input instead of dropping it.

## Author-facing report

Both refusals reach grammar health. `Ctx::refuse` for a rewrite or metathesis rule (`compile/rules.rs`) records `grammar.rule.build-failed` with the rule guid and the member error; the affix process path (`compile/affixes.rs`, `build_affix_allomorphs_for`) records `grammar.allomorph.process-build-failed` for the allomorph guid, with a generic message that does not name the failing member. Both codes have pages in `docs/grammar-diagnostics-reference.md`.

## Why

A rule that loads with a silently weakened pattern gives parses the author never wrote, with nothing to tell them. Refusal with a named finding is the safer failure, and a grammar a finished project holds does not contain these members.

## Rust site

`pg-grammar/src/compile/affixes.rs` (`phon_context_nodes`, `build_process_allomorph`); `pg-grammar/src/compile/rules.rs` (`phon_context_nodes`, the `RULE_BUILD_FAILED` refusals).

## Coverage and verification

Source reading only; not measured on ceb (no run was made for this entry). No test pins the refusal of an unknown-member pattern.

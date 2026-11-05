# FieldWorks metathesis rule with a left environment

`project.fwdata` was produced by LibLCM, not written by hand: `xample-projector author` from a plain three-root grammar with a `-s` suffix, then `configure-probe` with `feature-class` (`distinct: true`, so each phoneme has its own identity feature) and `metathesis` (`description` `[a, t, k]`, `leftEnv` 0, `leftSwitch` 1, `rightSwitch` 2). Its `PhMetathesisRule` has `StrucChange` `0 2 -1 1 -1`: left environment `a`, left switch `t`, right switch `k`. Roots are `atka` (R0), `itka` (R1), `tika` (R2).

`fw_metathesis.rs` imports it and parses the words below. The expected counts are what `hc.dll` (FieldWorks 9.3.10) gives for the live `HCLoader` language built from this very project (`xample-projector parse-hc`), and for the grammar `XmlLanguageWriter` wrote from it. The same grammar is `edge-cases/fw-metathesis-switch-indices` in sillsdev/fieldworks-parser-conformance.

HCLoader names the context at `RightSwitchIndex` "r" and sets it as the engine's left switch; `XmlLanguageWriter` writes `leftSwitch="prule1_r"`, the node at index 2, as the output shows. Before this change the importer read `StrucChange` from the wrong XML child, got "", and guessed both switches as index 0; the compiler then refused the rule.

A rule with a middle context, and a rule between featureless phonemes, are pinned by `conformance-staging/edge-cases/metathesis-middle-context` and `metathesis-featureless-phonemes` (issue #162, divergence 127).

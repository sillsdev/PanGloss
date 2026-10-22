# 102: Clitic rules and blocking strata (Machine #529) were unported

Kind: unported.
Status: fixed-in-rust. Rust now implements Machine PR #529 (`a2f80e28`, "Use IsCliticRule to allow
clitics to apply to templates"): `MorphologicalRule@isCliticRule`, `GetIsCliticRule()`'s
`Clitics`-stratum default, and `Morpher.IsBlockingStratum`. Before the port HC-Rust parsed as
pre-#529 C# did, and failed five words of `machine/conformance/edge-cases/clitic-rule-designation`.
C# was already correct, so no Machine issue is needed.

## C# site

All at `a2f80e28`:

- `XmlLanguageLoader.cs:865,961`: `IsCliticRule = (bool?)attr("isCliticRule")` on affix-process and
  realizational rules. `HermitCrabInput.dtd` declares it `(true | false) #IMPLIED`.
- `MorphemicMorphologicalRule.cs:11-21`: `IsCliticRule` (`bool?`) and `GetIsCliticRule()`: the
  value if set, else `Stratum.Name == "Clitics"`. Only affix-process rules ever call it; a
  realizational rule's value is loaded and never read.
- `Morpher.cs:815-829` `IsBlockingStratum`: a stratum with phonological rules, or the last stratum
  `RuleSelector` admits.
- `AnalysisStratumRule.cs:127-128`: the template state is cleared on entry only to a blocking
  stratum. The block that cleared it on every stratum output ("to allow clitics") is removed.
- `AnalysisAffixProcessRule.cs:85`: a clitic rule leaves `FinalTemplateState` unchanged, in a
  template slot or not.
- `SynthesisAffixProcessRule.cs:65,87,188`: both template gates and the output's
  `IsPartial`/`IsLastAppliedRuleFinal` update skip a clitic rule.
- `SynthesisAffixTemplatesRule.cs:67-70`: a passthrough is closed (`IsLastAppliedRuleFinal = true`)
  only when some template was applicable.
- `SynthesisStratumRule.cs:60-70`: a non-blocking stratum emits its words unchecked and still open
  (no final check, no remaining-rules check, no phonology, which it has none of); a blocking stratum
  refuses only `IsLastAppliedRuleFinal == false` (`?? true`, was `?? false`).

## Rust site

- `pg-grammar-model/src/model.rs`: `AffixProcessRuleDef::is_clitic_rule`,
  `Grammar::affix_rule_is_clitic`, `Grammar::is_blocking_stratum`.
- `pg-grammar/src/load.rs::parse_is_clitic_rule`, called for `MorphologicalRule` (stored) and
  `RealizationalRule` (validated, dropped). A value other than `true`/`false`/`1`/`0`
  (`XmlConvert.ToBoolean`'s set) refuses the grammar, naming the rule and value.
- `pg-grammar/src/compile/affixes.rs`, `compile/templates.rs`: `is_clitic_rule: None`. FieldWorks'
  `HCLoader` never sets `IsCliticRule` (checked at FieldWorks `620cd990`); it puts clitic affixes in
  a stratum named `Clitics`, as `compile/mod.rs` does, so the default makes them clitic.
- `pg-rules/src/stratum.rs`: `StratumAnalyzer::apply_one_mrule` (clitic keeps state),
  `StratumAnalyzer::analyze` (conditional entry reset; no fold reset),
  `synthesize_stratum_traced_with_policy` (takes the morpher's `rule_filter`; non-blocking emit;
  `Some(false)` refusal), `synth_apply_templates` (passthrough closed only when applicable).
- `pg-rules/src/morph.rs::synth_affix`, `synth_affix_cached`: `template_gated` excludes clitic rules;
  a clitic rule passes `partial: None` to `synth_process_allomorph`.
- `pg-parse/src/morpher.rs::synthesis_pipeline_selected_with_policy` passes its `rule_filter`.

## Coverage and revert evidence

`machine/conformance/edge-cases/clitic-rule-designation` (upstream `df2cf21a`, oracle-recorded at
`a2f80e28`), reached by moving the `machine` pin to PR #480 head `72dbd4f9`. Before the port that
pin fails exactly `sagds`, `sagdt`, `sagdsz`, `kolpu`, `kolpuli`; after it, 0 mismatches. Each
mechanism reverted alone, everything else kept:

| reverted | words that turn red |
|---|---|
| synthesis clitic gate (`template_gated` ignores clitic) | `sagds`, `sagdz`, `sagdsz` |
| analysis state kept by a clitic rule | `sagds`, `sagdz`, `sagdsz` (lost to the final-template prune) |
| synthesis blocking (always blocking, `!= Some(true)` refusal, passthrough always closed) | `kolpu`, `kolpuli` |
| analysis blocking (entry and fold reset restored) | none in the sweep; pinned by `non_clitic_state_crosses_a_non_blocking_stratum_and_prunes_its_final_template` |

The analysis-blocking reset only prunes: synthesis refuses `sagdt` either way. The pin test reads
the prune counters on `sagdt` (non-zero in `Main`) and `sagds` (zero).

The same pin move changes `morphotactic-attribute-breadth`'s grammar (also `df2cf21a`). On the old
grammar (pin `18cf242f`) post-#529 C# parses `kulpu` as `KUL+NF|kulpu`, as the port does: the PR #480
head harness reports `kulpu: expected [] got [KUL+NF|kulpu]` there. The old expectation was
pre-#529.

Loader: `is_clitic_rule_defaults_to_the_clitics_stratum_name_and_the_attribute_overrides_it`,
`is_clitic_rule_refuses_a_non_boolean_value` (`pg-grammar/src/load/tests.rs`).

## Interaction with 033

Entry 033's per-stratum prune policy (`FinalTemplatePruneFacts::default_prune_enabled`) enforces only
on strata below the first one owning a partial rule, where C# (`Morpher.IsPartial`) enforces nowhere.
Template state now crosses non-blocking strata, so could a state set where Rust does not enforce leak
into a stratum where it does? No: enforcement is monotone in analysis order. The non-enforcing strata
are analysed first and never set the state, so a word enters the first enforcing stratum with `None`,
exactly as every stratum entry saw before #529. From there the state flows as it does in C# with
enforcement on. #529 adds no case 033 did not already have.
## FieldWorks

No released Machine contains #529: `git tag --contains a2f80e28` is empty and it is not an ancestor
of `v3.9.5` (2026-10-05), the version FieldWorks' `Build/SilVersions.props` pins. The installed FLEx
(`C:\Program Files\SIL\FieldWorks 9`) ships HermitCrab `3.8.2+76e65e7e`. PanGloss follows Machine
`master`, the founding oracle, so it now matches FLEx only once FieldWorks takes a Machine release
containing #529. With FieldWorks' default layout (Morphology, Clitics, Surface; phonology on
Morphology unless `NotOnClitics` is false) no parse should change: Clitics has no templates and
every rule in it is clitic by name, so analysis state is untouched there; Surface is last and blocks.
If Morphology has no phonological rules it stops blocking, but a final template's closed word then
meets only clitic rules before Surface, and a non-final one is still refused at Surface, so the
outcomes match the old close-at-Morphology behaviour. The parity runs below agree on every word.

## Parity

Release binaries `d0a8cdad` (main before the port) and the port on top of it, `pangloss batch
--threads 1`, the default 50M step cap and no wall timeout, compared with `parse_compare.py`: every
compared row IDENTICAL; the one Mbugwe gap is listed in its row.

| corpus (grammar route) | words | result |
|---|---|---|
| Indonesian (fwdata) | 70 | 70 IDENTICAL |
| Amharic (fwdata) | 673 | 673 IDENTICAL |
| Aweti (fwdata) | 206 | 206 IDENTICAL |
| Sena (fwdata) | 6,146 | 6,146 IDENTICAL (653 SKIPPED on both sides: characters outside the table) |
| Mbugwe (fwdata) | first 478 of 1,645 | 477 IDENTICAL, 1 CAPPED on both sides (not comparable); both binaries were still on word 479 after 20 minutes and were stopped, so the remaining 1,167 words are **not compared** |
| Cebuano private lists (snapshot JSON, FieldWorks layout with `=ng` in Clitics) | 200 / 563 / 7,206 | all IDENTICAL |
| Cebuano dev list, snapshot plus one extra enclitic `y` | 563 | 563 IDENTICAL |

Analysis attempts are unchanged on the Cebuano dev list (57,203); work units fall slightly
(3,243,535 to 3,223,233), because a passthrough left unchanged no longer re-runs the stratum's rules,
as in C#.
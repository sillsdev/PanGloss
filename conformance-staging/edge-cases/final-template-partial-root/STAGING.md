# STAGING: final-template-partial-root

## Purpose

This is the complete Machine #480 final-template-partial-root corpus, staged because it is not present in PanGloss's pinned Machine submodule at aab4c59819adb7b366915e09df2fb28c62b8c03a. The Machine fixture was copied read-only from the supplied #480 worktree at fa8d9b2540962981add315d06203e928a11c1853. The copied grammar.xml and words.yaml are unchanged.

The words file records founding-oracle provenance (Machine commit a7056751, verified 2026-10-23). It has ten complete rows covering ordinary roots, partial roots, template-only parses, blocked final-template/rule combinations, and impossible partial-root/template combinations.

## Regression intent

Exercise #553's partial-root behavior end to end. Preserve every source status and identity row, and add a PanGloss runtime assertion that parse_word_with_stats_and_prunes reports an actual `template_batteries_skipped` effect for a partial-entry-only grammar. The clitic control separately verifies the per-template `final_templates_skipped` counter. Retain final-template-partial-discriminators as the partial-rule control. Do not treat source implementation or fixture presence as runtime verification.

## Promotion

Status: upstream_submitted
Source fixture: Machine #480 worktree at fa8d9b2540962981add315d06203e928a11c1853.
Candidate destination: machine/conformance/edge-cases/final-template-partial-root/
Keep this staged until Machine accepts the fixture and PanGloss bumps its pin; graduate the staged copy in the same change as the pin update.


Upstream contribution: [Machine PR #480](https://github.com/sillsdev/machine/pull/480). The same grammar/words bytes are present at [0bbb45c8](https://github.com/sillsdev/machine/tree/0bbb45c8e006f73f1f8f9b608d70450496a289d2/conformance/edge-cases/final-template-partial-root); [the source fixture documentation](https://github.com/sillsdev/machine/blob/0bbb45c8e006f73f1f8f9b608d70450496a289d2/conformance/docs/final-template-partial-root.md) supplies the explanation referenced by the unchanged words file.

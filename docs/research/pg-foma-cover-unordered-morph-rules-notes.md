# pg-foma cover_unordered_morph_rules.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-foma/tests/cover_unordered_morph_rules.rs`
implementation comments so the source can carry a one- or two-line pointer instead of the full
argument.

## What this file proves

Proposer-to-confirm containment for `MorphRuleOrder::Unordered`'s
`unordered-application.chain-depth-bounded` configuration predicate (target disposition:
`ConfirmOnly`), plus a deterministic `unordered-application.unbounded` budget-refusal witness.

## The synthetic fixture

One stratum, `morphologicalRuleOrder="unordered"`, two loose suffix rules declared in document
order `mrP` (index 0) then `mrQ` (index 1) — no `required_syn_fs`/feature interaction between them,
no phonological rules, no `Role::Infix` rule, no templates. Both suffix, so cascade application
order directly determines surface concatenation order: applying `mrP` then `mrQ` yields `"kpq"`;
applying `mrQ` then `mrP` yields `"kqp"`.

## The distinguishing property (empirically verified against the real oracle, `pg_parse::Morpher`)

`pg_rules::cascade::Cascade::permutation` (`Linear`) only ever recurses to a non-decreasing rule
index, so under a hypothetical `morphologicalRuleOrder="linear"` declaration of this same grammar,
`"kqp"` (rule index 1 firing before rule index 0) is not a reachable analysis at all —
`Morpher::parse_word_opts("kqp", ..)` returns an empty `structured` set under `Linear` (verified
directly against this fixture, `linear_variant`), while `"kpq"` (document order) is reachable under
either `mrule_order`. Declaring this grammar's own stratum `Unordered` is therefore the minimal
change that makes `"kqp"` a genuine, oracle-confirmed analysis — exactly the scenario where "a
word's analysis requires the stratum's rules to have applied in an order other than their declared
document order."

## The distinguishing witness against the pre-existing morphotactic-legality convention

This fixture has zero phonological rules and zero `Role::Infix` rules, so
`crate::preexpand::should_run` is `false` for it (both `mrP`/`mrQ` classify `Role::Suffix`), which
means `crate::preexpand::build_composites`/`crate::morphotactics::MorphotacticIndex::next_state`
(the "Linear-as-Unordered" pruning convention) are never consulted for a single (root, rule) pair
on this grammar — confirmed by `g.prules.is_empty()` in the test (the public proxy this integration
test can observe). The containment this file proves for `"kqp"` therefore comes entirely from
`crate::emit::build_deriv_chain`'s ordinary derivation-layer construction, not from that pruning
automaton — the two are not the same proof.

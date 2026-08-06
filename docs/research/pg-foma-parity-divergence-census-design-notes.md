# pg-foma parity_divergence_census.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-foma/tests/parity_divergence_census.rs`
implementation comments so the source can carry a one- or two-line pointer instead of the full
argument. Each section corresponds to one call site; the site names the function/type so this doc
can be found from either direction.

## Module doc: the soundness hazard the confirmation-free accuracy path rests on

`pg_foma::recipe_accuracy` detects undergeneration by checking that a candidate proposed the
admission key of every oracle analysis, performing no full-HC confirmation. That is a sound test for
undergeneration on its own. It is equivalent to full certification only if the other direction is
free — if a candidate's confirmed identity set can never contain an identity the oracle's set lacks.

The argument for that direction is strong: the candidate's confirm is a restricted
`Morpher::parse_word_selected` while the oracle is the same engine unrestricted, so the candidate
explores a subset of the search space. It is not airtight, and the gap is narrow and specific:
`pg_rules::word::WordKey` — the analysis-search dedup key — deliberately excludes the syntactic
feature struct, while `pg_parse::identity::AnalysisIdentity::category` is projected from it via
`WordAnalysis::pos_id`. Two search states differing only in `syn_fs` therefore collapse to one map
entry, and which one survives is decided first-wins by traversal order — which the restriction
perturbs. So a restricted run could in principle surface a category the unrestricted run deduplicated
away: a candidate-only identity.

That was inference, never an observation. This file measures it, because building containment on an
unmeasured assumption would silently certify a wrong answer on the day it stopped holding.

What is measured: `pg_foma::parity::IdentityDivergence::candidate_only_identities`, counted on the
ordinary certification path (inside `certify_corpus` itself, sharing its one projection pass — not a
second reimplementation that could disagree with the verdict about what it looked at) and
accumulated per run by `RunEvaluationCache`.

A zero licenses exactly one claim: on these fixtures, at these corpora, confirmation never yielded an
identity the oracle lacked, so undergeneration is the only way certification can fail and the
containment check detects it. It does not license removing confirmation from the certification path,
and it does not make the accuracy verdict a certification. It makes the accuracy verdict a
trustworthy fast screen. A non-zero is a finding, not a nuisance: it would mean the parity relation
and the compilation disagree about analysis identity somewhere, which is worth more than any speedup.

`occurrences_compared` is asserted non-zero, and `IdentityDivergence::supports_free_containment`
encodes the same rule in the type, because a run that was refused (a step-capped oracle occurrence, a
build failure) reports zero candidate-only identities purely because it compared nothing at all —
"I could not look" must never read as "everything is fine".

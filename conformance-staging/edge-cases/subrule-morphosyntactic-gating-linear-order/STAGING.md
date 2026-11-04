# subrule-morphosyntactic-gating-linear-order

The oracle pin's `edge-cases/subrule-morphosyntactic-gating` (`aab4c598`) under a distinct name. The
suite's version of the same fixture differs in one attribute: its stratum says
`morphologicalRuleOrder="unordered"`, the only order FieldWorks produces, where this copy says
`"linear"`. PanGloss's capability gate answers `ConfirmOnly` for an unordered stratum (the order-union
variant) and `Admit` for a linear one, and `cover_subrule_morphosyntactic_gating.rs` pins the `Admit`
verdict for a grammar whose only characteristic is subrule gating, so it needs the linear grammar.

It is a `local_regression`: it is not proposed upstream and must not graduate. The graduation guard
forbids one name in both roots, which is why this copy has its own name.

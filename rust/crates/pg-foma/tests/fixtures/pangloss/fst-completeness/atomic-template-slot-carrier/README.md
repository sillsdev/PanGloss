# Atomic template-slot carrier

This non-linguistic PanGloss fixture exercises one optional physical template
slot. The carrier chooses one lane before the root: a correlated Coupled lane
emits `p/P` before the root and its matching `s/S` after it, a Prefix-only lane
emits `u` before the root, a Suffix-only lane emits `t` after the root, and the
optional skip lane emits neither. The resulting FST preserves those choices
and does not manufacture crossed prefix/suffix paths.

Test-only mutations verify typed refusal for more than one cross-root slot,
carrier plus compounding, and a rule authored both in a slot and derivation
stratum. This is a PanGloss-only completeness fixture, not linguistic data.
Never promote it to `Machine`.

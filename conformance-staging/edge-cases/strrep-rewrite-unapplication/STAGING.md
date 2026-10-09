# StrRep-only rewrite inversion

This synthetic fixture pins literal `m -> p / x_` when segments have no authored
phonological features. `StrRep` is the only segment identity in Machine.

`xpuma` is independently justified by forward synthesis from `XMUMA`: the first
`m` follows `x`, changes to `p`, and the remaining `uma` is unchanged. The original
oracle loses this analysis. The other five rows record the oracle where it agrees.
The fixture deliberately remains red against that original oracle; do not replace
the derived answer with the observed failure.

The proposed Machine fix is on `fix/hc-strrep-unapplication` at `a873d60d`.
The additional synthetic `kad`/`kat` regression is committed at `a57d923d`.
See PanGloss divergence 071 for exact oracle revisions, runs, and revert checks.
The regression also requires rejecting a second, vacuous unapplication after
the target has already widened to include both input and output spellings.

Upstream PR link: pending lead publication.

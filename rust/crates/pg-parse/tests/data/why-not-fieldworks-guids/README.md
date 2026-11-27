# why-not guids on the snapshot route

`project.fwdata` is `adhoc-allomorph-copies` extended by hand (not produced by LibLCM):

- Stem entry `...0100` has two allomorphs: `da` (`...0101`) with environments `afterVowel` (`...0017`, `/[V]_`) then `afterT` (`...0030`, `/t_`), and `dat` (`...0111`) with `afterVowel` only.
- Affix entry `...0200` has two prefix allomorphs: `ta` (`...0201`) and `t` (`...0211`).
- `MoAlloAdhocProhib` `...09a1` forbids `...0201` with `...0101`; `...09a2` forbids `...0201` with `...0111` (same primary, other member); `...09a4` forbids `...0211` with `...0101` (other primary, same member).

`why_not_fieldworks_guids.rs` asks why `da` (two failed environments), `dat` (the second allomorph's environment), `tada`, `tadat` and `tda` do not parse and expects the MoForm, environment and prohibition guids and texts in each report.

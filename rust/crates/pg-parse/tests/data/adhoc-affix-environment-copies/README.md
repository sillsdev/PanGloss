# Ad hoc allomorph prohibitions over a multi-environment affix allomorph

`project.fwdata` is the `adhoc-affix-copies` project (prefix `ta-` with two MSAs, `MergeMSAs` off) with a second environment `beforeD` (`/_d`, `...0030`) and the prefix allomorph (`...0201`) restricted to two environments: `afterVowel` (`...0017`, never met at the start of a word) then `beforeD`. The allomorph compiles to two environment passes per MSA, and `tada` parses only through the second pass.

`adhoc_affix_copies.rs` adds one `MoAlloAdhocProhib` (anywhere) in each direction and expects no analysis. `HCLoader.LoadAffixProcessAllomorphs` registers one allomorph per environment in `m_allomorphs` (HCLoader.cs:1167-1178), so the prohibition must hold on every pass, not only the first.

Provenance is Rust-only (hand-written, not produced by LibLCM; no C# loader run).

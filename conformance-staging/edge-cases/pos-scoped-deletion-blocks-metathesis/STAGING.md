# STAGING: pos-scoped-deletion-blocks-metathesis

Pins divergence 050 (PanGloss issue 28, sillsdev/machine issue 520): a part-of-speech-scoped
deletion rule (`prHDel`, `h -> nothing / V _ V`, scoped to `posDel`) must not stop words of other
parts of speech from being analysed by metathesis rules in deeper strata.

- Oracle: forward-synthesis. `nui` and `mu+i` are hand-derived (see `words.yaml`); the controls
  `ai`, `pan`, `ahi`, `niu` agree by construction. hc.dll is expected to fail `nui` and `mu+i`
  until Machine #520 is fixed; a Machine master `hc.dll` (built Sep 9, before #539 merged) was run by hand on the grammar: no parse for `nui` and `mu+i`, `ai` parses; the full oracle replay was not run. Its
  `oracle-conformance-known-divergences.json` entry (needs the exact oracle reason text) is still to be
  recorded by whoever next runs `rust/tools/oracle-conformance.ps1`.
- Red on origin/main (`8f29d338`): `nui` and `mu+i` returned no analysis. Green with the
  `metathesis.rs` width-guard change.
- Graduation: candidate `machine/conformance/edge-cases/pos-scoped-deletion-blocks-metathesis/`
  once Machine #520 is fixed upstream.

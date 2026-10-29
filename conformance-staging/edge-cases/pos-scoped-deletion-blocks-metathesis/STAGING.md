# STAGING: pos-scoped-deletion-blocks-metathesis

Pins divergence 050 (PanGloss issue 28, sillsdev/machine issue 520): a part-of-speech-scoped
deletion rule (`prHDel`, `h -> nothing / V _ V`, scoped to `posDel`) must not stop words of other
parts of speech from being analysed by metathesis rules in deeper strata.

- Oracle: forward-synthesis. `nui` and `mu+i` are hand-derived (see `words.yaml`); the controls
  `ai`, `pan`, `ahi`, `niu` agree by construction. hc.dll is expected to fail `nui` and `mu+i`
  until Machine #520 is fixed; hc-conformance self-check (Machine master c2454760) reports FAIL on exactly `nui` and `mu+i` (no parse); the other four words pass. The matching entry is in `rust/tools/oracle-conformance-known-divergences.json`.
- Red on origin/main (`8f29d338`): `nui` and `mu+i` returned no analysis. Green with the
  `metathesis.rs` width-guard change.
- Graduation: candidate `machine/conformance/edge-cases/pos-scoped-deletion-blocks-metathesis/`
  once Machine #520 is fixed upstream.

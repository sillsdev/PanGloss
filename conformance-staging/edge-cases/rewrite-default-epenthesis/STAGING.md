# Rewrite defaults: epenthesis

GH#41. Derived from Machine's `loader-default-symbol` fixture, with one rewrite rule and
invented roots. Feature `nas` defaults to +; `a` omits that feature, while `d` explicitly has
nas=-. Underspecification is intentional: it is the condition under test.

The epenthesis target is nas=- between b and t. Surface a lacks nas and defaults to +, so bat is rejected in analysis. Surface d explicitly has nas=- and is accepted; the synthesized feature-only node renders as [ad(aa)].

The seven expected complete signatures in `words.yaml` were recorded with the actual C#
`hc.dll` built from Machine commit `aab4c59819adb7b366915e09df2fb28c62b8c03a`, using
`machine/conformance/adapters/hc-dotnet-wrapper.sh batch`. The same inputs were replayed in
PanGloss before and after the fix; see the round report for the exact differing rows. These
fixtures stay staged pending upstream acceptance. `defaultSymbol` is not currently emitted by
FieldWorks HCLoader.

Without the fix, PanGloss accepts `bat` as `|b[ad(aa)]t` (C#: no parse). The other six words
agree. With the fix, all seven signatures agree exactly.

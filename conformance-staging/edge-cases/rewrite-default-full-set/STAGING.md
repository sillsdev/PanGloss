# Rewrite defaults: explicit full symbol set

GH#85, a follow-up to GH#41 (`rewrite-default-epenthesis`). Same grammar, except that segment `a`
authors `nas` explicitly with both symbols (`symNasPlus symNasMinus`) instead of omitting it.

An explicit full set is a present value, so C# `UseDefaults` leaves it alone: unification binds the
set itself, not the `+` default. The epenthesis target is nas=- between b and t. Surface `a` carries
nas={+,-}, which unifies with nas=-, so `bat` parses as `|b[ad(aa)]t` here, while in
`rewrite-default-epenthesis` (where `a` omits `nas`, so the default `+` applies) it is rejected. The
rest of the words behave as in that fixture.

The seven expected results in `words.yaml` were recorded with the actual C# `hc.dll` built from
Machine commit `aab4c59819adb7b366915e09df2fb28c62b8c03a`, using
`machine/conformance/adapters/hc-dotnet-wrapper.sh batch`; `rules:` comes from the same build's
trace (`insert-oral` = `pr1` for both parsing words). `defaultSymbol` is not currently emitted by
FieldWorks HCLoader.

Without the fix, PanGloss encodes the explicit full set as the same full mask as an absent feature,
so `UseDefaults` substitutes `+` and `bat` has no parse (C#: `|b[ad(aa)]t`). The other six words
agree. With the fix, all seven agree exactly. Source of the bug: the Rust port.

Upstream PR: (none yet)

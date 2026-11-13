# mpr-features-beyond-128

PanGloss #150: PanGloss refused any grammar with more than 64 inflection classes, exception features and
entry inflection types together, although FieldWorks sets no such limit and C# HermitCrab keeps MPR
features in a `HashSet`. This grammar has 140 exception features. Roots carry one feature each, chosen at
the ends of the 64-bit words PanGloss stores sets in (ids 0, 1, 62, 63, 64 and the last one, plus 126, 127 and 128), and suffixes
require or exclude each of them. One root carries two (63 and 64) and a suffix requires both, and a
suffix adds the last feature to the word so a following suffix can require it.

C# verdict: all words replayed on the pinned `hc-conformance` (machine `4d9b454d`, self-check mode): PASS.
Oracle of record for `words.yaml`: the signatures were first written from PanGloss with the cap
removed, then confirmed identical by the C# self-check, so C# and PanGloss agree on every word.
Before the fix PanGloss 0.17.0 refuses this grammar with `grammar.limit.mpr-features`.

Source of the divergence: the Rust port (a `u64` bitset), not HC or the fixture. No `sillsdev/machine` issue.
FieldWorks-producible: yes, exception features are ordinary possibilities in the Exception Features list.
Upstream PR: not opened; the case stays staged until the owner asks to graduate it.

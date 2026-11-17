# Partial-feature literal rewrite

Sibling of the #136 admission review: replacing both rewrite classes with literal
segments still loses the ITA identity when a declared feature has no segment values.
A literal is not proof of a scalar feature bundle. Source: Rust FST proposal/admission.

Oracle: pinned C# Machine 1ddef2bd2f1503717de95556116df8e4a63846d6. Two-word
expectations copied from featureless-unused-feature and freshly replayed (3/3
sibling fixtures passed, fst-admission-sibling-oracle.log). The length neighbor is
a negative control. This is a loader-shape producibility claim, without an fwdata
round-trip witness. No C# bug or new Machine issue is claimed.

Upstream PR: pending (lane cannot publish).

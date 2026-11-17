# Partial-feature root spelling without a rewrite

Sibling of #136's class-bundle admission review. Deleting all phonological rules
still leaves two C# root identities on aka, but the FST proposes only AKA. This
pins the root spelling projection independently of rewrite syntax. Source: Rust
FST proposal/admission. No parser algorithm change is requested.

Oracle: founding C# HermitCrab, Machine 1ddef2bd2f1503717de95556116df8e4a63846d6.
Fresh self-check passed the two words; the managed recipe probe reproduced one
missing ITA identity with no exclusions. Evidence: fst-admission-no-rewrite-oracle.log
and fst-admission-no-rewrite-report/report.json in the lane. Expectations are the
observed C# signatures, not hand-derived. No Machine defect or fwdata round-trip
witness is claimed. This retains the original loader-shape producibility claim.

Upstream PR: pending (lane cannot publish).

# Structural closure permutation regression

Four independent internal affix processes replace different stem segments with a
segment plus an inserted segment. The root's initial segment always remains.
Every nonempty subset has a distinct surface and morph chain. The original FST
structural closure explores all 64 nonempty permutation prefixes for 15 outputs;
the five-rule unit-test extension explores 325 for 31 outputs.

The source is the Rust FST compiler's redundant enumeration, not an HC algorithm
or parser semantics change. `words.yaml` was transcribed from the pinned C#
HermitCrab tool at Machine `4d9b454d42a0da35e058641a65596339a79d115b`.
The C# replay completed all 18 words (16 subset surfaces, two negative controls).
The unit test measures redundant work and compares the full surface/tag language.

Upstream PR: none (this lane is authorized for a local commit only).

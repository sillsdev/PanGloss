# 116 — FST alpha-variable environment support

Kind: correctness/representability.
Status: open; broken until the alpha variable FST fix.

Machine binds alpha variables while traversing each environment, seeded from the target and from
the left environment. The interpretive Rust matcher is being changed to follow that ordering in
[115](115-alpha-environment-gap.md). The compiled FST backends have not been shown to preserve
all analyses produced by that binding-aware matcher, particularly when optional nodes create
multiple candidate environment matches. Existing isolated FST alpha tests do not establish that
whole-grammar property.

The FST compiler refuses a grammar containing alpha variables in a phonological rule or allomorph
environment. Tuned surface, templated underlying, and plan-composed emission all use the same
representability decision; the backend selector publishes it as well. The direct morphological
parser remains available. This guard does not implement variable-aware FST compilation. The reduced
fixture `rust/crates/pg-parse/tests/data/alpha-optional-environment.xml` and the compiler refusal
regression cover this gap; a full FST alpha fix requires a separate equivalence demonstration.

The pre-change differential inventory reported 414 observations: two envelope refusals whose
compilers succeeded and zero envelope admissions whose compilers failed. Its wrapper exited 139
after printing the passing result, so that run is not a clean gate.

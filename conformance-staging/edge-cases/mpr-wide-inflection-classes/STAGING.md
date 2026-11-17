# Wide inflection-class inventory (Closes #150)

Machine C# at `1ddef2bd2f1503717de95556116df8e4a63846d6` loads all 303 MPR
features and passes all 74 word expectations (24 accepted, 50 rejected). The signatures and
multiplicities came from that driver's `--propose` output and passed its self-check. Rust's
ordinary conformance replay now checks those complete statuses and signature multisets.
Slice 1 recorded the prior `grammar.limit.mpr-features` refusal; slice 2 widens representation.
POS and symbolic feature value caps remain 63 and are outside this change.

Ten sibling class families each have a parent and 29 children: 300 inflection classes, plus
two exception features and one entry inflection type. XML declarations are flat, as in
HCLoader's output; roots carry only their assigned class, and the parent allomorph lists its
subclasses. The nested snapshot path is tested separately in `compile/limit_tests.rs`.
Feature order fixes zero-based IDs at 63/64, 127/128, 255/256 and 299. The complete boundary
class cross-product and a class-0/class-256 negative guard against aliases. Other controls cover
inherited parent selection (Any), two required exceptions (All), excluded irregular entry types,
high-ID compound head/nonhead restrictions, and exception overwrite by derivation and compounding.

FieldWorks scope follows HCLoader's class closure, production restrictions, irregular slot
blocking and three MPR groups. No FieldWorks round-trip witness is claimed. All groups overwrite:
FieldWorks cannot produce an append group. Model tests cover append separately. C# parse replay
includes synthesis confirmation. Direct `GenerateWords` controls at the same pin check 50 boundary
pairs (7 admitted, 43 rejected); Rust repeats the same cross-product and exact surfaces.

MPR sets retain inline low words and share immutable wide words. The serde field encoding is
`{"version":1,"ids":[0,256,299]}` with ascending integer IDs; legacy numeric u64 input remains
accepted. Host outer schema versions remain unchanged because this field is self-versioned.

The fixture is staged in `edge-cases`, discovered by both generic replay and the C# driver, with
no skip or expected crash. A dedicated regression additionally requires exactly 74 checked words.
Upstream graduation and the conformance pin bump follow separately; this pair must then be removed.

```text
hc-conformance --fixtures <suite containing this edge-cases directory>
pwsh -NoProfile -File rust/tools/pg.ps1 -Mode test -Package pg-parse -TestTarget parse -Filter mpr_wide -Jobs 4
```

The compound output uses HCLoader's `+` boundary (`HCLoader.cs`,
`LoadEndoCompoundingRule`), not an inserted segment linker. The earlier fixture used a
literal `k` linker that FieldWorks cannot author; its two missing FST proposals were a
fixture/data error, not an in-scope engine defect. C# at the pinned revision replays all
74 corrected rows, including its boundary-bearing compound signatures.

The boundary correction also exposed a Rust FST confirmation bug: adjacent repeated
root tags collapse to one identity, and a filter based on the remaining root count
disabled self-compounding. Confirmation now admits compound rules under the same
lexical pins and exact identity routing. The corrected C# compound rows pin this
regression; the low-ID and 303-feature FST checks fail before that fix.

FST propose-and-confirm must retain all 24 HC identities across all 74 occurrences,
with zero missing or extra identities. The low-ID reindexing must have exactly the same
outcomes and both divergence directions. The scoreboard pins the oracle-exact cell;
the containment gate permits no new failure pair from the wide inventory.
See `docs/research/backend-scoreboard-extraction-reconciliation.md` for the census delta.

The lane report records revisions, commands and performance evidence. All lexemes are synthetic.

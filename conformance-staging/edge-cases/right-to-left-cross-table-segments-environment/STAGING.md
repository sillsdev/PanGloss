# STAGING: right-to-left-cross-table-segments-environment

Closes `plan-construct-coverage-completion` task 4.2's residual cross-table `PatternNode::Segments`
shape. The RTL rule belongs to table `t1`, while its literal right environment is explicitly segmented
against `t0`. Both tables spell `y`, but at deliberately different raw indices (`t0`: 0, `t1`: 3).
The oracle treats a cross-table Segments atom as a feature-lane constraint without a table-local
identity lane. The FST construction must preserve that source table identity and render a recall-safe
union of feature-unifiable grammar tokens; it must never reinterpret `t0`'s raw id as a `t1` id.

Pins `ey` as ROOT1's obligatory rewritten surface, rejects raw `ay`, and keeps `a` as ROOT2's
same-table control. Oracle is this repository's `pg_parse::Morpher`, per the plan's D6 discipline.
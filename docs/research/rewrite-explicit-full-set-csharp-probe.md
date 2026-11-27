# C# probe: explicit full symbol set versus an absent value (GH#85)

Run against `hc.dll`'s `SIL.Machine.dll` built from Machine `aab4c59819adb7b366915e09df2fb28c62b8c03a`. Feature `nas` has symbols `p`, `m`, default `p`. "absent" is an empty `FeatureStruct`; "full" is `nas={p,m}`. `useDefaults` is true unless stated.

| Operation | node absent | node full {p,m} |
|---|---|---|
| `node.IsUnifiable(nas=p)` | true | true |
| `node.IsUnifiable(nas=m)` | **false** (default `p`) | true |
| `node.IsUnifiable(nas=m, useDefaults:false)` | true | true |
| `node.Unify(nas=m)` | false | true, `[nas:m]` |
| `nas=p .Subsumes(node)` | true | **false** |
| `nas=m .Subsumes(node)` | false | false |
| `node.Subsumes(nas=p)` / `(nas=m)` | true / true | true / true |
| alpha variable bound from the node | `p` | `{m, p}` |
| then a second `nas=m` node against that binding | **false** | true |

HC's `Matcher` passes `MatcherSettings.UseDefaults` to arc conditions, and the rewrite rules set it, so an
explicit full set stays a present value there. The Rust rewrite engine tests unifiability and variable binding only;
`explicit_full_set_*` in `rust/crates/pg-rules/tests/rewrite_gate.rs` pin the unification and binding rows, and the
subsumption rows are recorded here as the C# verdict (PanGloss has no phonological `Subsumes` with defaults).

Known limit. `MutNode::explicit_full` is not cleared when a featured rewrite drops a node's char-def identity
(`rewrite_identity`, `add_strrep`); it is stale-retained for the rest of that rule application. Between rules it is
re-derived from the shape's `char_def`, so a node a featured rule touched reads as absent again and the default
applies (pinned by `explicit_full_set_is_dropped_between_rules_once_a_featured_rewrite_cleared_the_char_def`). This is
mostly harmless and closer to C# than before the fix; it matters only for grammars that author a full set on a
segment, which FieldWorks does not produce.

Program (a throwaway console project referencing `SIL.Machine.dll` and `SIL.Core.dll`):

```csharp
var p = new FeatureSymbol("p", "p");
var m = new FeatureSymbol("m", "m");
var nas = new SymbolicFeature("nas", p, m) { DefaultSymbolID = "p" };
var fsys = new FeatureSystem { nas };
fsys.Freeze();
FeatureStruct Abs() => FeatureStruct.New(fsys).Value;
FeatureStruct Full() => FeatureStruct.New(fsys).Symbol("p", "m").Value;
FeatureStruct Only(string s) => FeatureStruct.New(fsys).Symbol(s).Value;
// node.IsUnifiable(pat, true); node.Unify(pat, true, out o); pat.Subsumes(node, true); node.Subsumes(pat, true)
FeatureStruct Var(string n) => FeatureStruct.New(fsys).Feature("nas").EqualToVariable(n).Value;
// var vb = new VariableBindings(); node.Unify(Var("alpha"), true, vb, out o); then Only("m").Unify(Var("alpha"), true, vb.Clone(), out o2)
```

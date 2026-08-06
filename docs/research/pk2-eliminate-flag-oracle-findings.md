# `eliminate flag` C-foma oracle gate (`pg-foma/tests/pk2_eliminate_flag_oracle.rs`)

`foma-rs`'s `flag_eliminate` (the `foma` crate, pinned `=0.1.1`, `src/flags.rs`) is one of the
least-tested corners of foma: upstream bugs exist where flags interact with `_eq`
(github.com/mhulden/foma issue #60). Before any tuner enables an `Eliminate` arm, per-attribute
elimination must be equivalence-tested against the real C foma oracle; on any mismatch the design
must degrade to `AllFlags` — never to wrong.

## Method

Every network is a single Rust `&str` regex source, shared verbatim between foma-rs
(`fsm_parse_regex` + `flag_eliminate` + `ApplyHandle::up`) and real C foma 0.10.0alpha under WSL
(`wsl foma -q -f script.foma` to compile + `eliminate flag ATTR` + `save stack net.fst`, then
`wsl flookup net.fst` to batch-apply; `flookup`'s default direction is apply-up, matching
`ApplyHandle::up`, verified empirically).

For each (network, attribute) pair, four "legs" over the same fixed word list are computed and
asserted to agree as sets per word (a word with no legal analysis maps to the empty set; C-foma
prints `+?` for this, filtered out in `parse_flookup_output`):

1. **foma-rs baseline**: flags left in the network; `apply_up` obeys them (`ApplyHandle`'s
   `obey_flags` defaults to 1 in `apply_init`).
2. **foma-rs eliminated**: `flag_eliminate(opts, net, Some(attr))`, then `apply_up`.
3. **C-foma baseline**: same source, `save stack` with no elimination, `flookup`.
4. **C-foma eliminated**: same source + `eliminate flag ATTR`, `save stack`, `flookup`.

Legs 3-4 (anything that shells to `wsl`) skip gracefully when `wsl foma`/`wsl flookup` are
unavailable. Legs 1-2 (foma-rs-internal) always run. A mismatch found by this file is a successful
gate finding, not something to hide.

## The headline finding: the oracle check is necessary but not sufficient

`@E@` (`FLAG_EQUAL`) elimination is not equivalence-preserving, and this is a *different*,
separately-discovered divergence from issue #60. `foma-0.1.1/src/flags.rs`'s `flag_build` row
table (a literal bug-for-bug port of the real C table) has no rows with the eliminated flag's type
`== FLAG_EQUAL`, so eliminating an E-attribute never builds a filter — it silently degrades to
Strip (illegal paths become reachable) while still calling itself "eliminated".

E *passes* the spec's oracle check (both engines agree: eliminated `= {a,b}`, since foma-rs is a
bug-for-bug port) while *violating* the equivalence-preservation invariant (eliminated `{a,b} !=`
keepflag/baseline `{}`). A tuner that only ran the oracle check would wrongly enable `Eliminate`
for an E-tester. The real per-attribute gate must *also* assert `eliminated == baseline` within
one engine (this file computes both sides of that check for every battery). Which direction is
"wrong" here (does legit `@E.F.1@` semantics make `a`/`b` legal or illegal?) is not resolved by
this investigation — no arm is asserted safe for E, only that `Eliminate` is unsafe for it.

This generalizes structurally: `flag_build`'s table only has rows for eliminated-type U/R/D, so any
eliminated type absent from those rows (E confirmed; N/C/P are structurally identical, no rows
either) silently strips instead of eliminating. The positive verdict is scoped to U/R/D
accordingly.

## Battery coverage

- `battery_a_unify_agreement_across_stem_boundary` — Beesley & Karttunen separated dependency:
  determiner/noun NUM agreement via `@U@`.
- `battery_b_positive_require_and_disallow_combos` — `@P@`+`@R@` and `@P@`+`@D@`.
- `battery_c_three_independent_attributes_chained_elimination` — three flags (one pair with a
  prefix-colliding name, `NUM`/`NUMBER`, to stress `flag_purge`'s name-boundary guard) eliminated
  one at a time, checked at every checkpoint (Karttunen-style chain).
- `battery_d_flags_coexist_with_multichar_tags` — `<R:0001>`-shaped tag symbols alongside flags;
  asserts elimination never touches the tag.
- `battery_e_reduplication_shaped_flags_and_affix_issue60_risk` — the closest reproducible analog
  of issue #60's crash shape (flag diacritics + a reduplication-shaped stem + affixation). True
  generative reduplication is not a regular-language operation foma-rs/C-foma's regex parser
  exposes (and is out of pg-foma's FST scope — reduplication stays the peel), so this uses finite
  pre-copied stems (`catcat`, `dogdog`) standing in for a reduplicated shape. `_eq` in issue #60
  turned out to be the reporter's own xfst function name, not a foma builtin (confirmed via the
  issue text) — there is no `_eq(...)` construct in foma-rs's regex parser to substitute for.
- `rs_flags_obeyed_by_default_baseline` — the load-bearing assumption every leg above depends on,
  checked first.
- `e_flag_type_elimination_not_equivalence_preserving` — the headline E-flag finding above.

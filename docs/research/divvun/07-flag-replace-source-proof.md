# Flags × replace calculus in `foma-rs`: a source-level proof, not an experiment

Research agent 7. Scope per brief: read `foma-rs` and upstream C `foma` source directly to
determine, mechanistically, how flag diacritics and the replace (`->`/`<-`) calculus interact, and
adjudicate `pg-foma/src/gate.rs`'s three findings against that mechanism. **No code was changed, no
build was run** (`cargo`, `pg.ps1`) — every claim below is either a direct citation of source read
this session, or is explicitly marked as inference the source alone cannot close.

Sources read in full or in the cited part, this session:

- `C:/Users/johnm/Documents/repos/PanGloss/.claude/worktrees/divvun-research/rust/crates/pg-foma/src/gate.rs` (full)
- `C:/Users/johnm/Documents/repos/PanGloss/.claude/worktrees/divvun-research/rust/crates/pg-foma/tests/f0_viability.rs` (full)
- `C:/Users/johnm/Documents/repos/PanGloss/.claude/worktrees/divvun-research/rust/crates/pg-foma/tests/pk2_eliminate_flag_oracle.rs` (grepped for `->`, structure confirmed)
- `C:/Users/johnm/Documents/repos/PanGloss/.claude/worktrees/divvun-research/rust/crates/pg-foma/Cargo.toml` (full, for version-pin history)
- `C:/Users/johnm/Documents/repos/foma-rs/crates/foma/src/flags.rs` (full, 1308 lines)
- `C:/Users/johnm/Documents/repos/foma-rs/crates/foma/src/rewrite.rs` (read in full: lines 1–1403 and 1404–1533; remaining ~550 lines are the `rewr_notleftmost`/`rewr_notlongest`/markup-rule tail, structurally the same class of construction already characterized)
- `C:/Users/johnm/Documents/repos/foma-rs/crates/foma/src/apply.rs` (targeted reads: 930–1150, 1500–1700, plus grep of every `flag`/`obey_flags` hit)
- `C:/Users/johnm/Documents/repos/foma-rs/crates/foma/src/options.rs` (full)
- `C:/Users/johnm/Documents/repos/foma-rs/crates/foma/src/constructions/products.rs` (full, 871 lines — `fsm_compose`, `fsm_intersect`, `fsm_cross_product`)
- `C:/Users/johnm/Documents/repos/foma-rs/crates/foma/src/constructions/merge_sigma.rs` (`fsm_merge_sigma`, lines 285–330-ish)
- `C:/Users/johnm/Documents/repos/foma-rs/crates/foma/src/iface/unary.rs` (grep for `eliminate`/`twosided`)
- Upstream C: `C:/Users/johnm/Documents/repos/foma/foma/apply.c` (lines 1055–1120, plus grep of every `flag`/`obey_flags` hit), `constructions.c` (grep of `flag_is_epsilon`/`FLAG`), `rewrite.c` (grep of `flag` — zero hits), `mem.c` (global defaults)
- Prior reports `00`, `03`, `05` in this directory, for context (not re-derived from memory —
  used only to know what was already established and what needed closing)

I did not read `foma-rs`'s `minimize.rs` line-by-line for the crash (see §5); I did grep it for
`unsafe` (zero hits) as a diagnostic, discussed in §5.

---

## 1. Two independent subsystems, not one

The central fact this session establishes, read directly from source, is that **foma has no single
"flag diacritic" concept implemented once** — it has three separate subsystems that all key off the
same textual convention (`flag_check`'s DFA, `flags.rs:473-608`, VERIFIED, byte-identical port of
`apply.c`'s equivalent — the C has the identical state-machine shape, confirmed by reading
`apply.c` for the same `flag_check`-driven branches, though `flag_check` itself lives in `flags.c`
in C, not grepped line-by-line this session), but never reconcile with each other structurally:

| Subsystem | What it does | Where | When it runs |
|---|---|---|---|
| **Compile-time replace algebra** (`fsm_rewrite`) | Builds a 4-tape Kaplan–Kay-style rewrite automaton from LHS/RHS/context, treating **every** alphabet symbol — flag-shaped or not — as an ordinary member participating in `fsm_concat`/`fsm_intersect`/`fsm_minus`/`fsm_complement` | `foma-rs/crates/foma/src/rewrite.rs` (VERIFIED: zero mentions of "flag" anywhere in the file, confirmed by reading it and by the fact `flags`/`flag_check`/`FlagType` are never imported — the file's own `use` list at `rewrite.rs:11-24` has no reference to `crate::flags`) | Rule compile time, once, producing a final 2-tape `Fsm` |
| **Compose-time epsilon option** (`fsm_compose`) | An opt-in special case: if `flag_is_epsilon` is true, flag symbols are pre-seeded into both nets' sigmas before merge and treated as skippable (epsilon-like) via dedicated code paths | `foma-rs/crates/foma/src/constructions/products.rs:167-636`, esp. `225-277`, `443-524` | Whenever two `Fsm`s are composed, controlled per-call by `FomaOptions.flag_is_epsilon` |
| **Apply-time interpreter** (`apply_match_length`/`apply_match_str`) | Any arc whose label is `flag_check`-shaped is **zero-width** unconditionally (consumes 0 real input characters) and its "success" is decided by a flag-consistency state machine (`apply_check_flag`), never by matching the literal symbol against real tape content | `foma-rs/crates/foma/src/apply.rs:1510-1586` | Every traversal step of `apply_up`/`apply_down`, on the **final**, already-compiled 2-tape network |
| **Compile-time elimination** (`flag_eliminate`/`flag_purge`/`flag_twosided`) | Bakes the apply-time semantics into ordinary automaton structure once, by composing FAIL/SUCCEED filters and then stripping the flag symbols to epsilon | `foma-rs/crates/foma/src/flags.rs:61-266` (`flag_eliminate`), `389-446` (`flag_purge`), `688-807` (`flag_twosided`) | Only when explicitly invoked (`eliminate flag`/`eliminate flags`/`twosided flag-diacritics`, `foma-rs/crates/foma/src/iface/unary.rs:82,141,365` — VERIFIED, these are ordinary CLI commands, never called automatically by `fsm_rewrite` or `fsm_lexc_parse_string`) |

The mismatch that produces `gate.rs`'s finding 1 is the seam between subsystem 1 and subsystem 3:
**the compile-time replace algebra assumes a flag literal is ordinary tape content requiring exact
positional presence; the apply-time interpreter unconditionally overrides that assumption for
*any* symbol shaped like a flag, everywhere in the final network, regardless of which construction
put it there.**

---

## 2. The compile-time side: flags are ordinary symbols to `fsm_rewrite` — VERIFIED

`fsm_rewrite` (`rewrite.rs:78-605`) builds the classical four-tape encoding (position class / rule
number / input symbol / output symbol, module doc `rewrite.rs:6-9`). Every symbol that appears in a
rule's `left`/`right`/`right2`, or in a context's `left`/`right`, is run through
`rewrite_add_special_syms` (`rewrite.rs:1428-1455`) which only adds the rewrite calculus's own
bookkeeping symbols (`@O@`, `@I@`, `@I[@`, `@I]@`, `@I[]@`, `@ID@`, `@#@`, `@#0001@`-style rule
tags — `SPECIALSYMBOLS`, `rewrite.rs:71-72`) to the alphabet; it does **not** inspect the symbol
for flag-shapedness. A flag literal such as `@D.MPR1@` used inside a rule's context or center is
therefore an ordinary alphabet member throughout: it flows through `rewrite_cp`/`rewrite_cp_markup`
(center cross-product + alignment, `rewrite.rs:1399-1426`), through `rewrite_upper`/`rewrite_lower`
(context-direction lowering, `rewrite.rs:889-1214`), and — only if the rule has a `||` context at
all (`rewrite_contexts.is_some()`, gated at `rewrite.rs:383`) — through `rewr_context_restrict`
(`rewrite.rs:1486-1533+`), which builds `NotContain(cpleft ++ C ++ cpright)` via ordinary
`fsm_concat`/`fsm_union` and subtracts it from the running `base` language with `fsm_minus`
(`rewrite.rs:392,502,505,552,567`).

**None of this machinery ever imports `crate::flags` or calls `flag_check`.** The upstream C is
identical on this point: `grep flag rewrite.c` returns **zero matches** (VERIFIED, this session).
The C rewrite calculus was written entirely independently of the flag-diacritic apply-time
machinery — they are Kaplan & Kay's replace calculus and Karttunen's flag diacritics, two published
mechanisms from different papers, implemented in different files, with no cross-awareness. This is
the first half of the mismatch, and it is **identical in C and in the port** — a design property of
classical foma, not a port regression.

---

## 3. The apply-time side: flags are zero-width state tests, unconditionally — VERIFIED, both languages

`apply_match_length` and `apply_match_str` are the two functions that decide, for an arc's input
symbol, how many real input characters it consumes and whether it succeeds. Both check
`flag_check`-shapedness **first**, before anything else that would look at the real input string:

```rust
// foma-rs/crates/foma/src/apply.rs:1510-1516
pub fn apply_match_length(h: &ApplyHandle, symbol: i32) -> i32 {
    if symbol == EPSILON { return 0; }
    if h.has_flags && !h.flag_lookup[symbol as usize].r#type.is_empty() {
        return 0;                                    // <-- zero-width, no input consulted
    }
    ...
```

```rust
// foma-rs/crates/foma/src/apply.rs:1559-1572 (apply_match_str, non-ENUMERATE branch)
if h.has_flags && !h.flag_lookup[symbol as usize].r#type.is_empty() {
    if !h.obey_flags { return 0; }
    ...
    if apply_check_flag(h, ftype, fname.as_deref(), fvalue.as_deref()) == FlagCheck::Succeed {
        return 0;                                    // success: consumed 0 real chars
    } else {
        return -1;                                   // failure: NOT because the real tape lacked it
    }
}
```

The upstream C is **byte-for-byte the same logic**, down to the comment:

```c
// foma/foma/apply.c:1082-1116
/* Match a symbol from sigma against the current position in string */
/* Return the number of symbols consumed in input string            */
/* For flags, we consume 0 symbols of the input string, naturally   */
int apply_match_str(struct apply_handle *h, int symbol, int position) {
    ...
    if (h->has_flags && (h->flag_lookup+symbol)->type) {
	if (!h->obey_flags) { return 0; }
	if (apply_check_flag(...) == SUCCEED) { return 0; } else { return -1; }
    }
    ...
```

`h.has_flags`/`h.flag_lookup` are populated purely syntactically — any symbol in the final
network's sigma whose spelling matches `flag_check`'s grammar is marked flag-typed
(`apply.rs:1693-1718`, mirrored in C at `apply.c` around its own `has_flags` initialization,
grepped and confirmed present), **with no memory of which construction step put that symbol there
or what role it was compiled to play.** `obey_flags` defaults to `true` in both languages
(`foma-rs/crates/foma/src/apply.rs:538`, `h.obey_flags = true;`; C `foma/foma/mem.c:24`,
`int g_obey_flags = 1;`), so this override is always active unless a caller explicitly disables it.

**This is the second half of the mismatch, and it too is identical in C and in the port.**

---

## 4. Why this produces exactly `gate.rs`'s finding 1 — INFERRED, from the two VERIFIED halves above

`gate.rs`'s finding 1 (`pg-foma/src/gate.rs:14-23`) reports that a flag literal inside a replace
rule's `||` context, or inside its LHS/RHS center, compiles without error but produces a
nondeterministic mix of "fired"/"didn't fire" apply results for the same input, and that a context
consisting solely of a flag literal additionally crashes the minimizer.

The mechanism, reconstructed from §2 and §3: `fsm_rewrite`'s context-restriction and
center-alignment machinery (`rewr_context_restrict`, `rewrite_cp`) compile the requirement "this
tape position must contain literal symbol `@D.MPR1@`" into the automaton exactly as it would
compile "this position must contain literal symbol `x`" — as an ordinary equality test, contributing
one real tape slot to every downstream length/alignment/leftmost/longest-match calculation that
`rewrite.rs` performs (`rewrite_tape_m_to_n_of_k`, `rewr_notleftmost`, `rewr_notlongest`, all of
which treat one arc as one tape position uniformly, with no flag exception). That assumption is
**sound as pure automaton algebra** — the compiled result correctly denotes "context holds iff this
exact symbol sequence, flag literal included, is present" as a formal language.

The mismatch appears only when the **final** compiled network is handed to `apply_up`/`apply_down`.
At that point, `h.has_flags` is true (the network's sigma contains a `flag_check`-shaped symbol),
and **every** arc bearing that symbol — including the one the compile-time context-restriction
construction relied on behaving like an ordinary literal — is reinterpreted as zero-width and
tested against `h.flag_state` instead of the real input. The compiled automaton's paths, which were
built assuming ordinary equality-testing, now admit or reject strings based on **flag-consistency
state accumulated along whichever path was taken so far**, not on whether the literal flag symbol
is actually present in the real underlying/surface tape at that position. Because the *rest* of
the base cascade (`fsm_kleene_star(rule_cp | outside)`, `rewrite.rs:248-259`) still offers
alternative paths built under the "ordinary symbol" assumption, the same input can now satisfy the
rewritten-branch's context via one path (flag state happens to be consistent) and the
unrewritten/outside branch via another (flag literal wasn't really there, but the compile-time
subtraction that was supposed to forbid this coexistence was computed under a different symbol
semantics than apply time now uses) — which is precisely "a nondeterministic mix of fired and
not-fired paths for the same input."

This account is **inferred**, not independently re-run: I did not compile or apply
`t -> 0 || a "@D.MPR1@" _` myself (forbidden by scope). It is offered because it is the only
account consistent with everything VERIFIED in §2–§3, and because it makes a falsifiable, precise
prediction (§7's Experiment A′).

**A structural prediction this account makes, and that a second, independent read confirms:**
finding 1 should depend on whether the rule has a `||` context or center pattern that must *match*
real content containing the flag literal, not merely on "flag inside a `->`/`<-` rule at all." §6
tests this prediction directly against the GiellaLT idiom and finds it holds structurally.

---

## 5. The minimizer crash — NOT settled by source reading

`gate.rs:20-22` reports a crash, `STATUS_STACK_BUFFER_OVERRUN`, inside `minimize.rs`, specifically
for "a context consisting of JUST a flag literal (no real segment)." I read `minimize.rs`'s public
surface and grepped it for `unsafe`: **zero hits** (VERIFIED). `STATUS_STACK_BUFFER_OVERRUN` is a
Windows `/GS` stack-cookie violation, ordinarily associated with either genuine unsafe-code buffer
overruns or a corrupted stack from unrelated causes (e.g., a deep-recursion stack overflow that
manifests through the same OS-level fault path). Since `minimize.rs` itself contains no `unsafe`
Rust, a classic Rust buffer-overrun in *this* file specifically is unlikely; the more plausible
reading (not verified) is a recursion-depth issue in whatever minimization strategy runs
(`fsm_minimize`, `minimize.rs:154`, Hopcroft by default per `FomaOptions::minimize_hopcroft = true`,
`options.rs:94`) on a pathologically small/degenerate automaton shape that the context-only-flag
construction happens to produce. **This is exactly the kind of thing source reading cannot settle**:
whether the crash is a genuine Rust-port memory-safety bug, a stack-depth limit that C would hit
too (just manifesting as a segfault or silent corruption instead of a labeled Windows exception), or
something else entirely, requires the actual repro and a debugger/backtrace — which this brief
excludes. Stated as **unknown**, not guessed; see §8 for the minimal experiment.

---

## 6. Is the Divvun idiom safe under this port? YES — for a source-verifiable structural reason

The GiellaLT filter (`remove-illegal-derivation-strings-flagbased.regex`, cited by report `03`) uses
the shape:

```
"@D.Der1.TRUE@" "@D.Der2.TRUE@" ... "@P.Der1.TRUE@" "+Der1" <- "+Der1" ,
```

This is `UPPER <- LOWER` with **no `||` at all** — `<-` is `ArrowType::LEFT` in this port
(`rewrite.rs:410-424` gates the *obligatory-application* check on `right` for `LEFT`-type rules,
i.e. the *matched* side is the lower/right side, here `"+Der1"`; the *inserted* side is the upper
side, here the flag sequence followed by `"+Der1"`). Structurally:

- **`LOWER = "+Der1"`** is the side that must *match real, pre-existing tape content* — an ordinary
  multichar tag, not flag-shaped, so `flag_check` never fires on it and apply-time's zero-width
  override never touches this side.
- **`UPPER = flags + "+Der1"`** is the side that gets *freely inserted* — the flags here are pure
  output material, never required to match anything against real input at the point of insertion.
- **There is no `rewrite_contexts` at all** (no `||`): `regex.rs`'s parser only populates
  `RewriteSet.rewrite_contexts`/builds `Fsmcontexts` nodes from an explicit `||` clause
  (`regex.rs:646-682` for the general case, `regex.rs:708-730` for another rule shape) — VERIFIED
  by reading the relevant construction sites. A rule with no `||` therefore compiles with
  `rewrite_contexts: None`, and `rewr_context_restrict` — the function whose `NotContain`-based
  subtraction is where §4's mismatch bites — is **never invoked at all** for this rule
  (`rewrite.rs:383`, `if rewrite_contexts.is_some() { ... }`, skipped entirely).
- The only construction this rule *does* go through besides the base obligatory-rewrite check is
  `rewrite_cp` (`rewrite.rs:1399-1426`, center cross-product/alignment) — which treats the flag
  literals exactly as it would treat any other inserted multichar symbol (e.g. `"+Der1"` itself),
  because `rewrite_cp`/`rewrite_align` have no flag-awareness either (§2) and do not depend on the
  inserted symbol being *matched* against anything — insertion is unconditional given the LHS match.

**Net: the toolkit defect in `gate.rs`'s finding 1 is specific to a flag literal appearing on the
*matched* side of a `->`/`<-` construction — inside a `||` context, or inside the LHS/RHS center
pattern in a role that requires it to be matched against real tape content — because that is the
only role that invokes `rewr_context_restrict`'s `NotContain`/subtraction machinery, the seam where
§4's compile-time-vs-apply-time mismatch produces wrong results.** A flag literal that is purely
*inserted* output material, with no `||` context at all, never reaches that machinery, and there is
no source-level reason to expect it to misbehave. This reconciles `gate.rs`'s finding with
GiellaLT's production usage **without contradiction on either side** — and refines the brief's own
working hypothesis (replacement vs. context) into its precise, verifiable form: **the safe/unsafe
line is "does this flag occurrence require compile-time matching against real tape content," not
merely "which side of the arrow is it written on."** (A flag placed in a rule's RHS but in a role
that *is* matched — e.g., part of an obligatory `LEFT`/`RIGHT` "unrewritten" check that still runs
even without `||`, `rewrite.rs:396-424,493-507` — would still need checking; the GiellaLT example
avoids this because the flags are prefixed onto, not substituted for, the matched tag, so the
"unrewritten" check's `fsm_minus(base, rewr_contains(unrewritten("+Der1")))` still only ever needs
to test the plain tag "+Der1", never the flag literals themselves, on the matched side.)

**What `pg-foma` would need to change to use this idiom directly: nothing new, structurally.**
`pg-foma`'s own replace-rule compiler already emits ordinary `->`/`<-`-shaped rules
(`crate::replace`, per gate.rs's own references). The Divvun idiom — flags inserted as a prefix to
an existing tag by a *context-free* insertion rule — is compilable with exactly the primitives
`pg-foma` already trusts elsewhere (plain lexc, plain `->` rules with no flags,
[`fsm_compose`], [`fsm_union`] — `gate.rs:51-53`'s own list). The one thing that must be verified
(not assumed) before relying on it is that the *later* rule/filter stage that actually *tests* the
inserted flags (an `@R.../@D...@` occurring on the matched side of some downstream rule, or read at
plain apply time with no rule involved at all) never needs those flags to participate in a `||`
context or an LHS/RHS matched-role — which is exactly what `entry_gate_key`/static partitioning in
`gate.rs` already sidesteps by not using flags for *testing* at all. **Combining "flags inserted by
a context-free `<-` rule" with "flags read back via plain apply-time traversal, never via a second
`->`/`<-` rule's matched side" is, on this session's source reading, safe. Combining flag-testing
*with* a second replace rule's `||`/matched-role usage is not, and remains the true scope of
finding 1.**

---

## 7. Finding 2 — `fsm_compose` and `flag_is_epsilon` — CONFIRMED, inherited from C, and it is documented, correct-by-design behavior, not a bug

Read in full (`products.rs:167-636`). With the default `flag_is_epsilon = false`
(`options.rs:83`, `FomaOptions::default()`), `fsm_compose`'s only flag-specific code paths are
gated `if g_flag_is_epsilon` (`products.rs:231,269,449,536`) and are **skipped entirely** when it
is off. The ordinary product-construction loop matches arcs by plain symbol equality
(`if bin == aout ...`, `products.rs:408`) with separate real-epsilon-only branches
(`aout == EPSILON`, `products.rs:443`; `bin == EPSILON`, `products.rs:529`) — a flag symbol is
just another ordinary alphabet member here, with **no** special skip/pass-through behavior.

`fsm_merge_sigma` (`constructions/merge_sigma.rs:285-330+`) only harmonizes **symbol numbering**
across the two nets' sigmas — it adds a numbering entry for a symbol missing from one side, but it
**never adds an arc**. So composing a flag-free net `[a]` with a flag-bearing net
`[a "@D.MPR1@"]`: after sigma merge, `[a]`'s sigma nominally includes `@D.MPR1@`, but `[a]` has no
arc carrying it. The product-construction loop can never find a matching transition for the
position requiring `@D.MPR1@` in the other net (no `bin==aout` match, and neither side is a real
`EPSILON`), so the composed language is empty for that path — reproducing gate.rs's
`compose([a], [a "@D.MPR1@"])` = `{}` finding **exactly**, VERIFIED mechanistically from source.

Turning `flag_is_epsilon` on fixes precisely this case (`products.rs:231-264` pre-seeds each net's
sigma with the other's flag symbols so `UNKNOWN`/`IDENTITY` don't clash with them, then
`products.rs:449,536` add dedicated epsilon-like transition-following for flag-labeled arcs) — but
the code's own `tracing::warn!` (`products.rs:259-263`) says this "may yield incorrect results" if
**both** sides carry flags, matching gate.rs's own observation that turning it on does not fix
finding 1.

**C comparison — identical, confirmed by direct read:**

```c
// foma/foma/mem.c:24-25
int g_obey_flags = 1;
int g_flag_is_epsilon = 0;
```

```c
// foma/foma/constructions.c:546  (fsm_compose)
extern int g_compose_tristate, g_flag_is_epsilon;
...
// :570, :605  -- identical "pre-seed sigma if flag_is_epsilon" guard
// :744, :786  -- identical "aout != EPSILON && g_flag_is_epsilon == 0" / "bin != EPSILON && ..." guards
// :748, :791  -- identical "g_flag_is_epsilon && ... is_flag" epsilon-following branches
```

Every gate the Rust port checks, the C checks at the structurally corresponding point, with the
same default. **Finding 2 is confirmed exactly as stated in `gate.rs`, and it is a documented,
intentional design in both C foma and the port — not a bug in either, and not a port regression.**
It is "surprising" only if one expects flag symbols to be epsilon-transparent by default; the
option name (`flag-is-epsilon`) and its default-off state say otherwise in both languages.

---

## 8. Finding 3 — the Kleene-star shadow workaround's ordering fragility — CONFIRMED, same root cause as finding 1, inherited

`gate.rs:34-47` describes a workaround (route flags out of any `->` construct via a
`[[c "@D.F@"] | [c:c_shadow "@R.F@"] | \c]*` shadow transform) that still misbehaves once composed
with a real lexc net, root-caused to flag state being "exactly whatever the last `@P@` on this path
assigned," a strictly left-to-right, per-path property. This is not a *new* mechanism — it is the
direct consequence of §3's apply-time semantics (`apply_match_length`/`apply_match_str` treat flags
as zero-width, path-order-dependent state tests, identical in C and the port) interacting with
however many *distinct paths* a composed lexc+rule network actually offers through the shadowed
region. Nothing in `flags.rs`'s data structures (`Flags`, a singly-linked list built purely from
`flag_extract`'s left-to-right sigma scan, `flags.rs:452-467`) or in `apply.rs`'s traversal
maintains anything beyond "whatever was last set/tested on *this* path" — there is no separate
notion of tape *position* the flag mechanism could use to disambiguate "set before" from
"set after" other than actual left-to-right traversal order, which is exactly what the workaround
already relied on and exactly what a real, branchier lexc network can defeat by offering an
alternate path ordering the hand-built probe net didn't have. **Confirmed, and inherited from C's
identical flag/apply design — not a port-specific defect.** I did not attempt to isolate the exact
lexc interaction that broke the probe (gate.rs itself says "root cause not fully isolated"); this
session adds the structural reason such fragility is *expected* given §3, not a new isolation of
the specific failing case.

---

## 9. Verdicts, summarized

| # | `gate.rs` claim | Verdict | Inherited from C, or port regression? |
|---|---|---|---|
| 1 | Flag literal in a `->`/`<-` rule's `||` context (or LHS/RHS matched role) corrupts apply results; bare-flag context crashes the minimizer | **Confirmed, but over-scoped as "`->` and flags do not mix safely in this port, full stop."** The true scope is narrower: the defect is in flags occupying a *matched* role (context, or LHS/RHS content that participates in `rewr_context_restrict`'s or the obligatory-rewrite check's real-content matching) — VERIFIED mechanistically in §4/§6. A flag occupying a purely *inserted* role in a context-free `<-`/`->` rule (no `||`, not part of an "unrewritten" check on the flag itself) is not shown to be affected, and the GiellaLT idiom's structure (§6) avoids the affected role entirely. | The two subsystems that collide (§2's flag-blind replace algebra, §3's flag-aware apply-time override) are **structurally identical in C and in the port** (rewrite.c has zero flag-awareness, apply.c's zero-width flag test is byte-identical to the Rust port's). The *mismatch* is therefore inherited-from-C-by-architecture. The **crash** specifically (§5) is not settled either way from source reading alone. |
| 2 | `fsm_compose` is not flag-epsilon-transparent by default; a flag-free net composed with a flag-bearing net (flag never set) returns empty, not the vacuous pass | **Confirmed exactly as stated.** Documented, intentional, matches the option's own name and default. Not a bug. | **Inherited from C**, byte-for-byte — same default (`mem.c:24-25`), same gated code paths in `constructions.c` at the structurally corresponding lines. |
| 3 | The Kleene-star flag-shadow workaround is fragile because flag state is exactly "whatever the last `@P@` on this path assigned," position-dependent on left-to-right traversal | **Confirmed**, and shown here to be the same root cause as finding 1 (§3's apply-time semantics), not a separate defect. | **Inherited from C** — same `apply_match_length`/`apply_match_str`/`Flags`-list design in both languages; no port-specific state-tracking difference found. |

**None of the three findings is shown, from source reading, to be a `foma-rs`-specific port
regression.** All three trace to design properties `foma-rs` faithfully ported from Mans Hulden's C
foma: the replace calculus and the flag-diacritic apply-time machinery were built independently of
each other in the original, and nothing in either language reconciles them when a flag occupies a
role the replace calculus's compile-time algebra needs to treat as literal, matched content.

---

## 10. Direct answer to the brief's central question

**Is the Divvun idiom (flags in the replacement of a `<-` rule, later read back at plain apply
time) safe under this port? Yes**, on the structural grounds in §6: it has no `||` context, the
flags occupy a purely inserted role never subject to `rewr_context_restrict`'s or the
obligatory-rewrite check's real-content matching, and both of those constructions are the only
places §2–§4's mismatch has been shown (mechanistically, not yet by running code) to bite. `pg-foma`
needs no new primitive to use this idiom — its existing plain-lexc / plain-`->`-with-no-flags /
`fsm_compose` / `fsm_union` toolkit (`gate.rs:51-53`) already suffices, provided the *reading* side
of the flags (wherever `@R.../@D...@` are tested) is likewise kept out of a `||` context or a
matched LHS/RHS role — i.e., tested by plain apply-time traversal (which is exactly how flag
diacritics are meant to work, per §3) or by a second context-free rule, never by embedding the test
inside a second rule's environment.

**If PanGloss wants flag-based MPR/POS gating back on the table** (currently shelved in favor of
the static, flag-free partition in `gate.rs`), the source reading here says the shelving was
correct for the *specific* construction attempted (flag test inside a rule's own `||` environment)
but does not foreclose the *insertion-then-plain-apply-time-test* idiom GiellaLT actually uses,
which was never attempted per `gate.rs`'s own module doc and per the confirmed gap in the test
suite (§11).

---

## 11. `gate.rs`'s "only ever test flags OUTSIDE any `->` construct" — verified true, precisely

- `tests/f0_viability.rs`'s only replace-rule test, `regex_compose_recovers_underlying_form`
  (`f0_viability.rs:230-256`), compiles `N -> m || _ [p|b]` (`f0_viability.rs:225`) with **no flag
  symbols anywhere in the rule**.
- The same file's flag tests, `flags_gate_paths_under_apply_up` and
  `flags_hidden_by_default_shown_when_enabled` (`f0_viability.rs:266-301`, `303-319`), use **plain
  concatenation regexes** (`[a "@U.F.1@" | b "@U.F.2@"] [c "@R.F.1@" | d "@R.F.2@"]`,
  `f0_viability.rs:275`) — no `->`/`<-` anywhere.
- `tests/pk2_eliminate_flag_oracle.rs` was grepped for the replace arrow: **zero occurrences** of
  `->` as a regex operator (the only hits are ASCII arrows inside prose comments and a
  `NUM -> NUMBER -> CASE` elimination-chain comment, `pk2_eliminate_flag_oracle.rs:541`, not
  compiled regex syntax).

**The untested region, characterized precisely:** any grammar construct where a flag-shaped
symbol (`flag_check`-matching) occurs (a) inside a `->`/`<-` rule's `||` context, or (b) inside such
a rule's LHS/RHS in a role subject to `rewr_context_restrict` or the obligatory-rewrite
"unrewritten" check (`rewrite.rs:396-424,493-507`) — as opposed to a purely inserted role in a
context-free rule (§6). No test in this codebase exercises either (a) or (b); `gate.rs`'s own
in-module probes (described but not committed, `gate.rs:9-13`) are the only record of (a)/(b) being
tried, and they were discarded rather than kept as regression tests.

---

## 12. Draft upstream issue — recommendation: do not file finding 1 or 2 as bugs; consider filing the crash

Findings 1 and 2, per §9, are **inherited, documented, correct-by-design** behavior that both C
foma and `foma-rs` share deliberately. Filing them as "bugs" against `divvun/foma-rs` would be
inaccurate and likely to be closed as working-as-intended (the option names and defaults say so
explicitly in both languages). What *would* be worth filing, if the crash in §5 is real and
reproduces on the current pinned version (`foma = "=0.4.2"`, `pg-foma/Cargo.toml:23` — the crash was
originally observed against `=0.1.1` per `gate.rs`'s own module doc, and this session did not
re-run it against 0.4.2; the pg-foma team's own Cargo.toml comment states 0.4.1/0.4.2 are
"untrusted-input hardening fixes only... byte-identical behavior for every previously-working net,"
which is *some* evidence the crash — if real — likely still reproduces, since a crash implies the
net was never "previously working" in the first place):

> **Title:** Minimizer crash (`STATUS_STACK_BUFFER_OVERRUN`) on a replace-rule context consisting
> solely of a flag-diacritic literal
>
> **Minimal reproducing expression (not re-run this session — reconstructed from
> `pg-foma/src/gate.rs:14-23`'s description):**
> ```
> a -> b || "@D.MPR1@" _
> ```
> or the equivalent grouped form `a -> b || ["@D.MPR1@"] _`, compiled with `fsm_parse_regex`, then
> `apply_up`/`apply_down` called on any input.
>
> **Expected:** either a clean compile error (flags are not valid as a sole context constituent, if
> that is an intentional restriction) or a deterministic apply result consistent with the pure
> formal-language reading of the rule (§2's compile-time semantics).
>
> **Actual (as reported by `gate.rs`, not independently re-run here):** compiles without error;
> `apply_up` crashes with a stack-buffer-overrun-shaped exception inside `minimize.rs`.
>
> **C-side comparison:** not established. `rewrite.c` (grepped, zero flag-awareness, confirmed §2)
> and `apply.c` (byte-identical zero-width flag semantics, confirmed §3) both have the *same
> mismatch* available to trigger; whether the *specific* minimizer path crashes identically in C
> foma 0.10.0alpha, degrades gracefully, or silently produces a wrong-but-non-crashing automaton is
> genuinely unknown from source reading (`minimize.c`'s equivalent function was not read this
> session — this is a gap; see §13). **Do not file this upstream without first re-confirming the
> crash reproduces on the currently-pinned `foma = 0.4.2`**, since the only record of it is against
> `0.1.1` and this session could not re-run it.

---

## 13. What source reading alone cannot settle, and the minimal experiment for each

1. **Does the finding-1 crash still reproduce on `foma = 0.4.2`** (the version `pg-foma` now pins,
   vs. the `0.1.1` the finding was originally made against)? *Minimal experiment:* compile
   `a -> b || "@D.MPR1@" _` with the pinned crate version and call `apply_up` on any short input;
   observe compile result and whether the crash reproduces. Not run this session (forbidden by
   scope).
2. **Does §4's inferred mechanism (compile-time-ordinary vs. apply-time-zero-width mismatch)
   actually produce the specific nondeterministic fired/not-fired symptom**, or is there a second,
   independent cause? *Minimal experiment:* compile the minimal case above without a crash-prone
   bare-flag context (e.g. `a -> b || x "@D.MPR1@" _` with a real segment `x` alongside the flag, as
   `gate.rs:16` describes as the non-crashing variant), then call `apply_up`/`apply_down` on inputs
   that do and do not contain the literal flag symbol in the surface string, and inspect whether
   the returned path set matches what §4 predicts (paths gated by accumulated flag-consistency
   state, not by literal tape presence).
3. **Does the GiellaLT idiom (§6) actually compile and gate correctly end-to-end in this port**,
   as Experiment A in `docs/research/divvun/00-synthesis-and-decision.md` §5 already proposes?
   *Minimal experiment:* take `remove-illegal-derivation-strings-flagbased.regex` verbatim (or a
   minimal excerpt — the `Der1`/`Der2` pair alone), compile it under `foma-rs`, and apply it against
   an ascending-order and a descending-order derivation string. This is the single highest-value
   remaining experiment: it would convert §6's structural argument into a directly demonstrated
   result. Not run this session (forbidden by scope).
4. **Is the crash a genuine Rust-port memory-safety issue, a recursion/stack-depth issue shared
   with C, or something else?** *Minimal experiment:* run the crashing case under a debugger (or
   with backtrace capture) to get the actual panic/fault location inside `minimize.rs`, and compare
   against C foma 0.10.0alpha's `minimize.c` on the same input (as `pk2_eliminate_flag_oracle.rs`'s
   own WSL-based cross-engine harness already does for the elimination-correctness question — the
   same harness shape could be reused here). Not run this session; `minimize.rs`'s own recursion
   structure was not fully traced (only grepped for `unsafe`, which returned zero hits).
5. **Whether a flag occupying an LHS/RHS *matched* role (as opposed to context, and as opposed to
   GiellaLT's purely-inserted role) in a rule with no `||` at all also misbehaves** — §6's argument
   covers the no-`||`-and-purely-inserted case exactly; it does not by itself rule out a mismatch in
   the no-`||`-but-matched case (e.g. a rule that deletes a flag literal: `"@D.X@" -> 0`, which is
   matched, not inserted, but still has no context). *Minimal experiment:* compile that case and
   check whether `apply_up`/`apply_down` behave consistently with §2's pure compile-time reading.

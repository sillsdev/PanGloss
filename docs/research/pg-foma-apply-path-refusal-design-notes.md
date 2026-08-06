# pg-foma apply_path_refusal_gate.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-foma/tests/apply_path_refusal_gate.rs` implementation
comments so the source can carry a one- or two-line pointer instead of the full argument. Each
section corresponds to one call site; the site names the function/type so this doc can be found from
either direction.

## Module doc: the `deep-optional-affix-nesting`/`recipe-template-generic` process-abort diagnosis

Two fixtures — `machine:edge-cases/deep-optional-affix-nesting` and
`staging:edge-cases/recipe-template-generic` — used to abort the whole test process instead of
failing a test: `memory allocation of 52 bytes failed`, then exit `0xc0000409`. They are one grammar:
`diff` shows the two `grammar.xml` files differ only in their `<Name>` element (and the staged copy's
trailing newline), and the two `words.yaml` files only in their `language:` line — never two
independent bugs.

`0xc0000409` is `STATUS_STACK_BUFFER_OVERRUN`, which is both what Rust's stack-overflow handler
produces and what MSVC's `abort()` produces via `__fastfail`. The message decides which: it was the
allocator's (`memory allocation of N bytes failed`), not the stack handler's (`thread '...' has
overflowed its stack`). This is heap exhaustion against procgov's 19GB job-object committed-memory
cap, not unbounded recursion. Three measurements pin that:

- the three corpus words parse uncapped (`Morpher::new(g, usize::MAX)`) in 0.185s, so no recursion in
  the engine is unbounded on this grammar;
- the plan-composed net builds in 0.027s, so the compiler is not where the memory goes;
- the tuned whole-grammar compiler proposes and confirms all three words in 0.597s.

Only the plan-composed propose dies, and it dies enumerating `apply_up` paths: measured
2,985,984 = 12^6 raw paths for `xxxxxxk` (against 924 real analyses), which implies
12^12 = 8,916,100,448,256 for `xxxxxxxxxxxxk` — the word the process never got past. The recursion is
depth-bounded (by each rule's `multipleApplication`, the DTD default 1, and by the template's
descending slot index); the search's output is what is unbounded in magnitude.

What is now asserted: the aborting shape is a `Certification::ResourceBreach`. A breach is not
selectable, so this cannot certify anything wrongly; and it is a refusal, not a truncation — the
refused word is never confirmed and its partial proposal set never reaches the oracle comparison, so
it cannot manufacture a recall failure the way a truncated proposal set would.

## `the_refused_magnitude_grows_with_the_word_and_not_with_the_grammar`: the over-generation magnitude

12^6 = 2,985,984 raw paths for `xxxxxxk` against C(12,6) = 924 real analyses is a 3,231x
over-generation, and 12^12 for the 12-`x` word is ~8.9 x 10^12 — which is why no larger buffer fixes
this. The test raises the envelope to just above the k=6 figure so the k=6 word completes and the
12-`x` word is the one that trips, proving the growth is in the word length and not a fixed cost.

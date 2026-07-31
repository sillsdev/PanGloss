# Divvun / GiellaLT — what they do, and why we do it differently

Three documents. Start with whichever matches your question.

| If you are asking… | Read |
|---|---|
| **"Why can't we just use Divvun? They're doing the same thing!"** | [why-not-just-use-divvun.md](why-not-just-use-divvun.md) |
| "What does their system actually do, and how do we know?" | [what-divvun-actually-does.md](what-divvun-actually-does.md) |
| "Is there anything we should steal from them?" | [ideas-worth-borrowing.md](ideas-worth-borrowing.md) |

## The one-paragraph version

Divvun/GiellaLT and PanGloss both produce a finite-state transducer that analyzes words in
morphologically complex languages. The difference is what the FST *is*. For Divvun the FST is the
whole analyzer — whatever it accepts is the answer. For PanGloss the FST is a **proposer**, and
HermitCrab's `confirm` step checks every candidate against the real grammar before it becomes an
answer. Their analyzer is hand-written per language by a linguist and, by their own published
maturity criteria and their own test harness, is **never measured for how much it wrongly accepts**.
Ours is compiled from a FLEx grammar and is checked per word. You cannot adopt their architecture
without deleting the check, and the check is the product.

That is a difference in goals, not a defect on either side. They ship working language tools for
languages nobody else serves, and several of their techniques are worth copying — see
[ideas-worth-borrowing.md](ideas-worth-borrowing.md).

## Status

Research conducted **2026-07-30** against fresh `lang-*` and `giella-core` clones; consolidated and
rewritten **2026-07-31**. The clones are gone, so GiellaLT-side citations carry the verification
status they had on 2026-07-30; PanGloss-side citations were re-verified on 2026-07-31. See the
provenance note at the top of [what-divvun-actually-does.md](what-divvun-actually-does.md).

The eighteen raw working reports this consolidates are in git history at commit `e0dd20f` if the
underlying notes are ever needed.

### Where the numbered reports went

Several code comments cite these by number (e.g. `pg-foma/src/emit.rs:11`, "research report 12's
`BoundRoot` finding"). The numbers resolve as follows:

| Reports | Subject | Now in |
|---|---|---|
| `00` | Synthesis and decision | superseded by all three documents |
| `01`, `02`, `04` | Architecture, toolchain, North Sámi morphophonology, scale | [what-divvun-actually-does.md](what-divvun-actually-does.md) §1, §2, §6 |
| `03`, `08`, `15`–`17` | Pruning vs. Constraint Grammar; over-generation and how it is measured | [why-not-just-use-divvun.md](why-not-just-use-divvun.md) §2, §4; [what-divvun-actually-does.md](what-divvun-actually-does.md) §4 |
| `05`, `09` | HC→FST expressibility; the cascade route and the two-level seam | [what-divvun-actually-does.md](what-divvun-actually-does.md) §3.4 |
| `06` | Interop and shipping path | [why-not-just-use-divvun.md](why-not-just-use-divvun.md) §5 |
| `07` | Flag × replace-calculus source proof | [what-divvun-actually-does.md](what-divvun-actually-does.md) §3.5; [ideas-worth-borrowing.md](ideas-worth-borrowing.md) idea 2 |
| `10`–`14` | Filter complexity and construction | [what-divvun-actually-does.md](what-divvun-actually-does.md) §5 |

**Standing decision: we keep the HermitCrab `confirm` step.** Nothing in this directory is a plan to
remove it. The ideas document is about making the proposer *tighter* — which buys speed, since
`confirm` already buys correctness.

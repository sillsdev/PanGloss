# pg-foma p6_gate_parity.rs: MPR/POS subrule-gating acceptance gate

An acceptance gate for MPR/POS subrule-gating (`pg_foma::gate`; see `pg-foma/src/gate.rs`'s module
doc for why it is a static partition, not a flag-diacritics encoding). Both cases compare
`pg_parse::Morpher`'s full-engine oracle against the compiled foma network's `apply_up` decoded
candidates, keyed by `(morpheme_ids, root_index)` — the same positional-multiset predicate
`p6_replace_prototype.rs`'s own parity gate uses.

## Case 1: Indonesian, MPR exclusion (`prule5`, `excludedMPRFeatures="mpr1"`)

The real `indonesian-hc.xml` declares this exclusion (4 lexical entries carry `ruleFeatures="mpr1"`),
but all 4 roots start with a consonant cluster (`pr`, `kl`, `sw`, `tr`), so `prule5`'s own
right-environment (a vowel class) never matches at the cluster's second consonant regardless of the
MPR gate — confirmed by both a natural-class read and by grepping the corpus word list (zero hits).
The real corpus therefore cannot exercise the critical juncture, so this file augments a copy of the
real grammar with two synthetic entries built to a shape the real corpus does independently attest
elsewhere (`tulis`/`pukul`): root `tanam` (no MPR restriction, control) and root `tabur` (carries
`ruleFeatures="mpr1"`, must survive deletion). Expected values are gathered from the real oracle
first, not predicted:

- `menanam` (deleted, control root) analyzes.
- `mentanam` (undeleted — wrong for the control root) does not.
- `menabur` (deleted — wrong for the mpr1-excluded root) does not.
- `mentabur` (undeleted — correct for the excluded root) analyzes.

## Case 2: POS gating (Amharic `prule1`/`prule2`'s exact shape)

Amharic's own grammar uses `<AffixTemplate>` morphotactics this prototype's `uflexc` emitter cannot
emit (a separate, already-costed gap, not attempted here), so an end-to-end Amharic corpus recall
gate is out of reach. Instead, a minimal, hand-authored, template-less grammar reproduces Amharic
`prule1`'s exact rule shape (3 fixed segments -> 1, no environment, `requiredPartsOfSpeech`), with
two lexical entries sharing the identical underlying shape `xyx` and differing only in part of
speech — so the gate is the only thing that can distinguish which entry a given surface form
recovers. Oracle ground truth: `xyx` (undeleted) can only be the noun entry (the verb's rule is
obligatory once applicable, so a verb root can never surface as raw `xyx`); `w` (merged) can only be
the verb entry.

## Regression coverage

- `ungated_cascade_would_have_missed_the_excluded_root` / `ungated_cascade_would_have_missed_the_noun_entry`:
  the ungated cascade (`compile_and_compose_rules`, the pre-existing, unedited entry point) misses
  the exact analysis the real engine accepts, proving the gated path (the case-1/case-2 tests above)
  closes a real recall gap rather than merely happening to match the oracle.
- `indonesian_full_corpus_parity_unregressed`: reruns the full 97/97 Indonesian corpus parity gate
  through the gated compile path; the augmented grammar's 2 synthetic entries neither collide with
  nor are reachable by any real corpus word.
- `amharic_gated_subrules_and_tuple_counts_unregressed`: reconfirms Amharic's own tuple-expansion
  numbers (82 states / 1,110,358 arcs, `p6-prototype-report.md` §5.1) are byte-identical through the
  untouched `compile_and_compose_rules` entry point, and that `pg_foma::gate` correctly finds
  Amharic's 3 real POS-gated subrules (`prule1`/`prule2`/`prule3`) without crashing. `#[ignore]`d by
  default per this repo's test-timing policy (Amharic's cascade compile costs several seconds); run
  via `cargo test -p pg-foma --release -- --ignored amharic_gated`.

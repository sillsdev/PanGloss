# Stall detection and reassessment heuristics

_Research note, 2026-08-23. Sources were checked live on this date._

## Bottom line

The literature does not support a universal “stop after N minutes” rule, nor does it
validate exactly two failed attempts as an optimal cutoff. It does support a more useful
operational distinction: continue while the next action has a stated, discriminating
information payoff; pause, reframe, or escalate when effort is no longer changing the
evidence state, the acceptance metric, or the set of viable hypotheses. Time is a review
trigger, not by itself proof that work is futile.

## What the evidence says

### 1. Repeating one causal hypothesis is a meaningful stall signal

Debugging is commonly modeled as an evidence loop: select a tactic, seek a clue, form a
hypothesis, run an experiment, and refine the hypothesis. A field study of professional
developers reports that novices can remain constrained by their initial tactic or
hypothesis even when it is unhelpful, while experts are more flexible. The authors also
describe scientific debugging as repeated hypothesis formulation, verification, and
refinement ([Siegmund et al., 2014](https://www.hirschfeld.org/writings/media/SiegmundPerscheidTaeumelHirschfeld_2014_StudyingTheAdvancementInDebuggingPracticeOfProfessionalSoftwareDevelopers_IEEE.pdf), pp. 269–270).

In a controlled study of 20 developers, incorrect hypotheses led developers to investigate
irrelevant information and block progress; developers had about two hypotheses per defect,
and having a correct hypothesis early strongly predicted success. Supplying candidate
hypotheses made success six times more likely ([Alaboudi & LaToza, 2020](https://arxiv.org/abs/2005.13652)).
This supports recording the current causal hypothesis and treating repeated tests that do
not update it as a warning. It does **not** establish that two failures is a universal
optimal number.

The practical interpretation is therefore: count an attempt only when it is a
discriminating test (or an acceptance test of a patch). After two independent tests fail
to support the same hypothesis, freeze speculative edits and reframe the hypothesis,
experiment, or requested expertise. If both attempts were inconclusive because of a
timeout, flaky environment, or missing access, classify the state as blocked/inconclusive,
not as two causal failures.

### 2. Long elapsed time warrants a checkpoint, not an automatic stop

An observational study of 11 professional developers (30 hours of sessions, 89 debugging
episodes) found a highly skewed duration distribution: some episodes lasted over 100
minutes, but 80% of all debugging time was in the longest 25% of episodes. Its “long” group
(at least 12.3 minutes) averaged 32 ± 23 minutes, while short episodes averaged about 31
seconds ([Alaboudi & LaToza, 2021](https://arxiv.org/abs/2105.02162), especially §§4.1–4.3).
An experiment with practitioners on 27 real bugs found mean diagnosis time of 32 minutes
and mean patching time of 16 minutes; a cluster of less-difficult diagnoses sat around
15–20 minutes, while the hardest diagnosis took about 55 minutes ([Böhme et al., 2017](https://doi.org/10.1145/3106237.3106255), pp. 120–121). These observations support
task-specific checkpoints, but do not make 15, 55, or 60 minutes a universal cutoff. Long
work is normal enough that a hard 60-minute stop would create false stops, while a long,
unstructured episode still deserves explicit review.

A systematic review of 102 studies of software-engineering time pressure found the common
pattern of increased efficiency but reduced quality; it also reports tunnel vision and
missed improvement opportunities under pressure. Effects vary by task and context
([Kuutila et al., 2020](https://arxiv.org/abs/1901.05771), §§5.4 and 7.2). Experiments on
sequential decisions likewise find that people adapt stopping thresholds to the time
horizon and outcome variance, and do not consistently use the mathematically optimal
threshold ([Baumann et al., 2023](https://doi.org/10.1037/xge0001287); [Khodadadi et al., 2017](https://doi.org/10.1016/j.cogpsych.2017.03.002)).

Thus “more than 60 minutes since the last new falsifiable fact or artifact” is defensible
as a **reassessment checkpoint** for agent work, but is not a literature-derived optimum.
The checkpoint should ask what new evidence the next action is expected to produce. A
shorter or longer local threshold may be appropriate for a fast test, a slow build, or a
high-stakes incident.

### 3. Delegation helps when it adds information, not merely another attempt

The Swarm Debugging studies collected debugging paths from professional developers and
tested sharing them with other developers. Shared sessions and visualized paths helped
participants form candidate fault-location hypotheses, although the results varied by
task ([Petrillo et al., 2019](https://arxiv.org/abs/1902.03520), §§1 and 6). The transferable
lesson is about the artifact being shared: a useful handoff carries observations, paths,
logs, or a narrowed hypothesis, not only a request to “try again.”

Google’s incident-response case study likewise says that failing to declare an incident
early left responders without the tools to respond efficiently, and that early escalation
would have produced a quicker, more organized response ([Google SRE Incident Response](https://sre.google/workbook/incident-response/), Case Study 1). There is no direct empirical study establishing a number of unproductive AI-agent delegations. The proposed signal is therefore a control heuristic: if two handoffs return no new artifact, changed belief, or newly available capability, stop repeating the same delegation; change the question, attach the evidence ledger, or escalate to a specifically named missing capability.

### 4. Growing scope or diff without acceptance progress is evidence of batching failure

Google’s engineering guidance recommends a self-contained, minimal change that addresses one
thing; it notes that small changes are easier to reason about, less likely to introduce bugs,
and waste less work if the direction is rejected ([Google Engineering Practices, “Small CLs”](https://github.com/google/eng-practices/blob/master/review/developer/small-cls.md)). DORA’s research-backed capability similarly says that small batches make hypothesis testing and course correction faster, reduce feedback time, and help avoid sunk-cost behavior ([DORA, “Working in small batches”](https://dora.dev/capabilities/working-in-small-batches/)).

This supports a scope/diff heuristic: when the changed surface grows but the acceptance
metric has not moved and no new falsifiable artifact is being produced, split to the
smallest testable slice, revert speculative breadth, or reframe the goal. The sources are
guidance and correlational/organizational research, not a validated line-count threshold;
the signal is progress relative to the acceptance metric, not raw lines changed.

### 5. Resource waiting and conceptual churn should be handled differently

NIST’s current incident-response guidance emphasizes recording actions and discovered facts,
preserving provenance, and collecting incident data as evidence ([NIST SP 800-61 Rev. 3](https://csrc.nist.gov/pubs/sp/800/61/r3/final)). Google SRE’s guidance emphasizes rapid escalation and involving more people when the responder cannot see a solution ([Google SRE, “Emergency Response”](https://sre.google/sre-book/emergency-response/)). Together these support an evidence ledger with a dependency owner and an escalation path.

Treat work as **resource-blocked** when the next evidence-producing action cannot occur
without an external dependency (machine, permission, reviewer, upstream answer, or build
slot), and record the owner, request, and next check time. Treat it as **conceptual churn**
when internal activity continues but the hypothesis, acceptance metric, or requested
artifact remains unchanged. The former calls for waiting in parallel with bounded
escalation; the latter calls for reframe or stop. This classification is an operational
inference, not a measured diagnostic instrument.

## Recommended operational rule for `agent-handoff`

At the start of each attempt or handoff, record four fields:

1. **Acceptance metric:** the test or observable condition that would count as progress.
2. **Current hypothesis/question:** what is believed and what the next action could falsify.
3. **Expected artifact:** the log, reproduction, test result, diff, or decision expected back.
4. **Dependency and checkpoint:** owner/ETA if blocked, plus the next reassessment time.

After each action, classify the result as **confirmed**, **falsified**, **inconclusive**, or
**resource-blocked**. Trigger reassessment immediately when any of the following holds:

- two independent, discriminating attempts have failed against the same causal hypothesis;
- 60 minutes have elapsed since the last new falsifiable fact or artifact;
- two delegations have returned no evidence change or newly available capability; or
- scope/diff has grown while the acceptance metric remains unchanged.

Reassessment means writing the current evidence and eliminated hypotheses, then changing at
least one of the hypothesis, experiment, acceptance slice, or required capability. If the
state is a named external wait with an owner and ETA, mark it blocked and escalate on the
checkpoint; do not manufacture conceptual churn while waiting. If it is not externally
blocked, stop repeating the same action and reframe or escalate with the evidence ledger.

The 60-minute value is a deliberately tunable default for this workflow, not a claim about
human optimal stopping. Local telemetry should measure how often checkpoints produce a new
fact, a corrected hypothesis, or a successful escalation, and adjust the threshold by task
class. High-stakes work may justify persistence, but it should still expose its expected
evidence payoff and dependency state.

## PanGloss case lesson: classify the axis before changing the architecture

The five-grammar FST effort exposed a concrete conceptual-churn pattern. Error-level production
readiness findings on deliberately stressful grammars were treated as reasons not to attempt the
grammars. Work then expanded into envelope thresholds, override architecture, and backend refusal
policy even though the immediate acceptance question was whether a complete construction could run
inside external containment. Real completeness defects were present, but the axis confusion caused
unnecessary work around them.

For PanGloss FST work, every handoff and reassessment must therefore classify the evidence as one of:

1. **correctness/representability** — may a valid HermitCrab analysis be omitted?;
2. **production readiness** — is a complete result suitable to ship?; or
3. **resource containment** — did this attempt remain inside its safety boundary?

A production-readiness Error is still eligible for a contained developer stress attempt. A
correctness Critical is not made accurate by more resources. A containment stop never licenses
partial output. If a proposed change cannot name which one of these axes it advances, that is an
immediate reassessment trigger rather than another implementation attempt.

## Limitations

- Debugging studies are small, observational, or controlled experiments; they do not model
  multi-agent software work directly.
- Time-pressure results are heterogeneous and include quality trade-offs; they do not justify
  a universal minute count.
- Google, DORA, and NIST guidance is strong first-party practice, but it is not causal proof
  that every team will see the same effect.
- The exact cutoffs of two attempts, two unchanged delegations, and 60 minutes are proposed
  operating defaults. They should be treated as falsifiable process hypotheses and tuned from
  the repository’s own acceptance outcomes.

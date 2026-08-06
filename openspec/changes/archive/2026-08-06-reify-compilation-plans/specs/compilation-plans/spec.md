## ADDED Requirements

### Requirement: A compilation plan is reified as a content-addressed DAG

The compile step SHALL represent the composition topology for a grammar as a first-class,
enumerable `Plan`: a directed acyclic graph of nodes drawn from a closed kind set (`Leaf`,
`Compose` with an n-ary child list and a physical strategy, `Union`, `Gate`, `Replace`). Node
identity SHALL be content-addressed (a function of node kind, child identities, and configuration)
so that identical sub-plans share one identity, storage, and measurement across plans.

#### Scenario: Two plans share a lexicon leaf

- **WHEN** the enumerator emits two candidate plans that differ only in how the phonological
  cascade is grouped but reference the same compiled lexicon fragment
- **THEN** that lexicon `Leaf` has one content-address shared by both plans and is compiled and
  measured once, not once per plan

#### Scenario: Adding a node kind is a closed-set change

- **WHEN** a new composition node kind is introduced
- **THEN** it is added to the closed kind enum and every exhaustive match over node kinds fails to
  compile until it is handled (no catch-all arm)

### Requirement: Hardcoded topology branching is replaced by enumerated plans

The imperative topology decisions `preexpand::should_run`, `emit::probe_would_refuse`, and
`gate::partition_entries` SHALL be expressed as choices the strategy enumerator makes when emitting
candidate plans, not as inline boolean branches. The pre-refactor behavior SHALL be preserved as a
specific enumerable plan (e.g. an ungated grammar collapses to a single-group `Gate`).

#### Scenario: Ungated grammar collapses to one group

- **WHEN** a grammar has no gated subrule reachable through the emitter
- **THEN** the `Gate` node partitions its entries into exactly one group, reproducing the
  pre-refactor single-network compile byte-for-byte

#### Scenario: Refuse-probe becomes two candidate plans

- **WHEN** a grammar contains a construct that today sets `probe_would_refuse`
- **THEN** the enumerator can emit both the structural-composite plan and the ordinary
  concatenative plan as capability-passing candidates, and the differential oracle diffs them

### Requirement: Selection is capability-safe by construction

The enumerator SHALL emit only plans all of whose nodes pass the capability characteristics-check
envelope, and every capability-passing plan SHALL be recall-preserving (produce the identical
confirmed set). Selection among passing plans SHALL use a deterministic objective (measured or
estimated states+arcs / payload size, tie-broken by content-address) and SHALL never be able to
select a plan that changes the confirmed set.

#### Scenario: Selection changes cost, never correctness

- **WHEN** two capability-passing plans exist for a grammar and one is selected by the cost
  objective
- **THEN** the selected plan's confirmed set for every word equals the other plan's confirmed set

### Requirement: Differential-correctness oracle over multiple plans

When two or more capability-passing plans exist for a grammar, the build SHALL, in its always-on
tier, run the committed conformance corpus through each plan's proposer and assert that the confirmed
analysis sets are equal per word. On disagreement it SHALL report the shortest disagreeing word and
the symmetric difference of proposed analyses and SHALL treat the disagreement as a capability-
predicate defect (not a tolerable divergence).

#### Scenario: Plans disagree on a word

- **WHEN** two capability-passing plans produce different confirmed sets for some corpus word
- **THEN** the build fails with the shortest such word and the set difference, attributing it to the
  predicate that admitted a non-recall-equivalent plan

### Requirement: Propose contains only language-preserving operations

Every operation in a proposer pipeline SHALL be language-preserving (trimming, epsilon-removal,
determinization where valid, minimization). Operations that can change the recognized relation
(weight-based beam pruning, top-k or best-path shortcuts) SHALL NOT appear in propose and are
confined to the confirm/ranking layer.

#### Scenario: A relation-changing optimization is rejected in propose

- **WHEN** a proposer construction applies an operation that could drop a valid path (e.g. best-path
  pruning)
- **THEN** it is disallowed in the propose pipeline and must move to confirm

### Requirement: Plan nodes are individually addressable for coverage

Each plan node SHALL be addressable by its content-address and declared kind so that conformance and
fuzz fixtures can be tagged with the plan-node-kind interactions they exercise, and t-wise coverage
over capability-legal node-kind tuples can be reported.

#### Scenario: A node-kind interaction has no covering fixture

- **WHEN** coverage is computed over capability-legal node-kind pairs
- **THEN** any pair exercised by no fixture is reported as a coverage gap

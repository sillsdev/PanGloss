## ADDED Requirements

### Requirement: A compilation plan serializes to versioned JSON
A `Plan` SHALL serialize to a documented, versioned JSON shape that round-trips, and whose node
identities are the plan's own content addresses.

#### Scenario: The same grammar produces the same document
- **WHEN** an unchanged grammar is planned twice
- **THEN** the serialized JSON is identical, including node identities

#### Scenario: A semantic change moves exactly the affected identities
- **WHEN** one rule's content changes
- **THEN** that node's identity and its ancestors' identities change, and unrelated sibling subtrees'
  identities do not

### Requirement: The plan renders as a mermaid graph labelled by linguistic role
Plan JSON SHALL render as a mermaid graph in which every node carries the linguistic work it performs
(the stratum, template, rule class, or construct it accounts for), with the plan node kind as
secondary detail.

#### Scenario: An author reads the decomposition
- **WHEN** a grammar with more than one stratum and at least one templated layer is rendered
- **THEN** the diagram distinguishes the strata and shows which layer each templated group belongs to

### Requirement: Large plans render readably or say what they omitted
Rendering a plan whose node count exceeds the readability threshold SHALL summarize rather than emit
an unreadable graph, and SHALL state in its output that summarization occurred, the threshold, and the
emitted node count.

#### Scenario: A realistic lexicon is rendered
- **WHEN** a plan's leaf count exceeds the threshold
- **THEN** sibling leaf groups render as labelled summary nodes carrying counts, and the output
  records that it summarized

#### Scenario: Full detail is requested explicitly
- **WHEN** full rendering is requested
- **THEN** no summarization is applied and the emitted node count is reported

### Requirement: A refused construct is visible in the diagram
Where a capability verdict applies to a node, the rendering SHALL show that verdict as determined by
the real capability evaluation, never inferred from the node's presence in the plan.

#### Scenario: A grammar containing a refused construct is rendered
- **WHEN** a grammar carrying a permanently carved-out construct is rendered
- **THEN** the responsible node is marked refused, and the diagram does not present it as handled

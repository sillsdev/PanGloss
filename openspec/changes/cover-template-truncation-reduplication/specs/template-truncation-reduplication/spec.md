## ADDED Requirements

### Requirement: Template and truncation coverage is variant-specific
Template depth, order, alternatives, final/partial behavior, template-less paths, and bounded
truncation chains SHALL receive individual dispositions and oracle-backed witnesses.

#### Scenario: Two template alternatives share a category
- **WHEN** either alternative is legal for the same root
- **THEN** the proposer recalls both oracle analyses without combining incompatible prefix/suffix slots

### Requirement: Peeled reduplication is proven end to end
Reduplication handled outside the FST SHALL be labeled `peeled` and SHALL prove bounded
peeler-to-HermitCrab recall, shape, identity, and multiplicity.

#### Scenario: A word requires reduplication
- **WHEN** the FST alone cannot propose its oracle analysis
- **THEN** the bounded peeler supplies it for confirmation without claiming compiled FST support

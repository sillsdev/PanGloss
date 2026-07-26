## ADDED Requirements

### Requirement: A cheap cascade attributes a word to a loaded language before FST work runs
When more than one language's `.pgpack` is loaded and active for a session, PanGloss SHALL narrow the
candidate loaded languages for each word, before running FST propose→confirm against more than one
loaded grammar, using only **hard** signals: host-supplied writing-system metadata (authoritative
external input) and a per-writing-system script/character-set feasibility gate (a writing system that
cannot contain the word's characters cannot have produced it). PanGloss SHALL NOT run full
propose→confirm against every loaded grammar for every word when a hard signal already narrows the
candidate set to one.

The session/document language prior SHALL NOT eliminate a candidate language; it ranks results only
(see the ambiguity requirement below). Narrowing is an optimization for speed and candidate quality,
never a correctness step.

#### Scenario: The session prior favors one language but another also accepts
- **WHEN** the session/document prior favors language A, and the word also parses (non-`guessed`)
  in loaded language B, which no hard signal excludes
- **THEN** PanGloss keeps both A and B as candidates and returns both, ranked with A first — it does
  not drop B on the strength of the prior alone

#### Scenario: Host declares the current writing system
- **WHEN** the host (e.g. FLEx, Paratext, Keyman) supplies the writing system or input language for
  the current field/context
- **THEN** PanGloss uses that language directly as the candidate and does not run the script gate or
  session prior for that word

#### Scenario: No host metadata, orthographically distinct loaded languages
- **WHEN** no host writing-system metadata is supplied and the loaded languages use different,
  non-overlapping word-forming character sets
- **THEN** the script/character-set gate alone narrows the candidate set to the one language whose
  writing system can contain the word's characters

### Requirement: A word parsing in exactly one loaded language is identified for free
A word that parses (non-`guessed`) in exactly one candidate loaded language, after the cascade in the
previous requirement, SHALL be treated as identified without further disambiguation work.

#### Scenario: Single accepting grammar
- **WHEN** exactly one loaded grammar's analyzer accepts a word with `guessed = false`
- **THEN** PanGloss attributes the word to that language and does not consult the other loaded
  grammars for identification purposes

### Requirement: A word parsing in two or more loaded languages is tagged with all of them and ranked, never reduced to one
When a word parses (non-`guessed`) in more than one candidate loaded language, PanGloss SHALL tag the
word with **every** accepting language and SHALL rank them — session/document prior first, then each
language's own class-n-gram score for the candidate under a stated normalization. PanGloss SHALL NOT
reduce a multi-language result to a single language on the strength of a soft signal.

#### Scenario: Session prior orders, but does not eliminate
- **WHEN** a word parses validly in two loaded sibling languages and the session's recent language
  history favors one of them
- **THEN** PanGloss returns the word tagged with both, with the session-favored language ranked first

#### Scenario: No signal distinguishes the candidates
- **WHEN** a word parses validly in two loaded languages, the session prior does not favor either,
  and their normalized class-n-gram scores are equal
- **THEN** PanGloss returns the word tagged with both candidate languages, in a stable documented
  order, and does not pick one

#### Scenario: The candidate-language budget cannot cover every accepting language
- **WHEN** the measured per-grammar resource/latency budget cannot cover propose→confirm plus tier-1
  expansion for every accepting language
- **THEN** PanGloss drops the lowest-ranked candidate languages as an explicitly **reported** degraded
  mode, and SHALL NOT drop a candidate language silently

### Requirement: A word parsing in no loaded language's normal branch falls through to guess branches in prior order, and total failure is a distinct outcome
A word that fails to parse (non-`guessed`) in any narrowed candidate language SHALL be attempted
against each candidate's guess branch (`WordAnalysis.guessed = true`) in session-prior order. If no
candidate language's guess branch produces an acceptable analysis, PanGloss SHALL report a distinct
"no loaded language could account for this word" outcome rather than silently returning an ordinary
no-correction-found result or forcing a guess under a language the evidence does not support.

#### Scenario: Word is genuinely unknown to one loaded language but known via guessing
- **WHEN** a word fails the normal lexicon in the only remaining candidate language but its guess
  branch produces an acceptable analysis
- **THEN** PanGloss returns that guessed analysis tagged `guessed = true` for that language

#### Scenario: Word matches no loaded language at all
- **WHEN** a word fails both the normal and guess branches of every candidate loaded language
- **THEN** PanGloss returns the distinct not-accounted-for outcome, not a generic failure indistinct
  from "no correction found for a recognized-language word"

### Requirement: The two-scale class n-gram (D4) stays per-language with no shared vocabulary or union model
Each loaded language SHALL have its own complete two-scale class n-gram (inter-word class trigram
and intra-word morpheme n-gram per `PLAN.md` D4), trained only on that language's own data. PanGloss
SHALL NOT construct a shared/union vocabulary or a single cross-language class-n-gram model.

#### Scenario: Two loaded languages have incompatible POS inventories
- **WHEN** two loaded grammars define different POS categories and feature inventories
- **THEN** each grammar's class-n-gram is estimated and queried independently, with no attempt to
  map one grammar's classes onto the other's

### Requirement: A code-switch context boundary degrades to the coarsest backoff rung
When scoring a candidate word in language L whose immediately preceding word was identified as a
different language, PanGloss SHALL treat that boundary as a context break for L's class-n-gram and
back off to the coarsest defined backoff rung (or no context) rather than constructing or querying a
joint cross-language class distribution.

#### Scenario: Candidate follows a code-switched word
- **WHEN** the word immediately before the current candidate was attributed to a different loaded
  language than the candidate's own
- **THEN** the candidate's inter-word class-n-gram term is computed using the coarsest backoff rung,
  and this degraded scoring is reported as expected behavior at a code-switch boundary, not an error

### Requirement: Cross-language score comparison uses a stated, explicitly provisional normalization
Wherever PanGloss compares a score computed under one loaded language's class-n-gram against a score
computed under another loaded language's class-n-gram (for ordering a multi-language-tagged word or
for cross-language next-word ranking), it SHALL apply a documented normalization step and SHALL treat
that normalization as an unvalidated design bet requiring measurement, not a settled result. This
comparison SHALL affect ranking only; it SHALL NOT eliminate an accepting language, so an incorrect
normalization degrades result ordering and never discards a correct analysis.

#### Scenario: Comparing tied candidates across languages
- **WHEN** two loaded languages' class-n-grams both accept a candidate and their raw log-probability
  scores must be compared to break a tie
- **THEN** PanGloss applies the documented per-language normalization before comparing, and any
  report or diagnostic surfacing that comparison labels it as provisional

### Requirement: Next-word prediction merges independent per-language proposals, biased toward language continuity
For word prediction with multiple languages loaded, PanGloss SHALL generate candidate continuations
independently from each loaded language's own class-n-gram, merge the resulting candidate lists, and
rank them using the cross-language normalization above, weighted toward continuing in the same
language as the immediately preceding word.

#### Scenario: Predicting after a monolingual run of text
- **WHEN** the preceding several words were all attributed to one loaded language
- **THEN** that language's proposed continuations are weighted above other loaded languages' proposed
  continuations for the same position, absent stronger contrary evidence

### Requirement: Per-language spell-check data is an additive, self-contained `.pgpack` section
The class-n-gram tables, phonological substitution-cost table, and per-writing-system
orthographic-unit data for one language SHALL be carried as additive data sections inside that
language's own `.pgpack`, remaining data-only per the existing package contract. PanGloss SHALL NOT
introduce a shared or union data blob spanning more than one language's package.

#### Scenario: Two Language Packs are loaded together
- **WHEN** a host loads Language Pack A and Language Pack B in the same process
- **THEN** each pack's spell-check data remains fully contained in its own package and neither
  package's data section references or depends on the other's contents

### Requirement: Session state, not `.pgpack` content, carries the active language set and seen-word cache
PanGloss SHALL represent which loaded packs are in scope for a given session/document, in
host-supplied priority order, as session-level state distinct from any single `.pgpack`. PanGloss
SHALL represent the seen-word cache (words typed by this user or present in this document) as one
flat, per-session/document set whose entries carry the language (or languages, when ambiguous) they
were attributed to, rather than as separate per-language caches split at write time.

#### Scenario: A document contains code-switched text
- **WHEN** a single document contains words from two loaded languages
- **THEN** the seen-word cache for that document contains both languages' words together, each
  tagged with its attributed language, in one set

#### Scenario: An ambiguous word is cached
- **WHEN** a word was tagged with two candidate languages under the ambiguity requirement above
- **THEN** its seen-word cache entry carries both language tags rather than being forced into one
  language's cache

### Requirement: Personal overlays remain per-(user, language) using the existing overlay mechanism
Personal wordlist, confusion/error-model, and cache-LM overlays SHALL be created and maintained
per loaded language per user, reusing the existing `SuppliedRootOverlay`/`RootAuthority` override
mechanism and the revisioned, immutable `LexiconSnapshot` pattern. PanGloss SHALL NOT introduce a
cross-language-shared personal overlay.

#### Scenario: A user has personal overlays for two loaded languages
- **WHEN** a user has typed enough in both Language A and Language B to have accumulated personal
  wordlist entries in each
- **THEN** each language's overlay is a distinct, independently revisioned `SuppliedRootOverlay`/
  `LexiconSnapshot` instance, and no entry or confusion-weight in one is visible to the other

### Requirement: Language load/unload is an explicit host-driven operation with no PanGloss-owned eviction policy
Adding a language at runtime SHALL be loading its `.pgpack` as a new isolated handle; removing one
SHALL be unloading that handle; neither SHALL require recompiling any grammar. PanGloss SHALL NOT
implement an automatic eviction policy (e.g. LRU-based unloading) for resident language packs in this
design.

#### Scenario: A host adds a language mid-session
- **WHEN** a host loads a new `.pgpack` for a language not previously resident
- **THEN** that language becomes an additional isolated handle usable in the active language set
  without affecting already-loaded languages' state

#### Scenario: Memory pressure is not PanGloss's decision to resolve alone
- **WHEN** many languages are resident and memory grows roughly linearly with the number loaded
- **THEN** PanGloss reports the resource state rather than silently unloading a language the host did
  not explicitly unload

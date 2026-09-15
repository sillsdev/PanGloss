# Gates on the documents agents read

CLAUDE.md and the skills under `.claude/skills/` are read by agents as instructions, and nothing
checked them. They rotted in exactly the ways prose rots: paths that no longer resolve, recipes that
name a branch `.gitmodules` does not, bugs narrated in the present tense whose field had already been
deleted, and — in one skill — five instructions to run bare Cargo that the repo's own `PreToolUse`
hook refuses. The skills are about four times the size of CLAUDE.md, so they are in scope too.

Three test gates in `pg-cli` hold the properties that prose cannot hold for itself.

## `agent_docs_resolve_gate` — every path an agent doc names must exist

Scans CLAUDE.md and every `.claude/skills/*/SKILL.md` for backticked tokens and asserts that each one
that is a repo path resolves on disk. A reader told to open a dead path finds nothing, and a recipe
built from one fails.

**Deciding what is a path claim is the whole difficulty.** A backticked token counts only when it
contains a separator, has no space, is longer than three characters, does not start with `http` or
`-`, and contains no `::` (which makes it a Rust item path, not a filesystem one).

That is still not enough. The first version flagged twenty false positives against four real
findings — `sillsdev/machine` (a GitHub slug), `perf/pr494-priority-union` (a git branch),
`languages/bantu-verbal` (relative to the conformance submodule), `recipes/schema.json` (relative to
a skill's own directory) — and twenty-to-four is the ratio at which a gate gets muted rather than
fixed. So only a token rooted at a known top-level directory is checked: `rust/`, `docs/`,
`.claude/`, `conformance-staging/`, `machine/`, `samples/`, `openspec/`. Tokens containing `:`, `*`
or `<` are skipped as absolute Windows paths, globs, and placeholders respectively.

Bare symbol names are deliberately out of scope: resolving those needs a compiler, and a regex that
guessed would fire on prose and then get muted — the same failure the root allowlist prevents.

A consequence worth knowing when writing these docs: a path that lives in **another** repository
must not be backticked with one of those roots. The C# review of PR #494 lives at
`docs/pr494-review.md` on the machine repo's `perf/pr494-priority-union` branch, and writing it that
way in a PanGloss skill makes the gate — correctly — call it dead. Name the file alone, and say
which repo and branch it is on in prose.

`the_gate_can_actually_see_a_dead_path` pins the classifier against all four false-positive shapes
and against two true positives, so a future narrowing cannot quietly make the gate vacuous.

## `divergence_catalogue_gate` — the catalogue's own conventions

`docs/divergences/` broke inside the session that created it: three files numbered `001`, two
numbered `002`, and the extras were evidence dumps rather than catalogue entries. A hand-maintained
index cannot notice that. The gate holds three properties:

- **Ids are unique.** Two files sharing an id means `README.md`'s status table can describe only one
  of them and the other is invisible. Supporting material moved under `evidence/`, which carries no
  id by convention and is therefore not an entry.
- **The index lists every entry and invents none.** A reader scanning the table must not miss a file
  that exists.
- **The gate can see entries at all.** It requires at least twenty. Change the naming convention and
  the two checks above would pass over an empty set; this one fails instead.

## `skills_never_instruct_bare_cargo` — no skill may contradict the hook

The `block-bare-cargo.py` `PreToolUse` hook refuses `cargo build|test|check|run` and
`cargo nextest run`. A skill that instructs an agent to run one of those is telling it to do
something the repo will refuse, which costs a round trip and teaches the agent that the docs and the
tooling disagree.

The gate reads the refused subcommand list **out of the hook itself** rather than restating it, so
it cannot claim a prohibition the hook does not enforce, or miss one it adds later.

## The dead pin this class of gate would have caught

`rust/crates/pg-parse/tests/discontinuous_env_gate.rs` is the worked example of the failure. It names
a fixture at `rust/conformance/allomorphy/discontinuous-env/` — a path that has never existed in this
tree — and both its tests skip twice over, on `#[ignore]` and again on a `have_fixture()` guard that
survives `--include-ignored`. CLAUDE.md cites that very fixture as the flagship demonstration of
oracle discipline, so the demonstration protected nothing and nothing noticed.

Reviving it means authoring `conformance-staging/edge-cases/discontinuous-env/` (a grammar with a
discontinuous morph whose allomorph environment holds at its first piece and is violated at a later
one), generating its expectations from the C# founding oracle, rewriting the test body to use
`pg_conformance_fixtures` discovery instead of the hand-rolled path and `expected.tsv` reader,
dropping `have_fixture()` so an absent fixture **fails** rather than skips, and deleting both
`#[ignore]` attributes. That is open work.

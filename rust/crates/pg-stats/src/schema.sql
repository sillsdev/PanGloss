-- Statistics cache schema. Documented as a public escape hatch: callers may query this database
-- directly, so `schema_version` in `cache_identity` and `run` is a compatibility promise.

-- One durable identity survives an empty cache and makes stale open handles harmless. The
-- nullable engine is claimed by the first successful flush.
CREATE TABLE IF NOT EXISTS cache_identity (
  cache_id       INTEGER PRIMARY KEY CHECK (cache_id = 1),
  schema_version INTEGER NOT NULL,
  grammar_hash   TEXT    NOT NULL,
  engine         TEXT
);

CREATE TABLE IF NOT EXISTS run (
  run_id              INTEGER PRIMARY KEY,
  schema_version      INTEGER NOT NULL,
  counter_semantics   INTEGER NOT NULL,
  build_info          TEXT    NOT NULL,
  fwdata_path         TEXT    NOT NULL,
  grammar_hash        TEXT    NOT NULL,
  engine              TEXT    NOT NULL,
  options_hash        TEXT    NOT NULL,
  options_json        TEXT    NOT NULL,
  created_utc         TEXT    NOT NULL,
  word_count          INTEGER NOT NULL,
  total_elapsed_ns    INTEGER NOT NULL,
  -- `--work-cap`'s storage form: NULL when this run recorded no cap (an engine that has no such
  -- concept, e.g. `foma`); -1 is the `usize::MAX` ("unbounded") sentinel, since that value does
  -- not fit i64's positive range. Only non-NULL values are ever compared for a compatibility check.
  work_cap            INTEGER
);

CREATE TABLE IF NOT EXISTS word (
  word_id       INTEGER PRIMARY KEY,
  run_id        INTEGER NOT NULL,
  form          TEXT    NOT NULL,
  elapsed_ns    INTEGER NOT NULL,
  -- `work` is the word's total work steps (`work_steps`), the deterministic parse cost. `fact.work` is a different unit
  -- (shape segments examined per object), so a word's `fact.work` values do not sum to `word.work`; its `fact.work_steps` values do.
  work          INTEGER NOT NULL,
  passes        INTEGER NOT NULL,
  capped        INTEGER NOT NULL,
  timed_out     INTEGER NOT NULL,
  gates_measured INTEGER NOT NULL DEFAULT 0,
  invalid_shape INTEGER NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS word_form ON word(form);

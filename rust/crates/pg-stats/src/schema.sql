-- Statistics cache schema. Documented as a public escape hatch: callers may query this database
-- directly, so `schema_version` in `run` is a compatibility promise, not a note.

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
  total_elapsed_ns    INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS object (
  object_id        INTEGER PRIMARY KEY,
  key              TEXT NOT NULL,
  kind             TEXT NOT NULL,
  label            TEXT NOT NULL,
  identity_quality TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS object_key_kind ON object(key, kind);

-- The unique indexes make interning race-safe across processes, not merely within one
-- transaction: two concurrent runs interning the same key must converge on one row, or their fact
-- rows split across duplicate ids and every SUM double-counts. The sentinel rows carry a NULL key,
-- which SQLite exempts from uniqueness.
CREATE TABLE IF NOT EXISTS stratum (
  stratum_id INTEGER PRIMARY KEY,
  key        TEXT,
  label      TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS stratum_key ON stratum(key);

CREATE TABLE IF NOT EXISTS allomorph (
  allomorph_id INTEGER PRIMARY KEY,
  key          TEXT,
  label        TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS allomorph_key ON allomorph(key);

CREATE TABLE IF NOT EXISTS word (
  word_id       INTEGER PRIMARY KEY,
  run_id        INTEGER NOT NULL,
  form          TEXT    NOT NULL,
  elapsed_ns    INTEGER NOT NULL,
  attempts      INTEGER NOT NULL,
  passes        INTEGER NOT NULL,
  capped        INTEGER NOT NULL,
  timed_out     INTEGER NOT NULL,
  invalid_shape INTEGER NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS word_form ON word(form);

-- `direction` is 'analysis' | 'synthesis', never a foreign key: unlike stratum/allomorph it has no
-- structural locator or label to intern, just a fixed two-value tag, so it is stored inline.
CREATE TABLE IF NOT EXISTS fact (
  word_id          INTEGER NOT NULL,
  object_id        INTEGER NOT NULL,
  stratum_id       INTEGER NOT NULL,
  allomorph_id     INTEGER NOT NULL,
  direction        TEXT    NOT NULL,
  attempts         INTEGER NOT NULL,
  work             INTEGER NOT NULL,
  outputs          INTEGER NOT NULL,
  not_applied      INTEGER NOT NULL,
  no_root          INTEGER NOT NULL,
  surface_mismatch INTEGER NOT NULL,
  uses             INTEGER NOT NULL,
  self_time_ns     INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (word_id, object_id, stratum_id, allomorph_id, direction)
) WITHOUT ROWID;

-- Keyed by run because coverage is a property of the pipeline that ran: an accumulating cache can
-- hold both an hc run and a foma run, and the counters one of them cannot measure are measured
-- normally by the other. A cache-wide coverage row would let the later run's state mask the
-- earlier one's, rendering a real zero as unmeasured or the reverse.
CREATE TABLE IF NOT EXISTS coverage (
  run_id  INTEGER NOT NULL,
  kind    TEXT NOT NULL,
  counter TEXT NOT NULL,
  state   TEXT NOT NULL,
  PRIMARY KEY (run_id, kind, counter)
) WITHOUT ROWID;

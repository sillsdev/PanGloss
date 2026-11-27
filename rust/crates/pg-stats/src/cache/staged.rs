//! Ordered, atomic batch writes for fresh and populated stats caches.
use super::{insert_run, validate_identity, Record, StatsCache, StatsRunBuffer, StatsRunWriter};
use crate::{
    FactRecord, GateRecord, RunMetadata, StatsError, StatsProfile, StructuralLocator, WordRecord,
};
use rusqlite::{Connection, TransactionBehavior};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write as _};
use std::mem::size_of;
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{Builder, JoinHandle};
use std::time::Duration;

mod stage_file;
use stage_file::StageFile;
pub use stage_file::{clean_up_stage, STAGE_CLEANUP_ARG};

const QUEUE_WORDS: usize = 500;
const QUEUE_RAW_BYTES: usize = 16 * 1024 * 1024;
const SPILL_BYTES: u64 = 64 * 1024 * 1024;
pub(super) const BUSY_TIMEOUT: Duration = Duration::from_millis(250);

type WorkerResult = Result<Option<(StatsCache, i64)>, StatsError>;
type CheckRun = dyn Fn(&Connection) -> Result<(), StatsError> + Send;

enum Write {
    Word(WordRecord, usize),
    Buffered {
        file: Arc<Mutex<File>>,
        offset: u64,
        len: usize,
    },
    Drain(SyncSender<()>),
    Finish,
}

fn word_payload_bytes(word: &WordRecord) -> usize {
    let locator_bytes =
        |locator: &StructuralLocator| locator.key.capacity() + locator.label.capacity();
    let facts = word.facts.capacity() * size_of::<FactRecord>()
        + word
            .facts
            .iter()
            .map(|fact| {
                fact.object_key.capacity()
                    + fact.object_label.capacity()
                    + [&fact.stratum, &fact.allomorph, &fact.morpheme]
                        .into_iter()
                        .flatten()
                        .map(locator_bytes)
                        .sum::<usize>()
            })
            .sum::<usize>();
    let gates = word.gates.as_ref().map_or(0, |gates| {
        gates.capacity() * size_of::<GateRecord>()
            + gates
                .iter()
                .map(|gate| {
                    gate.object_key.capacity()
                        + gate.object_label.capacity()
                        + gate.morpheme.as_ref().map_or(0, locator_bytes)
                        + gate.feature_key.capacity()
                        + gate.feature_kind.capacity()
                        + gate.feature_label.capacity()
                        + gate.mode.capacity()
                })
                .sum::<usize>()
    });
    word.form.capacity() + facts + gates
}

struct Worker {
    send: Option<SyncSender<Write>>,
    thread: Option<JoinHandle<WorkerResult>>,
    cancelled: Arc<AtomicBool>,
    queued_bytes: Arc<AtomicUsize>,
    raw_budget: usize,
    spill_cap: u64,
    spill: Option<(Arc<Mutex<File>>, u64)>,
    directory: std::path::PathBuf,
}

impl Worker {
    fn start(
        run: RunMetadata,
        chunk: Option<NonZeroUsize>,
        directory: &Path,
        start_run: impl FnOnce(&RunMetadata, &AtomicBool) -> Result<StatsRunWriter, StatsError>
            + Send
            + 'static,
    ) -> Result<Self, StatsError> {
        let (send, receive) = sync_channel(QUEUE_WORDS);
        let cancelled = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&cancelled);
        let queued_bytes = Arc::new(AtomicUsize::new(0));
        let outstanding = Arc::clone(&queued_bytes);
        let thread = Builder::new().name("stats-sqlite".into()).spawn(move || {
            let mut messages = receive.into_iter();
            let Some(first) = messages.next() else {
                return Ok(None);
            };
            if stopped.load(Ordering::Relaxed) {
                return Ok(None);
            }
            let mut writer = start_run(&run, &stopped)?;
            for message in std::iter::once(first).chain(messages) {
                if stopped.load(Ordering::Relaxed) {
                    return Ok(None);
                }
                match message {
                    Write::Word(word, bytes) => {
                        outstanding.fetch_sub(bytes, Ordering::Relaxed);
                        writer.write_chunked(&word, &run, chunk)?;
                    }
                    Write::Buffered { file, offset, len } => {
                        let mut bytes = vec![0; len];
                        {
                            let mut file = file.lock().expect("stats spill mutex");
                            file.seek(SeekFrom::Start(offset))?;
                            file.read_exact(&mut bytes)?;
                        }
                        let word = WordRecord::decode(&mut bytes.as_slice())?;
                        writer.write_chunked(&word, &run, chunk)?;
                    }
                    Write::Drain(done) => {
                        let _ = done.send(());
                    }
                    Write::Finish => return writer.finish().map(Some),
                }
            }
            Ok(None)
        })?;
        Ok(Self {
            send: Some(send),
            thread: Some(thread),
            cancelled,
            queued_bytes,
            raw_budget: QUEUE_RAW_BYTES,
            spill_cap: SPILL_BYTES,
            spill: None,
            directory: directory.to_owned(),
        })
    }

    fn join(&mut self) -> WorkerResult {
        drop(self.send.take());
        self.thread
            .take()
            .ok_or(StatsError::RunWriteFailed)?
            .join()
            .map_err(|_| std::io::Error::other("stats writer panicked"))?
    }

    fn record(
        &mut self,
        word: WordRecord,
        fallback: Option<&mut StatsRunBuffer>,
    ) -> Result<(), StatsError> {
        let bytes = word_payload_bytes(&word);
        let message = if bytes
            <= self
                .raw_budget
                .saturating_sub(self.queued_bytes.load(Ordering::Relaxed))
        {
            if let Some(log) = fallback {
                log.write_word(&word)?;
            }
            self.queued_bytes.fetch_add(bytes, Ordering::Relaxed);
            Write::Word(word, bytes)
        } else {
            let mut encoded = Vec::new();
            word.encode(&mut encoded)?;
            if let Some(log) = fallback {
                log.append(|encoder| encoder.write_all(&encoded))?;
            }
            self.spill(&encoded)?
        };
        self.send(message)
    }

    fn spill(&mut self, encoded: &[u8]) -> Result<Write, StatsError> {
        if self
            .spill
            .as_ref()
            .is_some_and(|(_, size)| *size >= self.spill_cap)
        {
            // Every earlier offset is consumed before the file can be truncated.
            let (done, drained) = sync_channel(0);
            self.send(Write::Drain(done))?;
            if drained.recv().is_err() {
                self.join()?;
                return Err(StatsError::RunWriteFailed);
            }
            let (file, size) = self.spill.as_mut().expect("existing stats spill");
            file.lock().expect("stats spill mutex").set_len(0)?;
            *size = 0;
        }
        if self.spill.is_none() {
            self.spill = Some((
                Arc::new(Mutex::new(tempfile::tempfile_in(&self.directory)?)),
                0,
            ));
        }
        let (file, size) = self.spill.as_mut().expect("initialized stats spill");
        let offset = *size;
        {
            let mut file = file.lock().expect("stats spill mutex");
            file.seek(SeekFrom::Start(offset))?;
            file.write_all(encoded)?;
        }
        *size += encoded.len() as u64;
        Ok(Write::Buffered {
            file: Arc::clone(file),
            offset,
            len: encoded.len(),
        })
    }

    fn send(&mut self, message: Write) -> Result<(), StatsError> {
        if self
            .send
            .as_ref()
            .ok_or(StatsError::RunWriteFailed)?
            .send(message)
            .is_err()
        {
            self.join()?;
            return Err(StatsError::RunWriteFailed);
        }
        Ok(())
    }

    fn finish(mut self) -> Result<(StatsCache, i64), StatsError> {
        if let Some(send) = &self.send {
            let _ = send.send(Write::Finish);
        }
        self.join()?.ok_or(StatsError::RunWriteFailed)
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if self.thread.is_some() {
            self.cancelled.store(true, Ordering::Relaxed);
            let _ = self.join();
        }
    }
}

/// Writes an ordered batch for atomic publication. A frozen batch requires an empty destination.
pub struct BatchWriter {
    worker: Worker,
    publication: Option<Publication>,
    failed: bool,
}

struct Publication {
    cache: StatsCache,
    file: StageFile,
    run: RunMetadata,
    fallback: Option<StatsRunBuffer>,
    chunk: Option<NonZeroUsize>,
    check_run: Box<CheckRun>,
}

impl BatchWriter {
    /// The compatibility check also runs under the shared cache write lock before the first run.
    pub fn new(
        cache: StatsCache,
        run: RunMetadata,
        chunk: Option<NonZeroUsize>,
        check_run: impl Fn(&Connection) -> Result<(), StatsError> + Send + 'static,
    ) -> Result<Self, StatsError> {
        Self::with_stage(cache, run, chunk, check_run, StageFile::local)
    }

    /// Uses a cleanup subprocess that owns the staging file across interrupts.
    pub fn new_supervised(
        cache: StatsCache,
        run: RunMetadata,
        chunk: Option<NonZeroUsize>,
        check_run: impl Fn(&Connection) -> Result<(), StatsError> + Send + 'static,
    ) -> Result<Self, StatsError> {
        Self::with_stage(cache, run, chunk, check_run, StageFile::supervised)
    }

    fn with_stage(
        cache: StatsCache,
        run: RunMetadata,
        chunk: Option<NonZeroUsize>,
        check_run: impl Fn(&Connection) -> Result<(), StatsError> + Send + 'static,
        make_stage: impl FnOnce(&Path) -> io::Result<StageFile>,
    ) -> Result<Self, StatsError> {
        let directory = cache
            .conn
            .path()
            .and_then(|path| Path::new(path).parent())
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .to_owned();
        let fresh: bool =
            cache
                .conn
                .query_row("SELECT NOT EXISTS(SELECT 1 FROM run)", [], |row| row.get(0))?;
        if fresh {
            let file = make_stage(&directory)?;
            let path = file.path().to_owned();
            let schema = cache.schema_version;
            let worker = Worker::start(run.clone(), chunk, &directory, move |run, _| {
                let conn = Connection::open(path)?;
                // Failed staging files are discarded in full. The shared cache uses a durable journal.
                conn.execute_batch("PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF;")?;
                StatsCache::open_on_version(conn, &run.grammar_hash, schema)?
                    .cache
                    .into_run(run)
            })?;
            Ok(Self {
                worker,
                failed: false,
                publication: Some(Publication {
                    cache,
                    file,
                    run,
                    // Replay preserves IDs, first-seen labels and repeated observations if the destination changes.
                    fallback: chunk
                        .is_some()
                        .then(|| StatsRunBuffer::with_capacity(2 * 1024 * 1024)),
                    chunk,
                    check_run: Box::new(check_run),
                }),
            })
        } else {
            if chunk.is_none() {
                return Err(std::io::Error::other(
                    "frozen stats destination gained rows during preparation",
                )
                .into());
            }
            let worker = Worker::start(run, chunk, &directory, move |run, cancelled| {
                cache.into_batch_run(run, &check_run, Some(cancelled))
            })?;
            Ok(Self {
                worker,
                publication: None,
                failed: false,
            })
        }
    }

    #[cfg(test)]
    pub(super) fn limit_raw_queue(&mut self, bytes: usize) {
        self.worker.raw_budget = bytes;
    }

    #[cfg(test)]
    pub(super) fn limit_spill(&mut self, bytes: u64) {
        self.worker.spill_cap = bytes;
    }

    #[cfg(test)]
    pub(super) fn spilled_bytes(&self) -> u64 {
        self.worker.spill.as_ref().map_or(0, |(_, size)| *size)
    }

    pub fn write_word(&mut self, word: WordRecord) -> Result<(), StatsError> {
        if self.failed {
            return Err(StatsError::RunWriteFailed);
        }
        self.failed = true;
        let fallback = self
            .publication
            .as_mut()
            .and_then(|stage| stage.fallback.as_mut());
        self.worker.record(word, fallback)?;
        self.failed = false;
        Ok(())
    }

    pub fn finish(self) -> Result<(StatsCache, i64), StatsError> {
        if self.failed {
            return Err(StatsError::RunWriteFailed);
        }
        let Self {
            worker,
            publication,
            ..
        } = self;
        let Some(mut stage) = publication else {
            return worker.finish();
        };
        let (staged, last) = worker.finish()?;
        drop(staged);
        let log = stage
            .fallback
            .take()
            .map(StatsRunBuffer::seal)
            .transpose()?;
        if stage
            .cache
            .publish_stage(stage.file.path(), &stage.run, &*stage.check_run)?
        {
            return Ok((stage.cache, last));
        }
        let log = log.ok_or_else(|| {
            std::io::Error::other("frozen stats destination gained rows during parsing")
        })?;
        let writer = stage
            .cache
            .into_batch_run(&stage.run, &*stage.check_run, None)?;
        log.replay_into(writer, &stage.run, stage.chunk)
    }
}

pub(super) fn retry_busy<T>(
    path: &dyn std::fmt::Display,
    cancelled: Option<&AtomicBool>,
    mut operation: impl FnMut() -> Result<T, StatsError>,
) -> Result<T, StatsError> {
    let mut waiting = false;
    loop {
        if cancelled.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            return Err(StatsError::RunWriteFailed);
        }
        match operation() {
            Err(error) if error.is_busy() => {
                if !waiting {
                    eprintln!("stats: waiting for the cache write lock at {path}");
                    waiting = true;
                }
                // SQLite can return BUSY immediately during WAL setup or recovery.
                std::thread::sleep(Duration::from_millis(25));
            }
            result => return result,
        }
    }
}

impl StatsCache {
    fn into_batch_run(
        self,
        run: &RunMetadata,
        check_run: &CheckRun,
        cancelled: Option<&AtomicBool>,
    ) -> Result<StatsRunWriter, StatsError> {
        let path = self.conn.path().unwrap_or("<cache>");
        retry_busy(&path, cancelled, || {
            Ok(self.conn.execute_batch("BEGIN IMMEDIATE")?)
        })?;
        check_run(&self.conn)?;
        self.start_run(run)
    }

    fn publish_stage(
        &mut self,
        path: &Path,
        run: &RunMetadata,
        check_run: &CheckRun,
    ) -> Result<bool, StatsError> {
        let path = path
            .to_str()
            .ok_or_else(|| std::io::Error::other("stats staging path is not UTF-8"))?;
        let destination = self.conn.path().unwrap_or("<cache>");
        retry_busy(&destination, None, || {
            Ok(self.conn.execute("ATTACH DATABASE ?1 AS staged", [path])?)
        })?;
        let tx = retry_busy(&destination, None, || {
            Ok(rusqlite::Transaction::new_unchecked(
                &self.conn,
                TransactionBehavior::Immediate,
            )?)
        })?;
        crate::pack::refuse_packed_writer(&tx)?;
        validate_identity(&tx, self.schema_version, &self.grammar_hash)?;
        check_run(&tx)?;
        let full = StatsProfile::from_schema_version(self.schema_version)? == StatsProfile::Full;
        let empty: bool = tx.query_row(
            "SELECT NOT (EXISTS(SELECT 1 FROM main.run) OR EXISTS(SELECT 1 FROM main.word)
             OR EXISTS(SELECT 1 FROM main.sqlite_schema WHERE type='trigger'))",
            [],
            |row| row.get(0),
        )?;
        let empty = empty && (!full || tx.query_row(
            "SELECT NOT (EXISTS(SELECT 1 FROM main.object) OR EXISTS(SELECT 1 FROM main.feature)
             OR EXISTS(SELECT 1 FROM main.fact) OR EXISTS(SELECT 1 FROM main.gate_fact)
             OR EXISTS(SELECT 1 FROM main.morpheme WHERE morpheme_id<>0)
             OR EXISTS(SELECT 1 FROM main.stratum WHERE stratum_id<>0)
             OR EXISTS(SELECT 1 FROM main.allomorph WHERE allomorph_id<>0))",
            [],
            |row| row.get::<_, bool>(0),
        )?);
        if empty {
            let first = insert_run(&tx, self.schema_version, &self.grammar_hash, run)?;
            if first != 1 {
                return Err(std::io::Error::other(
                    "empty stats destination assigned a noninitial run ID",
                )
                .into());
            }
            tx.execute_batch("
                UPDATE main.run SET (word_count,total_elapsed_ns)=(SELECT word_count,total_elapsed_ns FROM staged.run WHERE run_id=1) WHERE run_id=1;
                INSERT INTO main.run SELECT * FROM staged.run WHERE run_id>1 ORDER BY run_id;
                INSERT INTO main.word SELECT * FROM staged.word;
            ")?;
            if full {
                tx.execute_batch(
                    "
                    INSERT INTO main.stratum SELECT * FROM staged.stratum WHERE stratum_id<>0;
                    INSERT INTO main.allomorph SELECT * FROM staged.allomorph WHERE allomorph_id<>0;
                    INSERT INTO main.morpheme SELECT * FROM staged.morpheme WHERE morpheme_id<>0;
                    INSERT INTO main.object SELECT * FROM staged.object;
                    INSERT INTO main.feature SELECT * FROM staged.feature;
                    INSERT INTO main.fact SELECT * FROM staged.fact;
                    INSERT INTO main.gate_fact SELECT * FROM staged.gate_fact;
                ",
                )?;
            }
        }
        tx.commit()?;
        self.conn.execute_batch("DETACH DATABASE staged")?;
        Ok(empty)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::cache::tests::{sample_run, sample_word};

    #[test]
    fn a_spilling_queue_works_under_a_low_descriptor_limit() {
        const CHILD: &str = "PANGLOSS_TEST_LOW_STATS_FD_LIMIT";
        if std::env::var_os(CHILD).is_none() {
            let output = std::process::Command::new("sh")
                .args(["-c", "ulimit -n 64 && exec \"$@\"", "stats-low-fd"])
                .arg(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    concat!(
                        module_path!(),
                        "::a_spilling_queue_works_under_a_low_descriptor_limit"
                    )
                    .trim_start_matches("pg_stats::"),
                ])
                .env(CHILD, "1")
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                output.status.success() && stdout.contains("1 passed"),
                "{stdout}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let (go, wait) = sync_channel(0);
        let mut worker = Worker::start(sample_run(), None, dir.path(), move |run, _| {
            wait.recv_timeout(Duration::from_secs(30)).unwrap();
            StatsCache::open_in_memory(&run.grammar_hash)?
                .cache
                .into_run(run)
        })
        .unwrap();
        let go = go;
        worker.raw_budget = 1;
        let forms: Vec<_> = (0..128).map(|i| format!("word-{i}")).collect();
        for form in &forms {
            let mut word = sample_word(form);
            word.facts[0].object_label = "a".repeat(20 * 1024);
            worker.record(word, None).unwrap();
        }
        assert!(worker.spill.as_ref().unwrap().1 > 128 * 16 * 1024);
        go.send(()).unwrap();
        let (cache, _) = worker.finish().unwrap();
        let stored: Vec<String> = cache
            .connection()
            .prepare("SELECT form FROM word ORDER BY word_id")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(stored, forms);
    }
}

//! Measurement-only census counters for prefilter research. Everything is behind the `HC_PREFILTER_CENSUS=<path>` environment variable; when it is unset every entry point returns after one cached flag read and nothing about parsing changes.
//!
//! Rows are `word, rule, census, cell, calls, work, shadow_units, shadow_arcs`, appended to the file when the next word begins and when the guard drops.

use std::collections::BTreeMap;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

static ENABLED: AtomicBool = AtomicBool::new(false);
static INIT: OnceLock<()> = OnceLock::new();

#[derive(Default)]
struct State {
    file: Option<std::fs::File>,
    word: String,
    rule: String,
    rows: BTreeMap<(String, &'static str, &'static str), [u64; 4]>,
}

static STATE: Mutex<State> = Mutex::new(State {
    file: None,
    word: String::new(),
    rule: String::new(),
    rows: BTreeMap::new(),
});

/// Whether the census is on; the environment is read once.
#[inline]
pub fn enabled() -> bool {
    INIT.get_or_init(|| {
        if let Some(path) = std::env::var_os("HC_PREFILTER_CENSUS") {
            if let Ok(f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                STATE.lock().unwrap().file = Some(f);
                ENABLED.store(true, Ordering::Relaxed);
            }
        }
    });
    ENABLED.load(Ordering::Relaxed)
}

fn flush(st: &mut State) {
    let rows = std::mem::take(&mut st.rows);
    let word = st.word.clone();
    if let Some(f) = st.file.as_mut() {
        for ((rule, census, cell), v) in rows {
            let _ = writeln!(
                f,
                "{word}\t{rule}\t{census}\t{cell}\t{}\t{}\t{}\t{}",
                v[0], v[1], v[2], v[3]
            );
        }
        let _ = f.flush();
    }
}

/// Start a word: write the previous word's rows and label the next ones.
pub fn begin_word(word: &str) {
    if !enabled() {
        return;
    }
    let mut st = STATE.lock().unwrap();
    flush(&mut st);
    st.word = word.to_string();
}

/// Writes the last word's rows when dropped.
pub struct Guard;
impl Drop for Guard {
    fn drop(&mut self) {
        check_report();
        if enabled() {
            flush(&mut STATE.lock().unwrap());
        }
    }
}

/// Name the rule whose subrules the next records belong to (affix census).
pub fn set_rule(rule: String) {
    STATE.lock().unwrap().rule = rule;
}

/// Add one call to a cell: `work` steps the real operation consumed, and two shadow cost units.
pub fn record(
    rule: Option<&str>,
    census: &'static str,
    cell: &'static str,
    work: u64,
    a: u64,
    b: u64,
) {
    let mut st = STATE.lock().unwrap();
    let rule = rule.map(str::to_string).unwrap_or_else(|| st.rule.clone());
    let e = st.rows.entry((rule, census, cell)).or_insert([0; 4]);
    e[0] += 1;
    e[1] += work;
    e[2] += a;
    e[3] += b;
}

/// Work steps spent so far on the active budget (0 when none).
pub fn used() -> u64 {
    crate::work::active_budget().map_or(0, |b| b.used() as u64)
}

/// True when the active budget has stopped the search, so a result is not a real "nothing".
pub fn stopped() -> bool {
    crate::work::active_budget().is_some_and(|b| b.capped() || b.timed_out())
}

/// True when work is suspended (a replay that reserves without spending): such calls are not counted.
pub fn suspended() -> bool {
    crate::work::active_budget().is_some_and(|b| b.is_suspended())
}

static GATE: OnceLock<u8> = OnceLock::new();
static CHECK_CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static CHECK_ACCEPT_EMPTY: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Measuring only: the landed version has no switch. `HC_ACCEPT_GATE`: 0 off (default), 1 skip a traversal the acceptance pass rejects, 2 (`check`) run both and panic if the pass rejects a call whose traversal matched.
pub fn gate_mode() -> u8 {
    *GATE.get_or_init(|| match std::env::var("HC_ACCEPT_GATE").as_deref() {
        Ok("1") => 1,
        Ok("check") => 2,
        _ => 0,
    })
}

/// Check mode: count one call; `accepts_empty` marks a pass that accepted a traversal that found nothing (allowed, not a bug).
pub fn check_note(accepts_empty: bool) {
    CHECK_CALLS.fetch_add(1, Ordering::Relaxed);
    if accepts_empty {
        CHECK_ACCEPT_EMPTY.fetch_add(1, Ordering::Relaxed);
    }
}

/// Print the check-mode totals once, at the end of a batch.
pub fn check_report() {
    if gate_mode() == 2 {
        eprintln!(
            "ACCEPT_GATE_CHECK calls={} accepts_but_empty={} rejects_but_nonempty=0 (a panic would have stopped the run)",
            CHECK_CALLS.load(Ordering::Relaxed),
            CHECK_ACCEPT_EMPTY.load(Ordering::Relaxed)
        );
    }
}

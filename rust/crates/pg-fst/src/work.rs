//! A shared deterministic work allowance for one search, with a scoped adapter for nested matchers.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use web_time::{Duration, Instant};

/// Shared cap state; clones spend the same allowance and observe the same stop reason.
#[derive(Clone)]
pub struct WorkBudget(Rc<State>);

struct State {
    cap: usize,
    used: Cell<usize>,
    capped: Cell<bool>,
    deadline: Cell<Option<Instant>>,
    timed_out: Cell<bool>,
    /// Ceiling on capture state one search may hold at once, in estimated bytes (`None` = unbounded).
    retained_cap: Cell<Option<usize>>,
    retained: Cell<usize>,
    retained_peak: Cell<usize>,
    retained_capped: Cell<bool>,
}

/// Default ceiling on the capture state one word's search holds at once, as estimated by `WorkBudget::charge_retained`.
/// This bounds the matcher's accounted register blocks and visited entries, not total process RSS or grammar storage.
pub const DEFAULT_RETAINED_CAP_BYTES: usize = 256 * 1024 * 1024;

/// Reservations between clock reads: a read costs about 90 instructions, and a timeout overshoots by at most this many units.
const DEADLINE_POLL_INTERVAL: usize = 64;

thread_local! {
    static ACTIVE: RefCell<Option<WorkBudget>> = const { RefCell::new(None) };
}

impl WorkBudget {
    pub fn new(cap: usize) -> Self {
        Self(Rc::new(State {
            cap,
            used: Cell::new(0),
            capped: Cell::new(false),
            deadline: Cell::new(None),
            timed_out: Cell::new(false),
            retained_cap: Cell::new(Some(DEFAULT_RETAINED_CAP_BYTES)),
            retained: Cell::new(0),
            retained_peak: Cell::new(0),
            retained_capped: Cell::new(false),
        }))
    }

    /// Replace the retained-capture ceiling (`None` = unbounded).
    pub fn with_retained_cap(self, cap: Option<usize>) -> Self {
        self.0.retained_cap.set(cap);
        self
    }

    /// Reserve estimated live capture bytes. A failed reservation latches a memory stop as well as an incomplete search.
    pub fn charge_retained(&self, bytes: usize) -> bool {
        let total = self.0.retained.get().saturating_add(bytes);
        if self.0.retained_cap.get().is_some_and(|cap| total > cap) {
            self.0.retained_capped.set(true);
            self.0.capped.set(true);
            return false;
        }
        self.0.retained.set(total);
        self.0
            .retained_peak
            .set(self.0.retained_peak.get().max(total));
        true
    }

    /// Return `bytes` previously charged once the matcher drops that state.
    pub fn release_retained(&self, bytes: usize) {
        self.0
            .retained
            .set(self.0.retained.get().saturating_sub(bytes));
    }

    /// Capture bytes charged right now.
    pub fn retained(&self) -> usize {
        self.0.retained.get()
    }

    /// The most capture bytes this budget ever had charged at once.
    pub fn retained_peak(&self) -> usize {
        self.0.retained_peak.get()
    }

    /// Whether the retained-capture ceiling (not the step cap) stopped the search.
    pub fn retained_capped(&self) -> bool {
        self.0.retained_capped.get()
    }

    pub fn with_timeout(self, timeout: Option<Duration>) -> Self {
        self.0.deadline.set(timeout.map(|d| Instant::now() + d));
        self
    }

    /// Reserve one work unit before exploring a branch or materializing a candidate.
    /// Failed reservations latch the reason and never advance the counter beyond the cap.
    pub fn consume(&self) -> bool {
        let stopped = if self.0.used.get().is_multiple_of(DEADLINE_POLL_INTERVAL) {
            self.stopped()
        } else {
            self.exhausted()
        };
        if stopped {
            return false;
        }
        self.0.used.set(self.0.used.get().saturating_add(1));
        true
    }

    /// A poll at the cap latches exhaustion even when the last reservation succeeded.
    pub fn stopped(&self) -> bool {
        if self.exhausted() {
            return true;
        }
        if self.0.deadline.get().is_some_and(|d| Instant::now() >= d) {
            self.0.timed_out.set(true);
            return true;
        }
        false
    }

    /// The deterministic half of `stopped`: latched stops and the cap, without reading the clock.
    fn exhausted(&self) -> bool {
        if self.0.capped.get() || self.0.timed_out.get() {
            return true;
        }
        if self.0.used.get() >= self.0.cap {
            self.0.capped.set(true);
            return true;
        }
        false
    }

    pub fn used(&self) -> usize {
        self.0.used.get()
    }

    /// Whether two handles refer to the same allowance.
    pub fn same_as(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    pub fn capped(&self) -> bool {
        self.0.capped.get()
    }

    pub fn timed_out(&self) -> bool {
        self.0.timed_out.get()
    }

    /// Install this explicit budget for synchronous matcher calls on the current thread.
    /// Nested scopes restore their caller on drop, including during unwinding. The guard
    /// cannot cross threads. Standalone calls outside a scope retain their uncapped behavior.
    pub fn enter(&self) -> WorkScope {
        let previous = ACTIVE.with(|active| active.replace(Some(self.clone())));
        WorkScope {
            current: self.clone(),
            previous,
        }
    }
}

/// Capture the current allowance for a synchronous diagnostic scope.
pub fn active_budget() -> Option<WorkBudget> {
    ACTIVE.with(|active| active.borrow().clone())
}

/// Restores the preceding thread-local budget when synchronous work finishes.
#[must_use]
pub struct WorkScope {
    current: WorkBudget,
    previous: Option<WorkBudget>,
}

impl Drop for WorkScope {
    fn drop(&mut self) {
        ACTIVE.with(|active| {
            let mut active = active.borrow_mut();
            assert!(
                active
                    .as_ref()
                    .is_some_and(|b| Rc::ptr_eq(&b.0, &self.current.0)),
                "work budget scopes must be dropped in nesting order"
            );
            *active = self.previous.take();
        });
    }
}

/// Charge the installed search budget; standalone operations have no allowance to consume.
pub fn consume() -> bool {
    ACTIVE.with(|active| active.borrow().as_ref().is_none_or(WorkBudget::consume))
}

/// Charge capture state to the installed budget; standalone operations are unbounded.
pub fn charge_retained(bytes: usize) -> bool {
    ACTIVE.with(|active| {
        active
            .borrow()
            .as_ref()
            .is_none_or(|b| b.charge_retained(bytes))
    })
}

/// Return capture state charged with `charge_retained`.
pub fn release_retained(bytes: usize) {
    ACTIVE.with(|active| {
        if let Some(b) = active.borrow().as_ref() {
            b.release_retained(bytes);
        }
    });
}

/// Poll the installed search budget without charging another unit.
pub fn stopped() -> bool {
    ACTIVE.with(|active| active.borrow().as_ref().is_some_and(WorkBudget::stopped))
}

#[cfg(test)]
mod tests;

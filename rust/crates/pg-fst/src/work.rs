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
}

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
        }))
    }

    pub fn with_timeout(self, timeout: Option<Duration>) -> Self {
        self.0.deadline.set(timeout.map(|d| Instant::now() + d));
        self
    }

    /// Reserve one work unit before exploring a branch or materializing a candidate.
    /// Failed reservations latch the reason and never advance the counter beyond the cap.
    pub fn consume(&self) -> bool {
        if self.stopped() {
            return false;
        }
        self.0.used.set(self.0.used.get().saturating_add(1));
        true
    }

    /// A poll at the cap latches exhaustion even when the last reservation succeeded.
    pub fn stopped(&self) -> bool {
        if self.0.capped.get() || self.0.timed_out.get() {
            return true;
        }
        if self.0.used.get() >= self.0.cap {
            self.0.capped.set(true);
            return true;
        }
        if self.0.deadline.get().is_some_and(|d| Instant::now() >= d) {
            self.0.timed_out.set(true);
            return true;
        }
        false
    }

    pub fn used(&self) -> usize {
        self.0.used.get()
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

/// Poll the installed search budget without charging another unit.
pub fn stopped() -> bool {
    ACTIVE.with(|active| active.borrow().as_ref().is_some_and(WorkBudget::stopped))
}

#[cfg(test)]
mod tests;

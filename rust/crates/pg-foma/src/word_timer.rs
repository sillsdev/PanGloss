//! The one wall-clock reading this crate's pipeline takes, isolated so the wasm32 caveat is stated
//! once.
//!
//! `Instant::now()` compiles on `wasm32-unknown-unknown` and aborts at runtime, so that target
//! reports `Duration::ZERO` rather than timing. Every number this produces is an observation:
//! nothing here belongs in a canonical report or an assertion, because a duration is not
//! reproducible.

#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct Timer(std::time::Instant);

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn start() -> Timer {
    Timer(std::time::Instant::now())
}

#[cfg(not(target_arch = "wasm32"))]
impl Timer {
    pub(crate) fn elapsed(&self) -> std::time::Duration {
        self.0.elapsed()
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) struct Timer;

#[cfg(target_arch = "wasm32")]
pub(crate) fn start() -> Timer {
    Timer
}

#[cfg(target_arch = "wasm32")]
impl Timer {
    pub(crate) fn elapsed(&self) -> std::time::Duration {
        std::time::Duration::ZERO
    }
}

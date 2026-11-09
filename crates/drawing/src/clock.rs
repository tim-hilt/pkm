//! The time of day. `std::time::SystemTime` panics on targets without an
//! operating system (WebAssembly in a browser), so a client there supplies
//! the time with [`set_clock`]; everywhere else the system's is used.

use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

static CLOCK: OnceLock<fn() -> f64> = OnceLock::new();

/// Makes `clock` the source of time: milliseconds since 1970. Only the first
/// call has an effect. Call it before the first [`crate::Editor`] is made, on
/// targets that have no system time.
pub fn set_clock(clock: fn() -> f64) {
    let _ = CLOCK.set(clock);
}

/// Milliseconds since 1970, as Excalidraw's `updated` and `created`.
pub(crate) fn now_ms() -> f64 {
    match CLOCK.get() {
        Some(clock) => clock(),
        None => SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0.0, |since| since.as_millis() as f64),
    }
}

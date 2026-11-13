//! The time of day. `std::time::SystemTime` panics on targets without an
//! operating system (WebAssembly in a browser), so a client there supplies
//! the time with [`set_clock`]; everywhere else the system's is used.

use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

static CLOCK: OnceLock<fn() -> f64> = OnceLock::new();

/// Makes `clock` the source of time: milliseconds since 1970. Only the first
/// call has an effect. Call it before anything reads the time, on
/// targets that have no system time.
pub fn set_clock(clock: fn() -> f64) {
    let _ = CLOCK.set(clock);
}

/// Milliseconds since 1970, as Excalidraw's `updated` and `created`.
pub fn now_ms() -> f64 {
    match CLOCK.get() {
        Some(clock) => clock(),
        None => SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0.0, |since| since.as_millis() as f64),
    }
}

/// `ms` (milliseconds since 1970) as UTC ISO 8601 to the second:
/// `2023-11-14T22:13:20Z`.
pub fn iso8601(ms: f64) -> String {
    let seconds = (ms / 1000.0).floor() as i64;
    let (days, of_day) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    // Civil date from days since 1970 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    let (hour, minute, second) = (of_day / 3600, of_day % 3600 / 60, of_day % 60);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

#[cfg(test)]
mod tests {
    use super::iso8601;

    #[test]
    fn formats_known_instants() {
        assert_eq!(iso8601(0.0), "1970-01-01T00:00:00Z");
        assert_eq!(iso8601(1_700_000_000_000.0), "2023-11-14T22:13:20Z");
        // A leap day, and the instant before the epoch.
        assert_eq!(iso8601(1_709_164_800_000.0), "2024-02-29T00:00:00Z");
        assert_eq!(iso8601(-1000.0), "1969-12-31T23:59:59Z");
    }
}

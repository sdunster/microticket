//! Wall-clock helpers.
//!
//! Time-dependent logic elsewhere (expiry checks, throttling) is written as pure
//! functions taking `now` as an explicit parameter rather than by injecting a clock
//! trait/mock — see [`crate::expire`] for the convention in practice. These two
//! functions are the only place "the actual current time" is read; everything else
//! can be unit tested with a fixed `now`.

use std::time::SystemTime;

/// Current Unix time in whole seconds (UTC). Used for all created/updated/expiry
/// timestamps.
pub fn now_sec() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Current Unix time in milliseconds (UTC), signed so it can be differenced against
/// another clock without underflowing when that clock is behind.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

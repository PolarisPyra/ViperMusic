use std::time::{SystemTime, UNIX_EPOCH};

/// Return the current Unix time in whole seconds, or zero if the clock is before the epoch.
pub(crate) fn unix_time_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

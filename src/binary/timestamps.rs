/// Seconds in a day.
pub const DAY: u32 = 24 * 60 * 60;

/// Rounds the given unix-timestamp down to days.
pub fn round_timestamp_to_day(timestamp: u32) -> u32 {
    timestamp / DAY * DAY
}

/// Calculates the offset (in days / indices) between
/// the timestamp saved in the file vs "now".
pub fn days_offset(timestamp_saved: u32, timestamp_now: u32) -> isize {
    let days_saved = timestamp_saved / DAY;
    let days_now = timestamp_now / DAY;

    days_now as isize - days_saved as isize
}

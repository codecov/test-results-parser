use std::ops::Range;

/// Seconds in a day.
pub const DAY: u32 = 24 * 60 * 60;

/// Rounds the given unix-timestamp down to days.
pub fn round_timestamp_to_day(timestamp: u32) -> u32 {
    timestamp / DAY * DAY
}

/// Calculates the offset (in days / indices) between
/// the "saved" timestamp vs "now".
pub fn offset_from_today(timestamp_saved: u32, timestamp_now: u32) -> isize {
    let days_saved = timestamp_saved / DAY;
    let days_now = timestamp_now / DAY;

    days_saved as isize - days_now as isize
}

/// This adjusts the `desired_range` to select the right subset of `data_range`
/// so that it matches up the days we want to select.
///
/// The `desired_range` is always starts from "today" (0), and goes into the past.
/// So a range `0..2` (exclusive) would select "today" (0) and "yesterday" (1).
///
/// To give an example using calendar days, our data, offset, desired and resulting
/// ranges may look like this:
/// ```
/// let data_range = 20..24; // representing data from 2024-11-20 to 2024-11-18
/// // … | 2024-11-20 | 2024-11-20 | 2024-11-19 | 2024-11-18 | …
/// //                ^- 20        |            |            ^- 23
///
/// let today_offset = -1;
/// // … | 2024-11-20 | …
/// //   ^ today
///
/// let desired_range = 0..2; // today and yesterday
///
/// let resulting_range = adjust_selection_range(data_range, desired_range, today_offset);
/// assert_eq!(resulting_range, 20..21);
/// // … | 2024-11-20 | 2024-11-20 | …
/// //                ^- 20        ^- 21
/// ```
pub fn adjust_selection_range(
    data_range: Range<usize>,
    desired_range: Range<usize>,
    today_offset: isize,
) -> Range<usize> {
    let range_start = data_range
        .start
        .saturating_add(desired_range.start)
        .saturating_add_signed(today_offset);
    let range_end = data_range
        .start
        .saturating_add(desired_range.end)
        .saturating_add_signed(today_offset);
    let range_start = range_start.max(data_range.start);
    let range_end = range_end.min(data_range.end);
    range_start..range_end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_day_offsets() {
        let offset = offset_from_today(0, 1 * DAY);
        assert_eq!(offset, -1);

        let offset = offset_from_today(0, 7 * DAY);
        assert_eq!(offset, -7);

        let offset = offset_from_today(2 * DAY, 1 * DAY);
        assert_eq!(offset, 1);
    }

    #[test]
    fn test_range_adjustment() {
        let range = adjust_selection_range(0..60, 0..7, 0);
        assert_eq!(range, 0..7);

        let range = adjust_selection_range(0..7, 0..60, 0);
        assert_eq!(range, 0..7);

        let range = adjust_selection_range(20..28, 0..60, -2);
        assert_eq!(range, 20..28);

        let range = adjust_selection_range(20..80, 0..7, 1);
        assert_eq!(range, 21..28);
    }
}

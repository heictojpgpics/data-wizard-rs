//! FAT timestamp decoding.
//!
//! FAT filesystems (FAT12/16/32 and exFAT) store timestamps in a packed
//! bitfield format defined by Microsoft's FAT specification
//! ("FAT32 Specification", section 6.1: *Date and Time Formats*, also
//! shared by the exFAT specification section 7.4). All FAT-family
//! timestamps are stored as **local time without a timezone**; the
//! conventional interpretation (used by forensic tooling) is to treat
//! them as UTC. We do the same and document it.
//!
//! # Formats
//!
//! **Date (u16)**
//! ```text
//! bits 15..9 : year - 1980   (0..=127  ->  1980..=2107)
//! bits  8..5 : month          (1..=12)
//! bits  4..0 : day            (1..=31)
//! ```
//!
//! **Time (u16)**
//! ```text
//! bits 15..11 : hour     (0..=23)
//! bits  10..5 : minute   (0..=59)
//! bits   4..0 : second / 2  (0..=29  ->  0..=58 seconds)
//! ```
//!
//! Creation time additionally carries a `tenths` byte (10 ms units,
//! 0..=199) stored separately in the directory entry.
//!
//! # Validation policy (strict, no silent fallback)
//!
//! A zero date or zero time means "not set" per the spec, and malformed
//! values (month 0, day 0, hour 24, ...) are returned as `None` rather
//! than clamped. Reporting unknown beats inventing a plausible value.

use time::{Date, OffsetDateTime, Time};

/// Decode a FAT date/time pair into a UTC timestamp.
///
/// `tenths` adds 10 ms resolution to the creation time when supplied
/// (0..=199 -> 0..=1.99 s). Values above 199 are clamped by the spec
/// ceiling, not rejected, because they only refine an otherwise valid
/// timestamp.
pub fn decode_fat_datetime(date: u16, time: u16, tenths: Option<u8>) -> Option<OffsetDateTime> {
    // A zero date or zero time field means "not set" per the spec.
    if date == 0 || time == 0 {
        return None;
    }

    let year = 1980u16 + ((date >> 9) & 0x7F);
    let month = ((date >> 5) & 0x0F) as u8;
    let day = (date & 0x1F) as u8;

    let hour = ((time >> 11) & 0x1F) as u8;
    let minute = ((time >> 5) & 0x3F) as u8;
    let second2 = (time & 0x1F) as u8; // seconds / 2

    // Strict range validation. Anything out of range is corrupt metadata;
    // we report None rather than clamping.
    if !(1..=12).contains(&month) {
        return None;
    }
    if !(1..=31).contains(&day) {
        return None;
    }
    if hour > 23 || minute > 59 || second2 > 29 {
        return None;
    }

    // Reject impossible calendar dates (Feb 30 etc.). time::Date::from_
    // calendar_date returns Err for these, so map it to None.
    let date_obj =
        Date::from_calendar_date(i32::from(year), time::Month::try_from(month).ok()?, day).ok()?;

    // 10 ms resolution: the FAT time field has 2-second granularity; the
    // tenths byte refines within that 2 s window (spec: 0..=199 -> 0..=1.99 s
    // on top of the truncated-to-2s seconds). Values above the spec ceiling
    // of 199 are clamped to 199, not rejected: they only refine an otherwise
    // valid timestamp.
    //
    // Reconstruction: actual = base_seconds + tenths*10ms, so the result can
    // cross into the next second (e.g. 14:30:20 + 1.37s -> 14:30:21.370).
    // second2 <= 29 (checked above), so base <= 58 and base+1 <= 59: the
    // sum never overflows the minute.
    let tenths = u16::from(tenths.unwrap_or(0)).min(199);
    let total_window_ms = u64::from(tenths) * 10; // 0..=1990
    let extra_secs = (total_window_ms / 1000) as u8; // 0..=1
    let millis = (total_window_ms % 1000) as u16; // 0..=990

    let time_obj = Time::from_hms_milli(hour, minute, second2 * 2 + extra_secs, millis).ok()?;
    Some(OffsetDateTime::new_utc(date_obj, time_obj))
}

/// Decode the FAT32 16-bit "last access date" (date only, no time).
///
/// The spec stores only a date for last access; midnight is used as the
/// time component when the date is valid.
pub fn decode_fat_access_date(date: u16) -> Option<OffsetDateTime> {
    // Reuse the full decoder with a midnight time. A midnight time value
    // (0) would be rejected as "not set"; use exactly 00:00:00 by calling
    // with time=1 (00:00:02) is wrong. Decode the date fields directly
    // instead so the semantics stay honest.
    if date == 0 {
        return None;
    }
    let year = 1980u16 + ((date >> 9) & 0x7F);
    let month = ((date >> 5) & 0x0F) as u8;
    let day = (date & 0x1F) as u8;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let date_obj =
        Date::from_calendar_date(i32::from(year), time::Month::try_from(month).ok()?, day).ok()?;
    let time_obj = Time::from_hms(0, 0, 0).ok()?;
    Some(OffsetDateTime::new_utc(date_obj, time_obj))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_valid_datetime() {
        // 2021-06-15 14:30:20 -> date bits: year 41 (2021), month 6, day 15
        let date = (41 << 9) | (6 << 5) | 15;
        // 14:30:20 -> hour 14, min 30, sec/2 = 10
        let time = (14 << 11) | (30 << 5) | 10;
        let ts = decode_fat_datetime(date, time, None).expect("valid datetime");
        assert_eq!(ts.year(), 2021);
        assert_eq!(u8::from(ts.month()), 6);
        assert_eq!(ts.day(), 15);
        assert_eq!(ts.hour(), 14);
        assert_eq!(ts.minute(), 30);
        assert_eq!(ts.second(), 20);
    }

    #[test]
    fn tenths_add_10ms_resolution() {
        let date = (41 << 9) | (6 << 5) | 15;
        let time = (14 << 11) | (30 << 5) | 10; // 14:30:20 (2s granularity)
        let ts = decode_fat_datetime(date, time, Some(137)).expect("valid");
        // 14:30:20 + 1.37s = 14:30:21.370: the tenths refine the 2-second
        // window and can cross into the next second.
        assert_eq!(ts.second(), 21);
        assert_eq!(ts.millisecond(), 370);
    }

    #[test]
    fn tenths_at_window_edge_do_not_overflow_minute() {
        // Base 14:30:58 (second2 = 29) + 1.99s = 14:30:59.990 -- still within
        // the minute (58 + 1 = 59 <= 59).
        let date = (41 << 9) | (6 << 5) | 15;
        let time = (14 << 11) | (30 << 5) | 29;
        let ts = decode_fat_datetime(date, time, Some(199)).expect("valid");
        assert_eq!(ts.second(), 59);
        assert_eq!(ts.millisecond(), 990);
    }

    #[test]
    fn zero_date_or_time_means_unset() {
        assert!(decode_fat_datetime(0, 12345, None).is_none());
        assert!(decode_fat_datetime(12345, 0, None).is_none());
    }

    #[test]
    fn rejects_invalid_month() {
        // month = 0
        let date = (41 << 9) | (0 << 5) | 15;
        let time = (14 << 11) | (30 << 5) | 10;
        assert!(decode_fat_datetime(date, time, None).is_none());
    }

    #[test]
    fn rejects_invalid_day() {
        // day = 0
        let date = (41 << 9) | (6 << 5) | 0;
        let time = (14 << 11) | (30 << 5) | 10;
        assert!(decode_fat_datetime(date, time, None).is_none());
    }

    #[test]
    fn rejects_impossible_calendar_date() {
        // Feb 30, 2021 (month=2 day=30 is syntactically valid but impossible)
        let date = (41 << 9) | (2 << 5) | 30;
        let time = (14 << 11) | (30 << 5) | 10;
        assert!(decode_fat_datetime(date, time, None).is_none());
    }

    #[test]
    fn rejects_invalid_hour_and_minute() {
        let date = (41 << 9) | (6 << 5) | 15;
        // hour 24
        let time = (24 << 11) | (30 << 5) | 10;
        assert!(decode_fat_datetime(date, time, None).is_none());
        // minute 60
        let time = (14 << 11) | (60 << 5) | 10;
        assert!(decode_fat_datetime(date, time, None).is_none());
        // second/2 = 30 (60s)
        let time = (14 << 11) | (30 << 5) | 30;
        assert!(decode_fat_datetime(date, time, None).is_none());
    }

    #[test]
    fn access_date_uses_midnight() {
        // 2000-01-01
        let date = (20 << 9) | (1 << 5) | 1;
        let ts = decode_fat_access_date(date).expect("valid");
        assert_eq!(ts.year(), 2000);
        assert_eq!(u8::from(ts.month()), 1);
        assert_eq!(ts.day(), 1);
        assert_eq!((ts.hour(), ts.minute(), ts.second()), (0, 0, 0));
    }

    #[test]
    fn access_date_zero_is_unset() {
        assert!(decode_fat_access_date(0).is_none());
    }

    #[test]
    fn earliest_representable_date() {
        // 1980-01-01 00:00:00 -> date=1? No: date bits year=0 month=1 day=1 -> value 0x21
        let date = (0 << 9) | (1 << 5) | 1;
        let time = (0 << 11) | (0 << 5) | 0;
        // time 0 means unset per spec -> None
        assert!(decode_fat_datetime(date, time, None).is_none());
        // With time 00:00:02 (time field = 1) it is representable
        let ts = decode_fat_datetime(date, 1, None).expect("valid");
        assert_eq!(ts.year(), 1980);
        assert_eq!(ts.second(), 2);
    }
}

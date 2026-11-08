//! A shared file keeps its modification time on both sides. The agent lists a
//! guest file's LastWriteTimeUtc in .NET's round-trip form
//! (`2026-11-08T00:05:12.1234567Z`); the host sends a Mac file's time as
//! milliseconds since the Unix epoch.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Milliseconds since the Unix epoch of a listed guest time; None for any
/// other form or a time before 1970, which then simply is not copied.
pub fn guest_mtime_ms(listed: &str) -> Option<u128> {
    let (date, time) = listed.strip_suffix('Z')?.split_once('T')?;
    let mut date = date.splitn(3, '-').map(|part| part.parse::<i64>().ok());
    let (year, month, day) = (date.next()??, date.next()??, date.next()??);
    let (clock, fraction) = time.split_once('.').unwrap_or((time, ""));
    let mut clock = clock.splitn(3, ':').map(|part| part.parse::<i64>().ok());
    let (hour, minute, second) = (clock.next()??, clock.next()??, clock.next()??);
    // The guest sends this text: .NET's DateTime range (years 1-9999) bounds the arithmetic.
    if !(1..=9999).contains(&year) || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    if !(0..=23).contains(&hour) || !(0..=59).contains(&minute) || !(0..=60).contains(&second) {
        return None;
    }
    if !fraction.bytes().all(|b| b.is_ascii_digit()) || fraction.len() > 7 {
        return None;
    }
    let millis: i64 = format!("{fraction:0<3}")[..3].parse().ok()?;
    let seconds = days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second;
    u128::try_from(seconds * 1_000 + millis).ok()
}

pub fn system_time(mtime_ms: u128) -> Option<SystemTime> {
    UNIX_EPOCH.checked_add(Duration::from_millis(u64::try_from(mtime_ms).ok()?))
}

/// Days since 1970-01-01 of a proleptic Gregorian date (H. Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
#[path = "share_mtime_tests.rs"]
mod tests;

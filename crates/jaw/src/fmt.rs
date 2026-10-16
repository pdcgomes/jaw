//! Times, counts and widths as the screen writes them.

use chrono::{DateTime, Datelike, FixedOffset, TimeZone};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub const MINUTE: i64 = 60_000;
pub const DAY: i64 = 24 * 60 * MINUTE;

fn local(at: i64, tz: FixedOffset) -> DateTime<FixedOffset> {
    tz.timestamp_millis_opt(at).single().unwrap_or_else(|| tz.timestamp_millis_opt(0).unwrap())
}

/// Milliseconds on the local clock, for comparing days and minutes without a calendar.
fn wall(at: i64, tz: FixedOffset) -> i64 {
    at + tz.local_minus_utc() as i64 * 1_000
}

/// Days from `at`'s day to `now`'s, in the local calendar.
fn days_before(at: i64, now: i64, tz: FixedOffset) -> i64 {
    wall(now, tz).div_euclid(DAY) - wall(at, tz).div_euclid(DAY)
}

/// The same minute of the same day, which a burst of messages shares.
pub fn same_minute(a: i64, b: i64, tz: FixedOffset) -> bool {
    wall(a, tz).div_euclid(MINUTE) == wall(b, tz).div_euclid(MINUTE)
}

pub fn same_day(a: i64, b: i64, tz: FixedOffset) -> bool {
    days_before(a, b, tz) == 0
}

pub fn clock(at: i64, tz: FixedOffset) -> String {
    let minutes = wall(at, tz).rem_euclid(DAY) / MINUTE;
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

const WEEKDAYS: [&str; 7] = ["Thursday", "Friday", "Saturday", "Sunday", "Monday", "Tuesday", "Wednesday"];

/// The day of the week, from the days since 1 January 1970, which was a Thursday.
fn weekday(at: i64, tz: FixedOffset) -> &'static str {
    WEEKDAYS[wall(at, tz).div_euclid(DAY).rem_euclid(7) as usize]
}

/// A day separator's label: Today, Yesterday, a weekday within the week, or the date.
pub fn day(at: i64, now: i64, tz: FixedOffset) -> String {
    match days_before(at, now, tz) {
        0 => "Today".into(),
        1 => "Yesterday".into(),
        2..=6 => weekday(at, tz).to_string(),
        _ if local(at, tz).year() == local(now, tz).year() => local(at, tz).format("%A %-d %B").to_string(),
        _ => local(at, tz).format("%-d %B %Y").to_string(),
    }
}

/// When a chat's newest message came, as the chat list says it.
pub fn when(at: i64, now: i64, tz: FixedOffset) -> String {
    match days_before(at, now, tz) {
        0 => clock(at, tz),
        1 => "yesterday".into(),
        2..=6 => weekday(at, tz)[..3].to_string(),
        _ if local(at, tz).year() == local(now, tz).year() => local(at, tz).format("%-d %b").to_string(),
        _ => local(at, tz).format("%-d %b %Y").to_string(),
    }
}

/// When something ends, such as a ban: its date and time.
pub fn until(at: i64, tz: FixedOffset) -> String {
    local(at, tz).format("%-d %B, %H:%M").to_string()
}

/// Everything about when, for the selected message.
pub fn full(at: i64, tz: FixedOffset) -> String {
    local(at, tz).format("%A %-d %B %Y, %H:%M:%S").to_string()
}

/// A count with its thousands marked, as 1,204.
pub fn num(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

pub fn width(s: &str) -> usize {
    s.width()
}

/// `s` cut to `w` columns, with an ellipsis where it was cut.
pub fn fit(s: &str, w: usize) -> String {
    if s.width() <= w {
        return s.to_string();
    }
    if w == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for g in s.graphemes(true) {
        let gw = g.width();
        if used + gw > w - 1 {
            break;
        }
        out.push_str(g);
        used += gw;
    }
    out.push('…');
    out
}

/// One line of `s`: its line breaks as spaces.
pub fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const UTC: FixedOffset = FixedOffset::east_opt(0).unwrap();
    /// Friday 16 October 2026, 14:02 UTC.
    const NOW: i64 = 1_792_159_320_000;

    #[test]
    fn days_say_how_far_back_they_are() {
        assert_eq!(day(NOW - 60_000, NOW, UTC), "Today");
        assert_eq!(day(NOW - DAY, NOW, UTC), "Yesterday");
        assert_eq!(day(NOW - 2 * DAY, NOW, UTC), "Wednesday");
        assert_eq!(day(NOW - 9 * DAY, NOW, UTC), "Wednesday 7 October");
        assert_eq!(day(NOW - 400 * DAY, NOW, UTC), "11 September 2025");
        assert_eq!(when(NOW - 60_000, NOW, UTC), "14:01");
        assert_eq!(when(NOW - DAY, NOW, UTC), "yesterday");
        assert_eq!(when(NOW - 5 * DAY, NOW, UTC), "Sun");
        assert_eq!(full(NOW, UTC), "Friday 16 October 2026, 14:02:00");
    }

    #[test]
    fn counts_and_widths() {
        assert_eq!(num(1204), "1,204");
        assert_eq!(num(48_210_000), "48,210,000");
        assert_eq!(num(12), "12");
        assert_eq!(fit("did you get the photos", 10), "did you g…");
        assert_eq!(fit("short", 10), "short");
        assert_eq!(fit("Mãe ❤", 10), "Mãe ❤");
    }
}

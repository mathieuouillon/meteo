//! Just enough calendar maths for weekday labels, without a date crate.

const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// Day of the week for an ISO date like "2026-11-12", 0 = Sunday.
pub fn weekday_index(iso_date: &str) -> Option<usize> {
    let mut parts = iso_date.get(..10)?.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, d) = (parts.next()??, parts.next()??, parts.next()??);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // 1970-01-01 was a Thursday.
    Some((days_from_civil(y, m, d) + 4).rem_euclid(7) as usize)
}

/// Short English weekday name for an ISO date like "2026-11-12" (→ "Thu").
/// (Screens use `Lang::weekday`; this is for the parsed data.)
pub fn weekday_short(iso_date: &str) -> Option<&'static str> {
    weekday_index(iso_date).map(|i| WEEKDAYS[i])
}

/// Local time as "YYYY-MM-DDTHH:MM" (Open-Meteo's format) for a Unix
/// timestamp and a UTC offset in seconds.
pub fn local_iso_minute(unix_secs: i64, utc_offset_seconds: i64) -> String {
    let t = unix_secs + utc_offset_seconds;
    let (days, secs) = (t.div_euclid(86_400), t.rem_euclid(86_400));
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}", secs / 3600, secs % 3600 / 60)
}

/// Unix seconds from an RFC 3339 time, "2026-11-13T23:00:00+00:00" (or
/// "…Z", or without seconds).
pub fn parse_rfc3339(s: &str) -> Option<i64> {
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, m, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (hh, mm) = (num(11..13)?, num(14..16)?);
    let (ss, rest) = if s.get(16..17) == Some(":") { (num(17..19)?, s.get(19..)?) } else { (0, s.get(16..)?) };
    // Skip fractional seconds.
    let rest = rest.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    let offset = match rest {
        "" | "Z" | "z" => 0,
        _ => {
            let sign = if rest.starts_with('-') { -1 } else { 1 };
            let h: i64 = rest.get(1..3)?.parse().ok()?;
            let m: i64 = rest.get(4..6)?.parse().ok()?;
            sign * (h * 3600 + m * 60)
        }
    };
    Some(days_from_civil(y, m, d) * 86_400 + hh * 3600 + mm * 60 + ss - offset)
}

/// Inverse of [`days_from_civil`] (Howard Hinnant's `civil_from_days`).
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// Days since 1970-01-01 in the proleptic Gregorian calendar
/// (Howard Hinnant's `days_from_civil`).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12; // March = 0
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_weekdays() {
        assert_eq!(weekday_short("1970-01-01"), Some("Thu"));
        assert_eq!(weekday_short("2000-02-29"), Some("Tue"));
        assert_eq!(weekday_short("2026-11-12"), Some("Thu"));
        assert_eq!(weekday_short("2027-01-01"), Some("Fri"));
        assert_eq!(weekday_short("2026-13-01"), None);
        assert_eq!(weekday_short("garbage"), None);
    }

    #[test]
    fn formats_local_time() {
        assert_eq!(local_iso_minute(0, 0), "1970-01-01T00:00");
        // 2026-11-12 13:15 UTC, in Paris (UTC+1) and New York (UTC-5).
        assert_eq!(local_iso_minute(1_794_489_300, 3600), "2026-11-12T14:15");
        assert_eq!(local_iso_minute(1_794_489_300, -18_000), "2026-11-12T08:15");
        // Leap day, and crossing midnight backwards.
        assert_eq!(local_iso_minute(951_782_400, 0), "2000-02-29T00:00");
        assert_eq!(local_iso_minute(951_782_400, -60), "2000-02-28T23:59");
    }
}

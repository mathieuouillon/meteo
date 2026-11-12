//! Just enough calendar maths for weekday labels, without a date crate.

const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// Short weekday name for an ISO date like "2026-11-12" (→ "Thu").
pub fn weekday_short(iso_date: &str) -> Option<&'static str> {
    let mut parts = iso_date.get(..10)?.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, d) = (parts.next()??, parts.next()??, parts.next()??);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // 1970-01-01 was a Thursday.
    let weekday = (days_from_civil(y, m, d) + 4).rem_euclid(7);
    Some(WEEKDAYS[weekday as usize])
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
}

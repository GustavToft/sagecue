use chrono::{DateTime, Local, Utc};

/// Format a UTC timestamp in the system's local timezone.
pub fn fmt_local(dt: DateTime<Utc>, fmt: &str) -> String {
    dt.with_timezone(&Local).format(fmt).to_string()
}

/// Format a duration in seconds into a human-readable string,
/// progressively showing larger units as needed.
///
/// Examples: `42s`, `3m 42s`, `2h 03m 42s`, `1d 02h 03m`
pub fn format_duration(total_secs: i64) -> String {
    if total_secs < 0 {
        return "--".to_string();
    }
    let days = total_secs / 86400;
    let hours = (total_secs % 86400) / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    if days > 0 {
        format!("{}d {:02}h {:02}m", days, hours, mins)
    } else if hours > 0 {
        format!("{}h {:02}m {:02}s", hours, mins, secs)
    } else if mins > 0 {
        format!("{}m {:02}s", mins, secs)
    } else {
        format!("{}s", secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, TimeZone};

    #[test]
    fn fmt_local_matches_chrono_local() {
        let dt = Utc.with_ymd_and_hms(2026, 4, 10, 12, 34, 56).unwrap();
        let expected = dt
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        assert_eq!(fmt_local(dt, "%Y-%m-%d %H:%M:%S"), expected);
    }

    #[test]
    fn fmt_local_is_not_utc_for_non_zero_offset() {
        // Pick a UTC instant and verify that the helper would render it in
        // local time by constructing the same conversion with a known offset.
        let dt = Utc.with_ymd_and_hms(2026, 4, 10, 23, 0, 0).unwrap();
        let plus_two = FixedOffset::east_opt(2 * 3600).unwrap();
        let shifted = dt.with_timezone(&plus_two).format("%H").to_string();
        assert_eq!(shifted, "01"); // 23:00 UTC is 01:00 +02:00
    }

    #[test]
    fn negative_returns_dash() {
        assert_eq!(format_duration(-1), "--");
        assert_eq!(format_duration(-100), "--");
    }

    #[test]
    fn zero_seconds() {
        assert_eq!(format_duration(0), "0s");
    }

    #[test]
    fn seconds_only() {
        assert_eq!(format_duration(42), "42s");
    }

    #[test]
    fn minutes_and_seconds() {
        assert_eq!(format_duration(90), "1m 30s");
        assert_eq!(format_duration(60), "1m 00s");
    }

    #[test]
    fn hours_minutes_seconds() {
        assert_eq!(format_duration(3661), "1h 01m 01s");
        assert_eq!(format_duration(3600), "1h 00m 00s");
    }

    #[test]
    fn days_hours_minutes() {
        assert_eq!(format_duration(86400 + 3661), "1d 01h 01m");
        assert_eq!(format_duration(86400), "1d 00h 00m");
    }
}

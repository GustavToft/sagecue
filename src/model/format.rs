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

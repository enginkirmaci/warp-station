//! Compact relative age labels ("just now", "5m ago", "2h ago", …): the most
//! significant unit only, no rounding across unit boundaries. Wall-clock
//! difference, so the label ignores the timestamp's UTC offset; future
//! timestamps (clock skew) clamp to "just now".

use chrono::{DateTime, Local};

pub fn format(at: DateTime<Local>) -> String {
    let span = Local::now() - at;
    let minutes = span.num_minutes().max(0);
    if minutes < 1 {
        "just now".into()
    } else if minutes < 60 {
        format!("{minutes}m ago")
    } else if minutes < 60 * 24 {
        format!("{}h ago", minutes / 60)
    } else if minutes < 60 * 24 * 7 {
        format!("{}d ago", minutes / (60 * 24))
    } else if minutes < 60 * 24 * 30 {
        format!("{}w ago", minutes / (60 * 24 * 7))
    } else if minutes < 60 * 24 * 365 {
        format!("{}mo ago", minutes / (60 * 24 * 30))
    } else {
        format!("{}y ago", minutes / (60 * 24 * 365))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn boundaries() {
        let now = Local::now();
        assert_eq!(format(now), "just now");
        assert_eq!(format(now - Duration::minutes(5)), "5m ago");
        assert_eq!(format(now - Duration::minutes(59)), "59m ago");
        assert_eq!(format(now - Duration::hours(2)), "2h ago");
        assert_eq!(format(now - Duration::hours(23)), "23h ago");
        assert_eq!(format(now - Duration::days(3)), "3d ago");
        assert_eq!(format(now - Duration::days(20)), "2w ago");
        assert_eq!(format(now - Duration::days(60)), "2mo ago");
        assert_eq!(format(now - Duration::days(400)), "1y ago");
        // future clamps
        assert_eq!(format(now + Duration::minutes(5)), "just now");
    }
}

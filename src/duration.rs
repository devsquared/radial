use anyhow::{Result, anyhow};
use jiff::SignedDuration;

/// Parse a human-readable duration string like `1h`, `30m`, `2h30m`, `45s`.
///
/// Supported units: `h` (hours), `m` (minutes), `s` (seconds).
/// Multiple units can be combined: `2h30m`, `1h15m30s`.
pub fn parse_duration(input: &str) -> Result<SignedDuration> {
    if input.is_empty() {
        return Err(anyhow!("Duration string cannot be empty"));
    }

    let mut total_secs: i64 = 0;
    let mut current_num = String::new();
    let mut found_any = false;

    for ch in input.chars() {
        if ch.is_ascii_digit() {
            current_num.push(ch);
        } else {
            if current_num.is_empty() {
                return Err(anyhow!(
                    "Expected a number before '{ch}' in duration '{input}'"
                ));
            }
            let n: i64 = current_num
                .parse()
                .map_err(|_| anyhow!("Invalid number in duration '{input}'"))?;
            current_num.clear();

            match ch {
                'h' => total_secs += n * 3600,
                'm' => total_secs += n * 60,
                's' => total_secs += n,
                _ => {
                    return Err(anyhow!(
                        "Unknown duration unit '{ch}' in '{input}'. Use h, m, or s."
                    ));
                }
            }
            found_any = true;
        }
    }

    if !current_num.is_empty() {
        return Err(anyhow!(
            "Trailing number without unit in duration '{input}'. Use h, m, or s."
        ));
    }

    if !found_any {
        return Err(anyhow!("No valid duration found in '{input}'"));
    }

    Ok(SignedDuration::from_secs(total_secs))
}

/// Format a `SignedDuration` as a human-readable string like `2h 14m`.
pub fn format_duration(d: &SignedDuration) -> String {
    let total_secs = d.as_secs().unsigned_abs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;

    match (hours, minutes) {
        (0, 0) => format!("{total_secs}s"),
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hours() {
        let d = parse_duration("1h").unwrap();
        assert_eq!(d.as_secs(), 3600);
    }

    #[test]
    fn parse_minutes() {
        let d = parse_duration("30m").unwrap();
        assert_eq!(d.as_secs(), 1800);
    }

    #[test]
    fn parse_seconds() {
        let d = parse_duration("45s").unwrap();
        assert_eq!(d.as_secs(), 45);
    }

    #[test]
    fn parse_combined() {
        let d = parse_duration("2h30m").unwrap();
        assert_eq!(d.as_secs(), 9000);
    }

    #[test]
    fn parse_all_units() {
        let d = parse_duration("1h15m30s").unwrap();
        assert_eq!(d.as_secs(), 4530);
    }

    #[test]
    fn parse_zero() {
        let d = parse_duration("0s").unwrap();
        assert_eq!(d.as_secs(), 0);
    }

    #[test]
    fn parse_empty_fails() {
        assert!(parse_duration("").is_err());
    }

    #[test]
    fn parse_no_unit_fails() {
        assert!(parse_duration("30").is_err());
    }

    #[test]
    fn parse_invalid_unit_fails() {
        assert!(parse_duration("5d").is_err());
    }

    #[test]
    fn parse_no_number_fails() {
        assert!(parse_duration("h").is_err());
    }

    #[test]
    fn format_hours_and_minutes() {
        let d = SignedDuration::from_secs(8040); // 2h 14m
        assert_eq!(format_duration(&d), "2h 14m");
    }

    #[test]
    fn format_just_minutes() {
        let d = SignedDuration::from_secs(300);
        assert_eq!(format_duration(&d), "5m");
    }

    #[test]
    fn format_just_hours() {
        let d = SignedDuration::from_secs(7200);
        assert_eq!(format_duration(&d), "2h");
    }

    #[test]
    fn format_seconds_only() {
        let d = SignedDuration::from_secs(42);
        assert_eq!(format_duration(&d), "42s");
    }
}

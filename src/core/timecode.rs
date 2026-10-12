//! `HH:MM:SS` trim times.

/// Seconds of a valid `HH:MM:SS` string, 0 for anything else.
pub fn to_seconds(s: &str) -> u64 {
    parse(s).unwrap_or(0)
}

fn parse(s: &str) -> Option<u64> {
    let parts: Vec<u64> = s.split(':').map(|p| p.trim().parse().ok()).collect::<Option<_>>()?;
    let [h, m, sec] = parts[..] else { return None };
    Some(h * 3600 + m * 60 + sec)
}

/// Whole seconds as `HH:MM:SS` (fractions are truncated).
pub fn format(seconds: f64) -> String {
    let t = seconds.max(0.0) as u64;
    format!("{:02}:{:02}:{:02}", t / 3600, t % 3600 / 60, t % 60)
}

fn is_valid(s: &str) -> bool {
    let parts: Vec<&str> = s.split(':').collect();
    let [h, m, sec] = parts[..] else { return false };
    matches!(
        (h.trim().parse::<i64>(), m.trim().parse::<i64>(), sec.trim().parse::<i64>()),
        (Ok(0..24), Ok(0..60), Ok(0..60))
    )
}

/// Checks one trim field against the duration and the other field. Values without a
/// colon are left alone. Returns the error message to show.
pub fn validate(value: &str, other: &str, is_start: bool, duration: Option<f64>) -> Result<(), String> {
    if !value.contains(':') {
        return Ok(());
    }
    if !is_valid(value) {
        return Err("Time format should be HH:MM:SS".into());
    }
    let t = to_seconds(value);
    if let Some(d) = duration.filter(|d| *d > 0.0)
        && t as f64 > d
    {
        return Err(format!("Time exceeds video duration ({})", format(d)));
    }
    if other.contains(':') && is_valid(other) {
        let o = to_seconds(other);
        if is_start && t >= o {
            return Err("Start time must be before end time".into());
        }
        if !is_start && t <= o {
            return Err("End time must be after start time".into());
        }
    }
    Ok(())
}

/// Drops `-ss X`, `-to X` and `-c copy`.
pub fn strip_trim_options(options: &str) -> String {
    let words: Vec<&str> = options.split_whitespace().collect();
    let mut out = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        let pair = words.get(i + 1);
        match (words[i], pair) {
            ("-ss" | "-to", Some(_)) | ("-c", Some(&"copy")) => i += 2,
            (w, _) => {
                out.push(w);
                i += 1;
            }
        }
    }
    out.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions() {
        assert_eq!(to_seconds("01:02:03"), 3723);
        assert_eq!(to_seconds("bad"), 0);
        assert_eq!(format(3723.9), "01:02:03");
    }

    #[test]
    fn validation_messages() {
        assert!(validate("", "00:00:10", true, None).is_ok());
        assert_eq!(validate("00:61:00", "", true, None).unwrap_err(), "Time format should be HH:MM:SS");
        assert_eq!(
            validate("00:02:00", "00:00:00", false, Some(90.0)).unwrap_err(),
            "Time exceeds video duration (00:01:30)"
        );
        assert_eq!(
            validate("00:00:10", "00:00:05", true, None).unwrap_err(),
            "Start time must be before end time"
        );
        assert!(validate("00:00:10", "00:00:05", false, None).is_ok());
    }

    #[test]
    fn strips_trim_options() {
        assert_eq!(strip_trim_options("-ac 2 -ss 00:00:01 -to 00:00:05 -c copy -y"), "-ac 2 -y");
    }
}

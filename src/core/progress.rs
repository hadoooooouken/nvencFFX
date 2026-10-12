//! Progress from ffmpeg's status lines.

/// ffmpeg's time duration syntax: `[-][HH:]MM:SS[.m...]` or `[-]S+[.m...][s|ms|us]`.
pub fn parse_time(s: &str) -> Option<f64> {
    let s = s.trim();
    let (sign, body) = match s.strip_prefix('-') {
        Some(rest) => (-1.0, rest),
        None => (1.0, s),
    };
    if !body.starts_with(|c: char| c.is_ascii_digit() || c == '.') {
        return None;
    }
    let seconds = if body.contains(':') {
        let parts: Vec<&str> = body.split(':').collect();
        let (h, m, sec) = match parts[..] {
            [m, sec] => ("0", m, sec),
            [h, m, sec] => (h, m, sec),
            _ => return None,
        };
        h.parse::<u64>().ok()? as f64 * 3600.0 + m.parse::<u64>().ok()? as f64 * 60.0 + sec.parse::<f64>().ok()?
    } else if let Some(ms) = body.strip_suffix("ms") {
        ms.parse::<f64>().ok()? / 1e3
    } else if let Some(us) = body.strip_suffix("us") {
        us.parse::<f64>().ok()? / 1e6
    } else {
        body.strip_suffix('s').unwrap_or(body).parse::<f64>().ok()?
    };
    Some(sign * seconds).filter(|t| t.is_finite())
}

/// The value of `key=` in a status line such as
/// `frame=  240 fps= 60 q=28.0 size=1024KiB time=00:00:08.00 bitrate=1048.6kbits/s speed=2x`.
fn value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let pattern = format!("{key}=");
    let mut from = 0;
    while let Some(i) = line[from..].find(&pattern) {
        let at = from + i;
        if at == 0 || line[..at].ends_with(char::is_whitespace) {
            return line[at + pattern.len()..].split_whitespace().next();
        }
        from = at + pattern.len();
    }
    None
}

/// Output position in seconds from `time=`; `None` for other lines and `time=N/A`.
pub fn time(line: &str) -> Option<f64> {
    value(line, "time").and_then(parse_time)
}

/// The value following the first `name` argument, as a time.
fn time_arg(args: &[String], name: &str) -> Option<f64> {
    args.windows(2).find(|w| w[0] == name).and_then(|w| parse_time(&w[1]))
}

/// Length of the output the `time=` values count towards: the `-ss`/`-to`/`-t` range
/// of the command, capped by the input duration, so trimmed conversions reach 100%.
pub fn expected_duration(args: &[String], input: Option<f64>) -> Option<f64> {
    let start = time_arg(args, "-ss").unwrap_or(0.0).max(0.0);
    let end = match (time_arg(args, "-t"), time_arg(args, "-to")) {
        (Some(t), _) => start + t,
        (None, Some(to)) => to,
        (None, None) => input?,
    };
    let end = input.map_or(end, |d| end.min(d));
    Some(end - start).filter(|d| *d > 0.0)
}

/// `time` as a fraction of `total`, clamped to 0..=1.
pub fn fraction(time: f64, total: f64) -> f32 {
    (time / total).clamp(0.0, 1.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn parses_ffmpeg_durations() {
        assert_eq!(parse_time("00:01:02.50"), Some(62.5));
        assert_eq!(parse_time("01:30"), Some(90.0));
        assert_eq!(parse_time("-00:00:00.02"), Some(-0.02));
        assert_eq!(parse_time("12.5"), Some(12.5));
        assert_eq!(parse_time("1500ms"), Some(1.5));
        assert_eq!(parse_time("3s"), Some(3.0));
        assert_eq!(parse_time("N/A"), None);
        assert_eq!(parse_time("inf"), None);
        assert_eq!(parse_time("1:2:3:4"), None);
    }

    #[test]
    fn reads_time_from_status_lines() {
        let line = "frame=  240 fps= 60 q=28.0 size=    1024KiB time=00:00:08.00 bitrate=1048.6kbits/s speed=2.0x";
        assert_eq!(time(line), Some(8.0));
        assert_eq!(value(line, "fps"), Some("60"));
        assert_eq!(value(line, "speed"), Some("2.0x"));
        assert_eq!(time("size=N/A time=N/A bitrate=N/A"), None);
        assert_eq!(time("Stream #0:0: Video: h264, runtime=5"), None);
        assert_eq!(time("frame=1 elapsed=0:00:01.00 time=00:00:02.00"), Some(2.0));
    }

    #[test]
    fn expected_duration_follows_the_trim_range() {
        assert_eq!(expected_duration(&args("ffmpeg -i a.mp4 o.mp4"), Some(100.0)), Some(100.0));
        assert_eq!(expected_duration(&args("ffmpeg -i a.mp4 o.mp4"), None), None);
        let trimmed = args("ffmpeg -ss 00:00:05 -to 00:00:20 -i a.mp4 o.mp4");
        assert_eq!(expected_duration(&trimmed, Some(100.0)), Some(15.0));
        assert_eq!(expected_duration(&trimmed, None), Some(15.0));
        assert_eq!(expected_duration(&args("ffmpeg -ss 90 -i a.mp4 -t 30 o.mp4"), Some(100.0)), Some(10.0));
        assert_eq!(expected_duration(&args("ffmpeg -ss 200 -i a.mp4 o.mp4"), Some(100.0)), None);
        assert_eq!(fraction(15.0, 10.0), 1.0);
    }
}

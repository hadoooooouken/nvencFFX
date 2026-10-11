//! Stream selection of the Streams window (`_create_stream_map_window`): the
//! streams listed by `ffprobe -hide_banner` and the `-map` options written for them.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stream {
    /// `N` of `Stream #0:N`
    pub index: usize,
    /// "Stream #N<extra>: <description>"
    pub text: String,
    /// `0:v:0`, `0:a:1`, ...: the position among the streams of its type
    pub map_code: String,
}

/// The streams by section; video streams that are cover art go to `pictures`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Streams {
    pub video: Vec<Stream>,
    pub audio: Vec<Stream>,
    pub subtitle: Vec<Stream>,
    pub pictures: Vec<Stream>,
}

impl Streams {
    /// The sections in the window's order (`add_stream_section`).
    pub fn sections(&self) -> [(&'static str, &[Stream]); 4] {
        [
            ("Video", &self.video),
            ("Audio", &self.audio),
            ("Subtitle", &self.subtitle),
            ("Pictures", &self.pictures),
        ]
    }

    /// Every stream in the window's order.
    pub fn all(&self) -> impl Iterator<Item = &Stream> {
        self.video.iter().chain(&self.audio).chain(&self.subtitle).chain(&self.pictures)
    }

    pub fn is_empty(&self) -> bool {
        self.all().next().is_none()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Type {
    Video,
    Audio,
    Subtitle,
}

/// `fetch_and_populate`: the `Stream #0:N...: Video|Audio|Subtitle: ...` lines;
/// data and attachment streams are left out.
pub fn parse(output: &str) -> Streams {
    let mut streams = Streams::default();
    let (mut v, mut a, mut s) = (0, 0, 0);
    for line in output.lines() {
        let Some((index, extra, kind, desc)) = parse_line(line) else { continue };
        let text = format!("Stream #{index}{extra}: {desc}");
        let (list, map_code) = match kind {
            Type::Video => {
                v += 1;
                let list = if desc.to_lowercase().contains("attached pic") { &mut streams.pictures } else { &mut streams.video };
                (list, format!("0:v:{}", v - 1))
            }
            Type::Audio => {
                a += 1;
                (&mut streams.audio, format!("0:a:{}", a - 1))
            }
            Type::Subtitle => {
                s += 1;
                (&mut streams.subtitle, format!("0:s:{}", s - 1))
            }
        };
        list.push(Stream { index, text, map_code });
    }
    streams
}

/// The original's case-insensitive
/// `Stream #0:(\d+)(.*?)\s*:\s*(Video|Audio|Subtitle)\s*:\s*(.*)`.
fn parse_line(line: &str) -> Option<(usize, &str, Type, &str)> {
    const PREFIX: &str = "stream #0:";
    let lower = line.to_ascii_lowercase();
    let mut from = 0;
    while let Some(found) = lower[from..].find(PREFIX) {
        let start = from + found + PREFIX.len();
        from = start;
        let digits = line[start..].bytes().take_while(u8::is_ascii_digit).count();
        let Ok(index) = line[start..start + digits].parse() else { continue };
        let rest = &line[start + digits..];
        // The first `:` that is followed by a stream type and another `:`
        for (colon, _) in rest.match_indices(':') {
            let after = rest[colon + 1..].trim_start();
            let Some((kind, len)) = [("video", Type::Video), ("audio", Type::Audio), ("subtitle", Type::Subtitle)]
                .into_iter()
                .find(|(name, _)| after.get(..name.len()).is_some_and(|w| w.eq_ignore_ascii_case(name)))
                .map(|(name, kind)| (kind, name.len()))
            else {
                continue;
            };
            if let Some(desc) = after[len..].trim_start().strip_prefix(':') {
                return Some((index, rest[..colon].trim(), kind, desc.trim()));
            }
        }
    }
    None
}

/// `apply_map`: the Add FF Options text with its `-map` / `-ignore_unknown` options
/// replaced by maps of the `selected` codes. With every stream selected (`all`) no
/// maps are written, so ffmpeg's default selection applies.
pub fn apply_map(options: &str, selected: &[&str], all: bool) -> String {
    let cleaned = remove_option(&remove_option(options, "-map", true), "-ignore_unknown", false);
    let cleaned = cleaned.trim();
    if all {
        return cleaned.to_owned();
    }
    let maps: Vec<String> = selected.iter().map(|code| format!("-map {code}")).collect();
    format!("{cleaned} -ignore_unknown {}", maps.join(" ")).trim().to_owned()
}

/// Python's `sub(r"-map\s+\S+", "", text)` (`with_value`) or
/// `sub(r"-ignore_unknown\s*", "", text)`.
fn remove_option(text: &str, option: &str, with_value: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(option) {
        out.push_str(&rest[..i]);
        let after = &rest[i + option.len()..];
        let value = after.trim_start();
        let gap = after.len() - value.len();
        if !with_value {
            rest = value;
        } else if gap > 0 && !value.is_empty() {
            rest = &value[value.find(char::is_whitespace).unwrap_or(value.len())..];
        } else {
            out.push_str(option);
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUTPUT: &str = "Input #0, matroska,webm, from 'a.mkv':
  Duration: 00:01:00.00, start: 0.000000, bitrate: 5000 kb/s
  Stream #0:0(eng): Video: hevc (Main 10), yuv420p10le(tv), 1920x1080, 60 fps (default)
  Stream #0:1[0x1](jpn): Audio: opus, 48000 Hz, 5.1, fltp (default)
  Stream #0:2(eng): AUDIO: aac (LC), 44100 Hz, stereo
  Stream #0:3(eng): Subtitle: ass (ssa)
  Stream #0:4: Video: mjpeg (Baseline), yuvj420p, 600x600, 90k tbr (attached pic)
  Stream #0:5: Attachment: ttf
  Stream #0:6: Data: bin_data (tmcd / 0x64636D74)";

    #[test]
    fn streams_are_grouped_with_their_map_codes() {
        let streams = parse(OUTPUT);
        let entries = |list: &[Stream]| list.iter().map(|s| (s.index, s.text.clone(), s.map_code.clone())).collect::<Vec<_>>();
        assert_eq!(entries(&streams.video), [(0, "Stream #0(eng): hevc (Main 10), yuv420p10le(tv), 1920x1080, 60 fps (default)".into(), "0:v:0".into())]);
        assert_eq!(
            entries(&streams.audio),
            [
                (1, "Stream #1[0x1](jpn): opus, 48000 Hz, 5.1, fltp (default)".into(), "0:a:0".into()),
                (2, "Stream #2(eng): aac (LC), 44100 Hz, stereo".into(), "0:a:1".into()),
            ]
        );
        assert_eq!(entries(&streams.subtitle), [(3, "Stream #3(eng): ass (ssa)".into(), "0:s:0".into())]);
        assert_eq!(streams.pictures[0].map_code, "0:v:1", "cover art counts as a video stream");
        let order: Vec<usize> = streams.all().map(|s| s.index).collect();
        assert_eq!(order, [0, 1, 2, 3, 4]);
        assert!(parse("No such file").is_empty());
    }

    #[test]
    fn maps_replace_the_previous_ones() {
        let options = "-ac 2 -ignore_unknown -map 0:v:0 -map 0:a:1 -map_metadata 0";
        assert_eq!(apply_map(options, &["0:v:0", "0:s:0"], false), "-ac 2 -map_metadata 0 -ignore_unknown -map 0:v:0 -map 0:s:0");
        assert_eq!(apply_map(options, &["0:v:0", "0:a:0"], true), "-ac 2 -map_metadata 0");
        assert_eq!(apply_map("-y -map 0:a:0 -ac 2", &[], true), "-y  -ac 2", "spaces left as the original's regex left them");
        assert_eq!(apply_map("", &["0:a:0"], false), "-ignore_unknown -map 0:a:0");
        assert_eq!(apply_map("-map", &[], true), "-map", "an option without a value stays");
    }
}

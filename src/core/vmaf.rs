//! VMAF analysis of the output against the input (`_run_vmaf_analysis`,
//! `_execute_vmaf`).

/// The analysis command and the label its score is shown with. The output is the
/// distorted video, the input the reference; both are cut to their first 30 s. An
/// output of another size is scaled to the input's size with `scale_flags`.
pub fn command(
    ffmpeg: &str,
    input: &str,
    output: &str,
    input_size: (u32, u32),
    output_size: (u32, u32),
    scale_flags: &str,
    threads: usize,
) -> (Vec<String>, &'static str) {
    let (filter, label) = if input_size == output_size {
        (format!("libvmaf=n_threads={threads}:n_subsample=10"), "VMAF score")
    } else {
        let (w, h) = input_size;
        (
            format!(
                "[0:v]scale={w}:{h}:flags={scale_flags}[dist];[1:v]null[ref];\
                 [dist][ref]libvmaf=n_threads={threads}:n_subsample=10"
            ),
            "VMAF score (with scaling)",
        )
    };
    let args = [ffmpeg, "-t", "30", "-i", output, "-t", "30", "-i", input, "-lavfi", &filter, "-f", "null", "-"];
    (args.map(str::to_owned).to_vec(), label)
}

/// The number of a `VMAF score: 93.4` line (`VMAF score:\s*([\d.]+)`).
pub fn score(line: &str) -> Option<&str> {
    let (_, rest) = line.split_once("VMAF score:")?;
    let rest = rest.trim_start();
    let len = rest.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(rest.len());
    (len > 0).then(|| &rest[..len])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaling_only_when_the_sizes_differ() {
        let (args, label) = command("ffmpeg", "in.mp4", "out.mp4", (1920, 1080), (1920, 1080), "lanczos", 8);
        assert_eq!(
            args.join(" "),
            "ffmpeg -t 30 -i out.mp4 -t 30 -i in.mp4 -lavfi libvmaf=n_threads=8:n_subsample=10 -f null -"
        );
        assert_eq!(label, "VMAF score");

        let (args, label) = command("ffmpeg", "in.mp4", "out.mp4", (1920, 1080), (1280, 720), "bicubic", 4);
        assert_eq!(
            args[10],
            "[0:v]scale=1920:1080:flags=bicubic[dist];[1:v]null[ref];[dist][ref]libvmaf=n_threads=4:n_subsample=10"
        );
        assert_eq!(label, "VMAF score (with scaling)");
    }

    #[test]
    fn score_is_read_from_the_summary_line() {
        assert_eq!(score("[Parsed_libvmaf_0 @ 0000021a] VMAF score: 95.123456"), Some("95.123456"));
        assert_eq!(score("VMAF score:100"), Some("100"));
        assert_eq!(score("VMAF score: n/a"), None);
        assert_eq!(score("frame=  100 fps=50"), None);
    }
}

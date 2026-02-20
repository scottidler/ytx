use crate::Transcript;

/// Render transcript as plain text (one segment per line, no timestamps)
pub fn render_text(transcript: &Transcript) -> String {
    transcript
        .segments
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render transcript as JSON with timestamps
pub fn render_json(transcript: &Transcript) -> String {
    serde_json::to_string_pretty(transcript).unwrap_or_default()
}

/// Render transcript as SRT subtitle format
pub fn render_srt(transcript: &Transcript) -> String {
    let mut output = String::new();
    for (i, seg) in transcript.segments.iter().enumerate() {
        let start = format_srt_time(seg.start);
        let end = format_srt_time(seg.start + seg.duration);
        output.push_str(&format!("{}\n{start} --> {end}\n{}\n\n", i + 1, seg.text));
    }
    output.truncate(output.trim_end().len());
    output
}

fn format_srt_time(seconds: f64) -> String {
    let total_ms = (seconds * 1000.0) as u64;
    let ms = total_ms % 1000;
    let total_secs = total_ms / 1000;
    let s = total_secs % 60;
    let total_mins = total_secs / 60;
    let m = total_mins % 60;
    let h = total_mins / 60;
    format!("{h:02}:{m:02}:{s:02},{ms:03}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Segment, TranscriptSource};

    fn sample_transcript() -> Transcript {
        Transcript {
            video_id: "test123".to_string(),
            title: "Test Video".to_string(),
            language: "en".to_string(),
            source: TranscriptSource::Caption,
            segments: vec![
                Segment {
                    text: "Hello world".to_string(),
                    start: 0.0,
                    duration: 1.5,
                },
                Segment {
                    text: "This is a test".to_string(),
                    start: 1.5,
                    duration: 2.0,
                },
            ],
        }
    }

    #[test]
    fn test_render_text() {
        let t = sample_transcript();
        let output = render_text(&t);
        assert_eq!(output, "Hello world\nThis is a test");
    }

    #[test]
    fn test_render_text_empty() {
        let t = Transcript {
            video_id: "empty".to_string(),
            title: "Empty".to_string(),
            language: "en".to_string(),
            source: TranscriptSource::Caption,
            segments: vec![],
        };
        assert_eq!(render_text(&t), "");
    }

    #[test]
    fn test_render_json() {
        let t = sample_transcript();
        let output = render_json(&t);
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["video_id"], "test123");
        assert_eq!(parsed["title"], "Test Video");
        assert_eq!(parsed["segments"].as_array().unwrap().len(), 2);
        assert_eq!(parsed["segments"][0]["text"], "Hello world");
        assert!((parsed["segments"][0]["start"].as_f64().unwrap() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_render_srt() {
        let t = sample_transcript();
        let output = render_srt(&t);
        let expected = "\
1
00:00:00,000 --> 00:00:01,500
Hello world

2
00:00:01,500 --> 00:00:03,500
This is a test";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_format_srt_time() {
        assert_eq!(format_srt_time(0.0), "00:00:00,000");
        assert_eq!(format_srt_time(1.5), "00:00:01,500");
        assert_eq!(format_srt_time(61.234), "00:01:01,234");
        assert_eq!(format_srt_time(3661.0), "01:01:01,000");
    }

    #[test]
    fn test_render_srt_empty() {
        let t = Transcript {
            video_id: "empty".to_string(),
            title: "Empty".to_string(),
            language: "en".to_string(),
            source: TranscriptSource::Caption,
            segments: vec![],
        };
        assert_eq!(render_srt(&t), "");
    }
}

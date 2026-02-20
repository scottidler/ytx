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
}

pub mod config;
pub mod output;
pub mod summarize;
pub mod whisper;
pub mod youtube;

use serde::Serialize;

/// A single captioned segment
#[derive(Debug, Clone, Serialize)]
pub struct Segment {
    pub text: String,
    pub start: f64,
    pub duration: f64,
}

/// Source of the transcript
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TranscriptSource {
    Caption,
    Whisper,
}

/// Complete transcript for a video
#[derive(Debug, Clone, Serialize)]
pub struct Transcript {
    pub video_id: String,
    pub title: String,
    pub language: String,
    pub source: TranscriptSource,
    pub segments: Vec<Segment>,
}

impl std::fmt::Display for TranscriptSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TranscriptSource::Caption => write!(f, "caption"),
            TranscriptSource::Whisper => write!(f, "whisper"),
        }
    }
}

/// Extract video ID from various YouTube URL formats
pub fn extract_video_id(input: &str) -> Option<String> {
    let input = input.trim();

    // Bare 11-character video ID
    if regex::Regex::new(r"^[a-zA-Z0-9_-]{11}$").unwrap().is_match(input) {
        return Some(input.to_string());
    }

    // youtube.com/watch?v=ID
    if let Some(caps) = regex::Regex::new(r"(?:youtube\.com/watch\?.*v=)([a-zA-Z0-9_-]{11})")
        .unwrap()
        .captures(input)
    {
        return Some(caps[1].to_string());
    }

    // youtu.be/ID
    if let Some(caps) = regex::Regex::new(r"youtu\.be/([a-zA-Z0-9_-]{11})")
        .unwrap()
        .captures(input)
    {
        return Some(caps[1].to_string());
    }

    // youtube.com/embed/ID
    if let Some(caps) = regex::Regex::new(r"youtube\.com/embed/([a-zA-Z0-9_-]{11})")
        .unwrap()
        .captures(input)
    {
        return Some(caps[1].to_string());
    }

    // youtube.com/shorts/ID
    if let Some(caps) = regex::Regex::new(r"youtube\.com/shorts/([a-zA-Z0-9_-]{11})")
        .unwrap()
        .captures(input)
    {
        return Some(caps[1].to_string());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bare_video_id() {
        assert_eq!(extract_video_id("dQw4w9WgXcQ"), Some("dQw4w9WgXcQ".to_string()));
    }

    #[test]
    fn test_watch_url() {
        assert_eq!(
            extract_video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ"),
            Some("dQw4w9WgXcQ".to_string())
        );
    }

    #[test]
    fn test_watch_url_with_extra_params() {
        assert_eq!(
            extract_video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=120"),
            Some("dQw4w9WgXcQ".to_string())
        );
    }

    #[test]
    fn test_short_url() {
        assert_eq!(
            extract_video_id("https://youtu.be/dQw4w9WgXcQ"),
            Some("dQw4w9WgXcQ".to_string())
        );
    }

    #[test]
    fn test_embed_url() {
        assert_eq!(
            extract_video_id("https://www.youtube.com/embed/dQw4w9WgXcQ"),
            Some("dQw4w9WgXcQ".to_string())
        );
    }

    #[test]
    fn test_shorts_url() {
        assert_eq!(
            extract_video_id("https://www.youtube.com/shorts/dQw4w9WgXcQ"),
            Some("dQw4w9WgXcQ".to_string())
        );
    }

    #[test]
    fn test_invalid_url() {
        assert_eq!(extract_video_id("not-a-valid-id"), None);
    }

    #[test]
    fn test_empty_input() {
        assert_eq!(extract_video_id(""), None);
    }

    #[test]
    fn test_whitespace_trimming() {
        assert_eq!(extract_video_id("  dQw4w9WgXcQ  "), Some("dQw4w9WgXcQ".to_string()));
    }
}

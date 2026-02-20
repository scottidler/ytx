use std::path::{Path, PathBuf};
use std::process::Command;

use eyre::{Result, bail};
use log::debug;
use reqwest::multipart;

use crate::{Segment, Transcript, TranscriptSource};

/// Maximum file size for a single Whisper API upload (25 MB)
const MAX_UPLOAD_BYTES: u64 = 25 * 1024 * 1024;

/// Whisper transcription model
#[derive(Debug, Clone, Default)]
pub enum WhisperModel {
    Gpt4oMiniTranscribe,
    Gpt4oTranscribe,
    #[default]
    Whisper1,
}

impl WhisperModel {
    fn api_name(&self) -> &str {
        match self {
            WhisperModel::Gpt4oMiniTranscribe => "gpt-4o-mini-transcribe",
            WhisperModel::Gpt4oTranscribe => "gpt-4o-transcribe",
            WhisperModel::Whisper1 => "whisper-1",
        }
    }

    fn response_format(&self) -> &str {
        match self {
            WhisperModel::Whisper1 => "verbose_json",
            // Newer transcribe models only support "json" or "text"
            _ => "json",
        }
    }

    fn supports_timestamp_granularities(&self) -> bool {
        matches!(self, WhisperModel::Whisper1)
    }
}

/// Transcribe a video using yt-dlp + Whisper API
pub async fn transcribe(
    client: &reqwest::Client,
    video_id: &str,
    lang: &str,
    model: &WhisperModel,
) -> Result<Transcript> {
    let api_key = std::env::var("OPENAI_API_KEY")
        .map_err(|_| eyre::eyre!("OPENAI_API_KEY environment variable not set (required for Whisper fallback)"))?;

    // Download audio via yt-dlp
    let audio_path = download_audio(video_id)?;

    // Get video title from yt-dlp
    let title = get_video_title(video_id).unwrap_or_default();

    // Check file size and chunk if needed
    let file_size = std::fs::metadata(&audio_path)?.len();
    debug!("Audio file size: {file_size} bytes");

    let segments = if file_size > MAX_UPLOAD_BYTES {
        transcribe_chunked(client, &api_key, &audio_path, model, lang).await?
    } else {
        transcribe_file(client, &api_key, &audio_path, model, lang).await?
    };

    Ok(Transcript {
        video_id: video_id.to_string(),
        title,
        language: lang.to_string(),
        source: TranscriptSource::Whisper,
        segments,
    })
}

fn download_audio(video_id: &str) -> Result<PathBuf> {
    let url = format!("https://www.youtube.com/watch?v={video_id}");
    let output_template = format!("/tmp/ytx-{video_id}.%(ext)s");
    let output_path = PathBuf::from(format!("/tmp/ytx-{video_id}.mp3"));

    // Reuse existing file on retry (avoid re-downloading after API errors)
    if output_path.exists() {
        debug!(
            "Audio file already exists, skipping download: {}",
            output_path.display()
        );
        return Ok(output_path);
    }

    debug!("Downloading audio via yt-dlp: {url}");

    let status = Command::new("yt-dlp")
        .args([
            "--extract-audio",
            "--audio-format",
            "mp3",
            "--audio-quality",
            "9", // lowest quality = smallest file (speech doesn't need high quality)
            "--no-playlist",
            "-o",
            &output_template,
            &url,
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .status();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => bail!("yt-dlp exited with status {s}"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            bail!(
                "yt-dlp not found. Install it to enable Whisper fallback:\n  \
                 pip install yt-dlp\n  \
                 or: brew install yt-dlp"
            );
        }
        Err(e) => bail!("failed to run yt-dlp: {e}"),
    }

    if !output_path.exists() {
        bail!("yt-dlp did not produce expected output file: {}", output_path.display());
    }

    Ok(output_path)
}

fn get_video_title(video_id: &str) -> Option<String> {
    let url = format!("https://www.youtube.com/watch?v={video_id}");
    Command::new("yt-dlp")
        .args(["--get-title", "--no-playlist", &url])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

async fn transcribe_file(
    client: &reqwest::Client,
    api_key: &str,
    audio_path: &Path,
    model: &WhisperModel,
    lang: &str,
) -> Result<Vec<Segment>> {
    debug!("Uploading {} to Whisper API", audio_path.display());

    let file_bytes = std::fs::read(audio_path)?;
    let file_name = audio_path.file_name().unwrap_or_default().to_string_lossy().to_string();

    let file_part = multipart::Part::bytes(file_bytes)
        .file_name(file_name)
        .mime_str("audio/mpeg")?;

    let mut form = multipart::Form::new()
        .part("file", file_part)
        .text("model", model.api_name().to_string())
        .text("language", lang.to_string())
        .text("response_format", model.response_format().to_string());

    if model.supports_timestamp_granularities() {
        form = form.text("timestamp_granularities[]", "segment");
    }

    let resp = client
        .post("https://api.openai.com/v1/audio/transcriptions")
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("Whisper API returned {status}: {body}");
    }

    let json: serde_json::Value = resp.json().await?;
    parse_whisper_response(&json)
}

fn parse_whisper_response(json: &serde_json::Value) -> Result<Vec<Segment>> {
    // verbose_json format has a "segments" array
    if let Some(segments) = json.get("segments").and_then(|s| s.as_array()) {
        return Ok(segments
            .iter()
            .filter_map(|seg| {
                let text = seg.get("text")?.as_str()?.trim().to_string();
                let start = seg.get("start")?.as_f64()?;
                let end = seg.get("end")?.as_f64()?;
                if text.is_empty() {
                    return None;
                }
                Some(Segment {
                    text,
                    start,
                    duration: end - start,
                })
            })
            .collect());
    }

    // Fallback: plain text response
    if let Some(text) = json.get("text").and_then(|t| t.as_str()) {
        return Ok(vec![Segment {
            text: text.trim().to_string(),
            start: 0.0,
            duration: 0.0,
        }]);
    }

    bail!("unexpected Whisper API response format");
}

async fn transcribe_chunked(
    client: &reqwest::Client,
    api_key: &str,
    audio_path: &Path,
    model: &WhisperModel,
    lang: &str,
) -> Result<Vec<Segment>> {
    // Split audio into chunks using yt-dlp's time ranges
    // Each chunk is ~20 minutes to stay under 25MB at 64kbps
    let chunk_duration_secs = 1200; // 20 minutes
    let file_size = std::fs::metadata(audio_path)?.len();
    let estimated_duration = file_size as f64 / (64_000.0 / 8.0); // 64kbps
    let num_chunks = (estimated_duration / chunk_duration_secs as f64).ceil() as usize;

    debug!("Splitting into {num_chunks} chunks of {chunk_duration_secs}s each");

    let mut all_segments = Vec::new();
    let mut time_offset = 0.0_f64;

    for i in 0..num_chunks {
        let start_time = i as f64 * chunk_duration_secs as f64;
        let chunk_path = PathBuf::from(format!("/tmp/ytx-chunk-{i}.mp3"));

        // Use ffmpeg to split (yt-dlp doesn't support time ranges on local files)
        let status = Command::new("ffmpeg")
            .args([
                "-y",
                "-i",
                &audio_path.to_string_lossy(),
                "-ss",
                &format!("{start_time}"),
                "-t",
                &format!("{chunk_duration_secs}"),
                "-acodec",
                "copy",
                &chunk_path.to_string_lossy(),
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()?;

        if !status.success() {
            bail!("ffmpeg failed to split audio at offset {start_time}s");
        }

        let mut segments = transcribe_file(client, api_key, &chunk_path, model, lang).await?;

        // Adjust timestamps for the offset
        for seg in &mut segments {
            seg.start += time_offset;
        }

        time_offset = start_time + chunk_duration_secs as f64;
        all_segments.extend(segments);

        // Clean up chunk
        let _ = std::fs::remove_file(&chunk_path);
    }

    Ok(all_segments)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_whisper_response_verbose_json() {
        let json = serde_json::json!({
            "text": "Hello world. This is a test.",
            "segments": [
                {
                    "id": 0,
                    "start": 0.0,
                    "end": 1.5,
                    "text": " Hello world."
                },
                {
                    "id": 1,
                    "start": 1.5,
                    "end": 3.0,
                    "text": " This is a test."
                }
            ]
        });

        let segments = parse_whisper_response(&json).unwrap();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].text, "Hello world.");
        assert!((segments[0].start - 0.0).abs() < f64::EPSILON);
        assert!((segments[0].duration - 1.5).abs() < f64::EPSILON);
        assert_eq!(segments[1].text, "This is a test.");
    }

    #[test]
    fn test_parse_whisper_response_plain_text() {
        let json = serde_json::json!({
            "text": "Just plain text."
        });

        let segments = parse_whisper_response(&json).unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "Just plain text.");
    }

    #[test]
    fn test_parse_whisper_response_empty_segments() {
        let json = serde_json::json!({
            "text": "",
            "segments": [
                {
                    "id": 0,
                    "start": 0.0,
                    "end": 1.0,
                    "text": ""
                }
            ]
        });

        let segments = parse_whisper_response(&json).unwrap();
        assert!(segments.is_empty());
    }

    #[test]
    fn test_whisper_model_api_names() {
        assert_eq!(WhisperModel::Gpt4oMiniTranscribe.api_name(), "gpt-4o-mini-transcribe");
        assert_eq!(WhisperModel::Gpt4oTranscribe.api_name(), "gpt-4o-transcribe");
        assert_eq!(WhisperModel::Whisper1.api_name(), "whisper-1");
    }
}

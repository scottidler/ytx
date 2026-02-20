use std::path::PathBuf;

use eyre::Result;
use log::debug;

use crate::Transcript;

fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from(".cache"))
        .join("ytx")
        .join("transcripts")
}

fn cache_path(video_id: &str, lang: &str) -> PathBuf {
    cache_dir().join(format!("{video_id}-{lang}.json"))
}

/// Load a cached transcript, if available.
pub fn load(video_id: &str, lang: &str) -> Option<Transcript> {
    let path = cache_path(video_id, lang);
    let data = std::fs::read_to_string(&path).ok()?;
    let transcript: Transcript = serde_json::from_str(&data).ok()?;
    debug!("Cache hit: {}", path.display());
    Some(transcript)
}

/// Save a transcript to the cache.
pub fn save(transcript: &Transcript) -> Result<()> {
    let path = cache_path(&transcript.video_id, &transcript.language);
    std::fs::create_dir_all(path.parent().unwrap())?;
    let data = serde_json::to_string_pretty(transcript)?;
    std::fs::write(&path, data)?;
    debug!("Cached transcript: {}", path.display());
    Ok(())
}

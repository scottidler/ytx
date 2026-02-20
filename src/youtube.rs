use eyre::{Result, bail};
use log::debug;
use regex::Regex;
use serde::Deserialize;

use crate::{Segment, Transcript, TranscriptSource};

const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

#[derive(Debug, Deserialize)]
struct InnerTubePlayerResponse {
    captions: Option<CaptionsData>,
    #[serde(rename = "videoDetails")]
    video_details: Option<VideoDetails>,
}

#[derive(Debug, Deserialize)]
struct VideoDetails {
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CaptionsData {
    #[serde(rename = "playerCaptionsTracklistRenderer")]
    player_captions_tracklist_renderer: Option<CaptionTracklistRenderer>,
}

#[derive(Debug, Deserialize)]
struct CaptionTracklistRenderer {
    #[serde(rename = "captionTracks")]
    caption_tracks: Option<Vec<CaptionTrack>>,
}

#[derive(Debug, Deserialize)]
struct CaptionTrack {
    #[serde(rename = "baseUrl")]
    base_url: String,
    #[serde(rename = "languageCode")]
    language_code: String,
}

/// Fetch transcript from YouTube's built-in captions via the InnerTube API
pub async fn fetch_captions(client: &reqwest::Client, video_id: &str, lang: &str) -> Result<Transcript> {
    // Step 1: Fetch the watch page to get the InnerTube API key
    let watch_url = format!("https://www.youtube.com/watch?v={video_id}");
    debug!("Fetching watch page: {watch_url}");

    let page_html = client
        .get(&watch_url)
        .header("User-Agent", USER_AGENT)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    let api_key = extract_api_key(&page_html)?;
    debug!("Extracted InnerTube API key: {api_key}");

    // Step 2: Call InnerTube player endpoint
    let player_url = format!("https://www.youtube.com/youtubei/v1/player?key={api_key}&prettyPrint=false");

    let body = serde_json::json!({
        "context": {
            "client": {
                "hl": lang,
                "gl": "US",
                "clientName": "WEB",
                "clientVersion": "2.20241126.01.00"
            }
        },
        "videoId": video_id
    });

    let resp: InnerTubePlayerResponse = client
        .post(&player_url)
        .header("User-Agent", USER_AGENT)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let title = resp
        .video_details
        .as_ref()
        .and_then(|vd| vd.title.clone())
        .unwrap_or_default();

    let tracks = resp
        .captions
        .and_then(|c| c.player_captions_tracklist_renderer)
        .and_then(|r| r.caption_tracks)
        .unwrap_or_default();

    if tracks.is_empty() {
        bail!("no captions available for video {video_id}");
    }

    // Find the requested language track, or fall back to the first available
    let track = tracks
        .iter()
        .find(|t| t.language_code == lang)
        .or_else(|| tracks.first())
        .unwrap(); // safe: tracks is non-empty

    let actual_lang = track.language_code.clone();
    debug!("Using caption track: lang={actual_lang}");

    // Step 3: Fetch the caption XML
    let caption_xml = client
        .get(&track.base_url)
        .header("User-Agent", USER_AGENT)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    let segments = parse_caption_xml(&caption_xml)?;

    Ok(Transcript {
        video_id: video_id.to_string(),
        title,
        language: actual_lang,
        source: TranscriptSource::Caption,
        segments,
    })
}

fn extract_api_key(html: &str) -> Result<String> {
    let re = Regex::new(r#""INNERTUBE_API_KEY"\s*:\s*"([^"]+)""#)?;
    if let Some(caps) = re.captures(html) {
        return Ok(caps[1].to_string());
    }

    // Fallback: try the newer pattern
    let re2 = Regex::new(r#"innertubeApiKey\s*[=:]\s*"([^"]+)""#)?;
    if let Some(caps) = re2.captures(html) {
        return Ok(caps[1].to_string());
    }

    bail!("could not extract InnerTube API key from watch page");
}

fn parse_caption_xml(xml: &str) -> Result<Vec<Segment>> {
    use quick_xml::Reader;
    use quick_xml::events::Event;

    let mut reader = Reader::from_str(xml);
    let mut segments = Vec::new();
    let mut current_start: Option<f64> = None;
    let mut current_dur: Option<f64> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"text" => {
                let mut start = None;
                let mut dur = None;
                for attr in e.attributes().flatten() {
                    match attr.key.as_ref() {
                        b"start" => {
                            start = String::from_utf8_lossy(&attr.value).parse::<f64>().ok();
                        }
                        b"dur" => {
                            dur = String::from_utf8_lossy(&attr.value).parse::<f64>().ok();
                        }
                        _ => {}
                    }
                }
                current_start = start;
                current_dur = dur;
            }
            Ok(Event::Empty(_)) => {
                // Self-closing <text .../> with no content — skip
            }
            Ok(Event::Text(ref e)) => {
                if let (Some(start), Some(dur)) = (current_start.take(), current_dur.take()) {
                    let raw_text = e.unescape().unwrap_or_default().to_string();
                    let text = html_escape::decode_html_entities(&raw_text).to_string();
                    if !text.is_empty() {
                        segments.push(Segment {
                            text,
                            start,
                            duration: dur,
                        });
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => bail!("error parsing caption XML: {e}"),
            _ => {}
        }
    }

    Ok(segments)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_api_key() {
        let html = r#"var ytInitialPlayerResponse = {};"INNERTUBE_API_KEY":"AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8";"#;
        let key = extract_api_key(html).unwrap();
        assert_eq!(key, "AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8");
    }

    #[test]
    fn test_extract_api_key_fallback() {
        let html = r#"innertubeApiKey="AIzaSyB123";"#;
        let key = extract_api_key(html).unwrap();
        assert_eq!(key, "AIzaSyB123");
    }

    #[test]
    fn test_extract_api_key_missing() {
        let html = "<html><body>no key here</body></html>";
        assert!(extract_api_key(html).is_err());
    }

    #[test]
    fn test_parse_caption_xml_basic() {
        let xml = r#"<?xml version="1.0" encoding="utf-8" ?>
<transcript>
    <text start="0.21" dur="2.34">Hello world</text>
    <text start="2.55" dur="1.50">This is a test</text>
</transcript>"#;

        let segments = parse_caption_xml(xml).unwrap();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].text, "Hello world");
        assert!((segments[0].start - 0.21).abs() < f64::EPSILON);
        assert!((segments[0].duration - 2.34).abs() < f64::EPSILON);
        assert_eq!(segments[1].text, "This is a test");
    }

    #[test]
    fn test_parse_caption_xml_html_entities() {
        let xml = r#"<?xml version="1.0" encoding="utf-8" ?>
<transcript>
    <text start="0.0" dur="1.0">it&amp;#39;s a &amp;quot;test&amp;quot;</text>
</transcript>"#;

        let segments = parse_caption_xml(xml).unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "it's a \"test\"");
    }

    #[test]
    fn test_parse_caption_xml_empty() {
        let xml = r#"<?xml version="1.0" encoding="utf-8" ?><transcript></transcript>"#;
        let segments = parse_caption_xml(xml).unwrap();
        assert!(segments.is_empty());
    }
}

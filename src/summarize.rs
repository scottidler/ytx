use eyre::{Result, bail};
use log::debug;

use crate::Transcript;

const DEFAULT_SYSTEM_PROMPT: &str = "You are a helpful assistant that summarizes video transcripts. \
Provide a clear, structured summary that captures the key points, main arguments, and important details. \
Use bullet points for key takeaways.";

/// Summarize a transcript using an LLM
pub async fn summarize(client: &reqwest::Client, transcript: &Transcript, model: &str) -> Result<String> {
    let transcript_text = transcript
        .segments
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");

    if is_anthropic_model(model) {
        summarize_anthropic(client, &transcript_text, &transcript.title, model).await
    } else {
        summarize_openai(client, &transcript_text, &transcript.title, model).await
    }
}

fn is_anthropic_model(model: &str) -> bool {
    model.starts_with("claude")
}

async fn summarize_anthropic(
    client: &reqwest::Client,
    transcript_text: &str,
    title: &str,
    model: &str,
) -> Result<String> {
    let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
        eyre::eyre!("ANTHROPIC_API_KEY environment variable not set (required for Claude summarization)")
    })?;

    debug!("Summarizing via Anthropic API with model {model}");

    let user_message = format!("Summarize this transcript from the video \"{title}\":\n\n{transcript_text}");

    let body = serde_json::json!({
        "model": model,
        "max_tokens": 4096,
        "system": DEFAULT_SYSTEM_PROMPT,
        "messages": [
            {
                "role": "user",
                "content": user_message
            }
        ]
    });

    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("Anthropic API returned {status}: {body}");
    }

    let json: serde_json::Value = resp.json().await?;
    extract_anthropic_text(&json)
}

fn extract_anthropic_text(json: &serde_json::Value) -> Result<String> {
    if let Some(content) = json.get("content").and_then(|c| c.as_array()) {
        let text: String = content
            .iter()
            .filter_map(|block| {
                if block.get("type")?.as_str()? == "text" {
                    block.get("text")?.as_str().map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("");
        if !text.is_empty() {
            return Ok(text);
        }
    }
    bail!("unexpected Anthropic API response format");
}

async fn summarize_openai(client: &reqwest::Client, transcript_text: &str, title: &str, model: &str) -> Result<String> {
    let api_key = std::env::var("OPENAI_API_KEY")
        .map_err(|_| eyre::eyre!("OPENAI_API_KEY environment variable not set (required for OpenAI summarization)"))?;

    debug!("Summarizing via OpenAI API with model {model}");

    let user_message = format!("Summarize this transcript from the video \"{title}\":\n\n{transcript_text}");

    let body = serde_json::json!({
        "model": model,
        "messages": [
            {
                "role": "system",
                "content": DEFAULT_SYSTEM_PROMPT
            },
            {
                "role": "user",
                "content": user_message
            }
        ]
    });

    let resp = client
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(&api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("OpenAI API returned {status}: {body}");
    }

    let json: serde_json::Value = resp.json().await?;
    extract_openai_text(&json)
}

fn extract_openai_text(json: &serde_json::Value) -> Result<String> {
    if let Some(text) = json
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|t| t.as_str())
    {
        return Ok(text.to_string());
    }
    bail!("unexpected OpenAI API response format");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_anthropic_model() {
        assert!(is_anthropic_model("claude-sonnet-4-6"));
        assert!(is_anthropic_model("claude-3-opus-20240229"));
        assert!(!is_anthropic_model("gpt-4o"));
        assert!(!is_anthropic_model("gpt-4o-mini"));
    }

    #[test]
    fn test_extract_anthropic_text() {
        let json = serde_json::json!({
            "content": [
                {
                    "type": "text",
                    "text": "Here is the summary."
                }
            ]
        });
        assert_eq!(extract_anthropic_text(&json).unwrap(), "Here is the summary.");
    }

    #[test]
    fn test_extract_anthropic_text_empty() {
        let json = serde_json::json!({"content": []});
        assert!(extract_anthropic_text(&json).is_err());
    }

    #[test]
    fn test_extract_openai_text() {
        let json = serde_json::json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Summary of the video."
                    }
                }
            ]
        });
        assert_eq!(extract_openai_text(&json).unwrap(), "Summary of the video.");
    }

    #[test]
    fn test_extract_openai_text_empty() {
        let json = serde_json::json!({"choices": []});
        assert!(extract_openai_text(&json).is_err());
    }
}

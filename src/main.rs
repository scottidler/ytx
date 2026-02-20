use std::io::{self, BufRead};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use eyre::{Result, bail};
use log::{debug, info};

mod cli;

use cli::{Cli, OutputFormat};

fn setup_logging() -> Result<()> {
    let log_dir = log_dir();
    std::fs::create_dir_all(&log_dir)?;
    let log_file = log_dir.join("ytx.log");

    let target = Box::new(std::fs::OpenOptions::new().create(true).append(true).open(&log_file)?);

    env_logger::Builder::from_default_env()
        .target(env_logger::Target::Pipe(target))
        .init();

    info!("Logging initialized: {}", log_file.display());
    Ok(())
}

fn log_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ytx")
        .join("logs")
}

fn tool_version(name: &str) -> Option<String> {
    Command::new(name)
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim()
                .lines()
                .next()
                .unwrap_or("")
                .to_string()
        })
}

fn build_after_help() -> String {
    let yt_dlp = tool_version("yt-dlp");

    let yt_dlp_line = match &yt_dlp {
        Some(v) => format!("  \x1b[32m✅\x1b[0m yt-dlp     {v}"),
        None => "  \x1b[31m❌\x1b[0m yt-dlp     (not found — needed for Whisper fallback)".to_string(),
    };

    let log_path = log_dir().join("ytx.log");

    format!(
        "\nREQUIRED TOOLS:\n{yt_dlp_line}\n\nLogs are written to: {}",
        log_path.display()
    )
}

/// Retry an async operation with exponential backoff
async fn retry<F, Fut, T>(max_attempts: u32, operation: F) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut last_err = None;
    for attempt in 0..max_attempts {
        match operation().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                if attempt + 1 < max_attempts {
                    let delay = Duration::from_millis(500 * 2u64.pow(attempt));
                    debug!("Attempt {} failed: {e}, retrying in {delay:?}", attempt + 1);
                    tokio::time::sleep(delay).await;
                }
                last_err = Some(e);
            }
        }
    }
    Err(last_err.unwrap())
}

#[tokio::main]
async fn main() -> Result<()> {
    setup_logging()?;

    let after_help = build_after_help();
    let cmd = <Cli as clap::CommandFactory>::command().after_help(after_help);
    let matches = cmd.get_matches();
    let cli = <Cli as clap::FromArgMatches>::from_arg_matches(&matches)?;

    // Load config file (non-fatal if missing/invalid)
    let config = ytx::config::Config::load().unwrap_or_default();

    // Apply config defaults (CLI flags take priority)
    let lang = cli.lang.clone();
    let model = cli.model.clone();

    if cli.verbose {
        let config_path = ytx::config::config_path();
        if config_path.exists() {
            eprintln!("Config: {}", config_path.display());
        }
        if let Some(ref default_lang) = config.default_lang {
            debug!("Config default_lang: {default_lang}");
        }
        if let Some(ref default_model) = config.default_model {
            debug!("Config default_model: {default_model}");
        }
    }

    let client = reqwest::Client::new();

    // Collect URLs: from arg or stdin
    let urls = if let Some(ref url) = cli.url {
        vec![url.clone()]
    } else {
        let stdin = io::stdin();
        stdin.lock().lines().collect::<Result<Vec<_>, _>>()?
    };

    if urls.is_empty() {
        bail!("no URL or video ID provided\n\nUsage: ytx <URL>\n       echo <URL> | ytx");
    }

    for url_input in &urls {
        let url_input = url_input.trim().to_string();
        if url_input.is_empty() {
            continue;
        }

        let video_id = ytx::extract_video_id(&url_input)
            .ok_or_else(|| eyre::eyre!("could not extract video ID from: {url_input}\n\nSupported formats:\n  https://www.youtube.com/watch?v=ID\n  https://youtu.be/ID\n  https://www.youtube.com/embed/ID\n  https://www.youtube.com/shorts/ID\n  <11-character video ID>"))?;

        let whisper_model = ytx::whisper::WhisperModel::default();
        let lang = lang.clone();

        let transcript = if cli.whisper_only {
            retry(3, || {
                let client = &client;
                let video_id = &video_id;
                let lang = &lang;
                let model = &whisper_model;
                async move { ytx::whisper::transcribe(client, video_id, lang, model).await }
            })
            .await?
        } else {
            let caption_result = retry(3, || {
                let client = &client;
                let video_id = &video_id;
                let lang = &lang;
                async move { ytx::youtube::fetch_captions(client, video_id, lang).await }
            })
            .await;

            match caption_result {
                Ok(t) => t,
                Err(e) => {
                    if cli.no_fallback {
                        return Err(e.wrap_err("caption extraction failed and --no-fallback set"));
                    }
                    if cli.verbose {
                        eprintln!("Caption extraction failed: {e}");
                        eprintln!("Falling back to Whisper transcription...");
                    }
                    retry(3, || {
                        let client = &client;
                        let video_id = &video_id;
                        let lang = &lang;
                        let model = &whisper_model;
                        async move { ytx::whisper::transcribe(client, video_id, lang, model).await }
                    })
                    .await?
                }
            }
        };

        if cli.verbose {
            eprintln!(
                "Video: {} ({})\nSource: {}\nLanguage: {}\nSegments: {}",
                transcript.title,
                transcript.video_id,
                transcript.source,
                transcript.language,
                transcript.segments.len(),
            );
        }

        let rendered = match cli.format {
            OutputFormat::Text => ytx::output::render_text(&transcript),
            OutputFormat::Json => ytx::output::render_json(&transcript),
            OutputFormat::Srt => ytx::output::render_srt(&transcript),
        };

        if let Some(ref path) = cli.output {
            std::fs::write(path, &rendered)?;
            if cli.verbose {
                eprintln!("Output written to: {}", path.display());
            }
        } else {
            println!("{rendered}");
        }

        if cli.summarize {
            let summary = ytx::summarize::summarize(&client, &transcript, &model).await?;
            println!("\n--- Summary ---\n{summary}");
        }
    }

    Ok(())
}

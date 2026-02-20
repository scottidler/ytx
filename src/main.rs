use std::io::{self, BufRead};
use std::path::PathBuf;
use std::process::Command;

use eyre::{Result, bail};
use log::info;

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

#[tokio::main]
async fn main() -> Result<()> {
    setup_logging()?;

    let after_help = build_after_help();
    let cmd = <Cli as clap::CommandFactory>::command().after_help(after_help);
    let matches = cmd.get_matches();
    let cli = <Cli as clap::FromArgMatches>::from_arg_matches(&matches)?;

    let client = reqwest::Client::new();

    // Collect URLs: from arg or stdin
    let urls = if let Some(ref url) = cli.url {
        vec![url.clone()]
    } else {
        let stdin = io::stdin();
        stdin.lock().lines().collect::<Result<Vec<_>, _>>()?
    };

    if urls.is_empty() {
        bail!("no URL or video ID provided");
    }

    for url_input in &urls {
        let video_id = ytx::extract_video_id(url_input)
            .ok_or_else(|| eyre::eyre!("could not extract video ID from: {url_input}"))?;

        let whisper_model = ytx::whisper::WhisperModel::default();

        let transcript = if cli.whisper_only {
            ytx::whisper::transcribe(&client, &video_id, &cli.lang, &whisper_model).await?
        } else {
            match ytx::youtube::fetch_captions(&client, &video_id, &cli.lang).await {
                Ok(t) => t,
                Err(e) => {
                    if cli.no_fallback {
                        return Err(e.wrap_err("caption extraction failed and --no-fallback set"));
                    }
                    eprintln!("Caption extraction failed, falling back to Whisper: {e}");
                    ytx::whisper::transcribe(&client, &video_id, &cli.lang, &whisper_model).await?
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
            OutputFormat::Json => {
                bail!("JSON output not yet implemented (Phase 3)");
            }
            OutputFormat::Srt => {
                bail!("SRT output not yet implemented (Phase 3)");
            }
        };

        if let Some(ref path) = cli.output {
            std::fs::write(path, &rendered)?;
        } else {
            print!("{rendered}");
        }
    }

    Ok(())
}

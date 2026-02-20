use clap::Parser;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
    Srt,
}

#[derive(Parser)]
#[command(
    name = "ytx",
    about = "YouTube transcript extractor",
    version = env!("GIT_DESCRIBE"),
)]
pub struct Cli {
    /// YouTube video URL or video ID (reads from stdin if omitted)
    pub url: Option<String>,

    /// Summarize the transcript via LLM
    #[arg(short, long)]
    pub summarize: bool,

    /// Output format: text (default), json, srt
    #[arg(short, long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,

    /// Preferred caption language
    #[arg(short, long, default_value = "en")]
    pub lang: String,

    /// Write output to file instead of stdout
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Skip caption extraction, always use Whisper
    #[arg(long)]
    pub whisper_only: bool,

    /// Don't fall back to Whisper if captions unavailable
    #[arg(long)]
    pub no_fallback: bool,

    /// LLM model for summarization
    #[arg(long, default_value = "claude-sonnet-4-6")]
    pub model: String,

    /// Show extraction method and metadata
    #[arg(short, long)]
    pub verbose: bool,
}

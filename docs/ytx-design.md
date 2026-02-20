# Design Document: ytx — YouTube Transcript Extractor

**Author:** Scott Aidler
**Date:** 2026-02-19
**Status:** Ready for Review
**Review Passes Completed:** 5/5

## Summary

`ytx` is a Rust CLI that extracts transcripts from YouTube videos with 100% URL coverage. It first attempts to pull YouTube's built-in captions (free, fast), then falls back to audio download + Whisper API transcription for videos without captions. Optionally summarizes transcripts via LLM.

## Problem Statement

### Background

YouTube is a primary source of educational and technical content. Extracting transcripts from videos enables searchability, summarization, and integration into research workflows. Web-based tools like NoteGPT charge $9/month for what amounts to caption extraction + LLM summary — capabilities the user already has API access to.

### Problem

There is no single, reliable CLI tool that:
1. Extracts transcripts from **any** YouTube video (not just those with captions)
2. Produces clean plain text output suitable for piping into other tools
3. Runs as a single binary with no runtime dependencies (no Python, no venv)

### Goals

- Extract transcripts from 100% of valid YouTube video URLs
- Single static Rust binary — no runtime dependencies beyond `yt-dlp` (for fallback only)
- Clean plain text output by default, suitable for piping
- Optional LLM-powered summarization
- Replace the $9/month NoteGPT subscription

### Non-Goals

- Real-time live stream transcription
- Video download or media processing
- GUI or web interface
- Subtitle file format conversion tool (SRT/VTT editing)
- Multi-language translation (use YouTube's built-in translation via `--lang` flag)

## Proposed Solution

### Overview

A two-tier transcript extraction strategy:

```
YouTube URL
    |
    v
[Tier 1] InnerTube API caption extraction (free, fast, ~90% of videos)
    |
    | (no captions available)
    v
[Tier 2] yt-dlp audio download → OpenAI transcription API (~$0.003-0.006/min)
    |
    v
Plain text transcript
    |
    | (--summarize flag)
    v
[Optional] LLM summary via Claude or OpenAI API
```

### Architecture

```
ytx/
├── src/
│   ├── main.rs           # CLI entry point, clap args
│   ├── lib.rs            # Public API
│   ├── youtube.rs        # Tier 1: InnerTube caption extraction
│   ├── whisper.rs        # Tier 2: yt-dlp + Whisper API fallback
│   ├── summarize.rs      # Optional LLM summarization
│   ├── output.rs         # Output formatting (text, json, srt)
│   └── config.rs         # Configuration and API key management
├── Cargo.toml
└── README.md
```

**Key components:**

1. **YouTube caption fetcher** (`youtube.rs`): Three-step extraction:
   1. `GET /watch?v={id}` — fetch page HTML, extract `INNERTUBE_API_KEY` via regex
   2. `POST /youtubei/v1/player` — InnerTube API call with browser-like `User-Agent` (required; YouTube rejects non-browser UAs). Returns caption track metadata at `captions.playerCaptionsTracklistRenderer.captionTracks`
   3. `GET /api/timedtext?v={id}&lang={lang}` — fetch caption XML from the signed `baseUrl` in the response

   Parses XML `<text start="0.21" dur="2.34">...</text>` elements into structured transcript segments.

2. **Whisper fallback** (`whisper.rs`): Shells out to `yt-dlp` to download audio (64kbps MP3 for minimal file size), then uploads to OpenAI's transcription API. Supports three models:
   - `gpt-4o-mini-transcribe` — $0.003/min (default, cheapest)
   - `gpt-4o-transcribe` — $0.006/min (better accuracy with accents/noise)
   - `whisper-1` — $0.006/min (legacy)

   Handles chunking for files >25MB by splitting audio into segments.

3. **Summarizer** (`summarize.rs`): Sends transcript to Claude or OpenAI for structured summary. Configurable system prompt.

4. **Output formatter** (`output.rs`): Renders transcript as plain text (default), JSON with timestamps, or SRT.

### Data Model

```rust
/// A single captioned segment
struct Segment {
    text: String,
    start: f64,      // seconds
    duration: f64,   // seconds
}

/// Complete transcript for a video
struct Transcript {
    video_id: String,
    title: String,
    language: String,
    source: TranscriptSource, // Caption | Whisper
    segments: Vec<Segment>,
}

enum TranscriptSource {
    Caption,   // YouTube's built-in captions
    Whisper,   // Audio transcription via Whisper API
}

/// LLM-generated summary
struct Summary {
    text: String,
    model: String,
}
```

### API Design (CLI Interface)

```
ytx <URL> [OPTIONS]

Arguments:
  [URL]  YouTube video URL or video ID (reads from stdin if omitted)

Options:
  -s, --summarize          Summarize the transcript via LLM
  -f, --format <FORMAT>    Output format: text (default), json, srt
  -l, --lang <LANG>        Preferred caption language [default: en]
  -o, --output <FILE>      Write output to file instead of stdout
      --whisper-only       Skip caption extraction, always use Whisper
      --no-fallback        Don't fall back to Whisper if captions unavailable
      --model <MODEL>      LLM model for summarization [default: claude-sonnet-4-6]
  -v, --verbose            Show extraction method and metadata
  -h, --help               Print help
  -V, --version            Print version
```

**Environment variables:**
- `OPENAI_API_KEY` — Required for Whisper fallback and OpenAI summarization
- `ANTHROPIC_API_KEY` — Required for Claude summarization

**Examples:**
```bash
# Basic transcript to stdout
ytx https://www.youtube.com/watch?v=dQw4w9WgXcQ

# Transcript + summary
ytx https://youtu.be/dQw4w9WgXcQ --summarize

# JSON output with timestamps
ytx dQw4w9WgXcQ --format json

# Pipe to clipboard
ytx dQw4w9WgXcQ | xclip -selection clipboard

# Pipe to another LLM tool
ytx dQw4w9WgXcQ | fabric -p extract_wisdom

# Batch: process a list of URLs
cat urls.txt | ytx

# Batch: with parallel (one transcript per URL)
cat urls.txt | parallel ytx {} -o {}.txt
```

### Implementation Plan

#### Phase 1: Core caption extraction
- CLI skeleton with clap
- InnerTube API integration for caption discovery and fetching
- XML caption parsing
- Plain text output to stdout
- URL/video ID parsing — all formats:
  - `https://www.youtube.com/watch?v=ID`
  - `https://youtu.be/ID`
  - `https://www.youtube.com/embed/ID`
  - `https://www.youtube.com/shorts/ID`
  - Bare 11-character video ID

#### Phase 2: Whisper fallback
- Shell out to yt-dlp for audio download
- Whisper API multipart upload
- Audio chunking for videos >25MB (~50 min at 64kbps MP3; download at low bitrate since it's speech)
- Automatic fallback when captions unavailable

#### Phase 3: Output formats and summarization
- JSON output with timestamps
- SRT output
- LLM summarization via Claude and OpenAI APIs
- `--summarize` flag

#### Phase 4: Polish
- Error messages and diagnostics
- `--verbose` mode showing extraction method, video metadata
- Config file support (~/.config/ytx/config.toml)
- Retry logic for transient failures

## Alternatives Considered

### Alternative 1: Use existing `ytranscript` or `yt-transcript-rs` crate
- **Description:** Depend on an existing Rust crate for YouTube caption extraction instead of implementing InnerTube API calls directly.
- **Pros:** Less code to write and maintain. Already handles some edge cases.
- **Cons:** These crates are immature (low download counts, infrequent updates). YouTube regularly changes its internal APIs — depending on a third-party crate means waiting for upstream fixes. The InnerTube API interaction is simple enough (~100 lines) that owning it is low-cost.
- **Why not chosen:** The extraction logic is simple (one POST, one GET, XML parse). Owning it means faster response to YouTube API changes and no dependency on potentially abandoned crates.

### Alternative 2: Use yt-dlp for everything (captions and audio)
- **Description:** Shell out to yt-dlp for both caption extraction (`--write-auto-sub`) and audio download.
- **Pros:** yt-dlp is extremely well-maintained and handles every edge case (age restriction, geo-blocking, PO tokens). Single external dependency for all YouTube interaction.
- **Cons:** Requires yt-dlp installed on the system. Slower than direct API calls for caption extraction. Adds ~60MB external dependency. Output parsing is fragile (text/file-based).
- **Why not chosen:** Using yt-dlp as the primary path adds unnecessary latency and dependency weight for the 90% case where direct API calls work fine. However, yt-dlp is kept as the audio download mechanism for the Whisper fallback tier, where its robustness matters most.

### Alternative 3: Python script with `uv run`
- **Description:** Single-file Python script with PEP 723 inline dependencies, executed via `uv run`.
- **Pros:** Faster to write. Direct access to `youtube-transcript-api` (mature, well-maintained). Whisper via `openai` Python SDK.
- **Cons:** Requires Python + uv runtime. Not a standalone binary. Dependency resolution on each run (or cached). Doesn't align with user preference for Rust tooling.
- **Why not chosen:** User prefers Rust for CLI tools. The venv/runtime question is exactly what motivated this project.

### Alternative 4: Local Whisper (no API)
- **Description:** Run Whisper model locally instead of using OpenAI's API.
- **Pros:** No per-minute cost. No API key needed. Works offline.
- **Cons:** Requires downloading ~1.5GB model. Needs Python + PyTorch or a Rust binding (whisper.cpp via `whisper-rs`). Significantly slower without GPU. Much more complex build.
- **Why not chosen:** The API cost is negligible for typical usage (~$0.36/hour). Local Whisper adds massive complexity for minimal savings. Could be added as a future option.

## Technical Considerations

### Dependencies

**Rust crates:**
- `clap` — CLI argument parsing
- `reqwest` — HTTP client (with `rustls` for TLS, avoiding OpenSSL dependency)
- `serde` / `serde_json` — JSON serialization
- `quick-xml` — XML parsing for caption data
- `tokio` — Async runtime
- `anyhow` — Error handling

**External (runtime):**
- `yt-dlp` — Required only for Whisper fallback tier. Not needed if video has captions.

### Performance

- **Tier 1 (captions):** Three HTTP requests total (~300-700ms). Negligible.
- **Tier 2 (Whisper):** Dominated by audio download time + API processing. Expect 30-120 seconds for a 10-minute video.
- No local compute requirements beyond the HTTP client.

### Security

- API keys read from environment variables only (never stored in binary or config files in plaintext)
- No shell injection risk: yt-dlp invoked via `Command::new("yt-dlp")` with argument array, not string interpolation
- Temporary audio files cleaned up after Whisper upload
- HTTPS for all API calls (enforced by reqwest + rustls)

### Testing Strategy

- **Unit tests:** URL/video ID parsing, XML caption parsing, segment formatting
- **Integration tests:** Full pipeline against known stable YouTube videos (e.g., YouTube's own test videos)
- **Mock tests:** Mock HTTP responses for InnerTube and Whisper APIs to test error handling without network
- Note: YouTube's internal APIs are undocumented and can change. Integration tests may break due to upstream changes, not code bugs.

### Rollout Plan

1. Build and test locally
2. Install via `cargo install` from local path
3. Optionally publish to crates.io if useful to others

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| YouTube changes InnerTube API | High | Medium | Tier 2 fallback always works. API interaction is minimal (~100 lines) and easy to update. Monitor youtube-transcript-api Python project for upstream changes. |
| YouTube PO token enforcement on timedtext | Medium | High | Fall back to yt-dlp which maintains PO token support via plugins. Could add cookie-based auth as escape hatch. |
| Whisper API 25MB file limit for long videos | Medium | Medium | Chunk audio into <25MB segments, concatenate transcripts. yt-dlp supports time-range downloads. |
| yt-dlp not installed | Low | Medium | Clear error message with install instructions. Tier 1 still works without it. |
| Rate limiting / IP blocking by YouTube | Low | Low | Only making 3 requests per video. Add retry with backoff. Not a concern for individual CLI use. |
| Age-restricted videos | Low | Medium | Tier 1 fails (InnerTube returns error without auth cookies). Tier 2 via yt-dlp handles this (supports cookie-based auth). Document `--cookies` passthrough. |
| Both tiers fail | Low | High | Clear error with diagnostic info: which tier failed, why, and suggested fixes. Exit code 1. |
| Interrupt during Whisper upload | Low | Low | Register signal handler to clean up temp audio files in `/tmp/ytx-*`. |

## Open Questions

- [ ] Config file (`~/.config/ytx/config.toml`) in Phase 1 for API keys and default model, or env vars sufficient for v1?
- [ ] Default summarization model — Claude Sonnet or GPT-4o? (Currently defaults to Claude)
- [ ] Should `--summarize` accept an optional prompt/template argument for custom summary formats?

## References

- [YouTube InnerTube API — caption extraction flow](https://github.com/jdepoix/youtube-transcript-api)
- [OpenAI Whisper API docs](https://platform.openai.com/docs/guides/speech-to-text)
- [yt-dlp documentation](https://github.com/yt-dlp/yt-dlp)
- [whisper-rs (Rust bindings for whisper.cpp)](https://github.com/tazz4843/whisper-rs)
- [yt-transcript-rs crate](https://crates.io/crates/yt-transcript-rs)
- [ytranscript crate](https://github.com/rudrodip/ytranscript)

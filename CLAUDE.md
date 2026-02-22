# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

Scrauper is a Rust CLI tool that scrapes retro game metadata and media from ScreenScraper.fr for ES-DE (EmulationStation Desktop Edition). Key features: multi-account rotation, multi-threaded scraping with rate limiting, response caching, pixel-accurate miximage compositing (pure Rust, no ImageMagick), and proxy rotation.

## Commands

```bash
# Build
cargo build --release

# Run all tests
cargo test

# Run a single test
cargo test test_name_here

# Run tests in a specific module
cargo test module::submodule

# Format check (CI enforces this)
cargo fmt --all -- --check

# Lint (CI runs with -D warnings via workspace config)
cargo clippy --all-targets

# Quick compile check
cargo check --all-targets
```

## Lint Rules

Strict clippy configuration in `Cargo.toml` workspace lints:
- `unsafe_code = "forbid"` — no unsafe code anywhere
- `dbg_macro`, `todo`, `unimplemented` — **denied**
- `print_stdout`, `print_stderr` — **denied** (use `tracing::info!`, `tracing::warn!`, etc.)
- `clippy::all`, `pedantic`, `nursery`, `cargo` — all warn-level

Formatting: `rustfmt.toml` enforces edition 2024 style, max width 100 chars.

## Architecture

**Pipeline flow:** ROM discovery → hash computation → API lookup (hash, then filename fallback) → metadata + media download → gamelist.xml generation → miximage compositing

### Module structure (`src/`)

- **`cli.rs`** — clap-derived CLI with 7 commands (scrape, info, systems, cleanup, generate-miximages, cache-stats, init)
- **`config.rs`** — TOML config parsing (`scrauper.toml`); contains XOR-encoded default dev credentials
- **`api/`** — ScreenScraper HTTP client: `client.rs` (reqwest wrapper), `endpoints.rs` (API calls), `account_pool.rs` (multi-account rotation), `rate_limiter.rs` (governor-based), `retry.rs` (exponential backoff)
- **`scraper/`** — Core orchestration: `pipeline.rs` (main scrape/miximage flows), `scanner.rs` (ROM directory walking), `matcher.rs` (hash + fuzzy filename matching), `downloader.rs` (media fetching), `completeness.rs` (skip-complete logic)
- **`media/`** — Image processing: `miximage.rs` (composite generation), `pillarbox.rs` (black bar removal), `processing.rs` (transforms)
- **`models/`** — Data types: `game.rs`, `gamelist.rs` (XML structs), `media.rs`, `system.rs`, `region.rs`
- **`output/`** — Output writers: `gamelist_writer.rs` (XML), `media_organizer.rs` (file layout), `cleanup.rs` (orphan removal)
- **`progress.rs`** — indicatif progress bar management

### Key patterns

- **Error handling:** `anyhow::Result<T>` for application errors, `ApiError` (thiserror) for API-specific errors
- **Async runtime:** tokio (full features) throughout; tests use `#[tokio::test]`
- **HTTP mocking:** tests use `wiremock::MockServer` for API client testing
- **Temp files in tests:** `tempfile` crate for filesystem tests

## Sensitive Files

- `scrauper.toml` — contains ScreenScraper credentials; gitignored, never commit
- `scrauper.toml.example` — safe template, committed to repo
- `/cache` — API response cache directory; gitignored

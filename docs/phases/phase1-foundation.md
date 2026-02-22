# Phase 1: Foundation + API Client

## Goal
Project skeleton that compiles, parses config, and talks to the ScreenScraper API. Deliverable: `scrauper info` and `scrauper systems` work end-to-end.

## Files to Create

```
Cargo.toml
scrauper.toml.example
src/
  main.rs
  cli.rs
  config.rs
  api/
    mod.rs
    client.rs
    rate_limiter.rs
    endpoints.rs
    error.rs
    retry.rs
  models/
    mod.rs
    game.rs
    system.rs
    media.rs
    region.rs
    gamelist.rs
```

## Steps

### 1.1 — Cargo.toml + project init
- [ ] `cargo init` in project root
- [ ] Add all dependencies (see main plan for full list)
- [ ] Use `reqwest` with `default-features = false, features = ["json", "stream", "rustls-tls"]`
- [ ] Verify `cargo build` succeeds with empty main

### 1.2 — Config parsing (`config.rs`)
- [ ] Define `Config` struct with all TOML sections (credentials, paths, scraping, content, miximage, locale, network, post_processing)
- [ ] Implement `Config::load(path)` — read TOML file, deserialize with serde
- [ ] Implement `Config::default()` — sensible defaults matching ES-DE
- [ ] Tilde expansion for paths (`~/ROMs` → `/home/user/ROMs`)
- [ ] Validation: error if devid/devpassword empty, warn if ssid/sspassword empty
- [ ] Write `scrauper.toml.example` with all options documented

### 1.3 — CLI argument parsing (`cli.rs`)
- [ ] Define clap `#[derive(Parser)]` with subcommands:
  - `scrape` — with `--system`, `--force`, `--interactive` flags
  - `info` — no extra args
  - `systems` — no extra args
  - `cleanup` — no extra args
  - `generate-miximages` — with `--system` flag
  - `cache-stats` — no extra args
- [ ] Global flags: `--config <path>` (default: `./scrauper.toml`), `--verbose`

### 1.4 — Data models (`models/`)
- [ ] `game.rs`: `RomFile`, `RomHashes`, `RomType`, `ScrapedGame`, `AvailableMedia`
- [ ] `system.rs`: `System` struct (id, names, extensions, type), system ID mapping logic
- [ ] `media.rs`: `MediaType` enum with `es_de_dirname()` and `api_media_id(region)` methods
- [ ] `region.rs`: Region/Language enums, `resolve_with_fallback(priority_list, available)` function
- [ ] `gamelist.rs`: `GameListEntry` and `GameList` structs with quick-xml serde support

### 1.5 — API error types (`api/error.rs`)
- [ ] `ApiError` enum: `RateLimited`, `DailyQuotaExceeded`, `TooManyUnknownRoms`, `GameNotFound`, `ApiClosedOverloaded`, `ApiClosedDown`, `SoftwareBlacklisted`, `InvalidCredentials`, `Request`, `Deserialize`
- [ ] Implement `From<reqwest::Error>` for `ApiError`
- [ ] Map HTTP status codes (429, 430, 431, 401, 403, 404, 423, 426) to enum variants

### 1.6 — API client (`api/client.rs`)
- [ ] `ScreenScraperClient` struct holding: `reqwest::Client`, credentials, softname, base URL
- [ ] Constructor: build reqwest client with timeouts from config
- [ ] `build_base_params()` — returns common query params (devid, devpassword, softname, ssid, sspassword, output=json)
- [ ] `get_json<T>(&self, endpoint, extra_params) -> Result<T, ApiError>` — generic GET + deserialize
- [ ] Response status code checking and ApiError mapping

### 1.7 — Rate limiter (`api/rate_limiter.rs`)
- [ ] `ApiQuotaTracker` struct with:
  - `tokio::sync::Semaphore` (permits = maxthreads)
  - `governor::RateLimiter` (maxrequestspermin tokens/minute)
  - `AtomicU32` daily counter + daily limit
- [ ] `acquire()` — await semaphore + rate limiter, check daily counter
- [ ] `release()` — release semaphore permit
- [ ] `is_daily_exhausted()` check

### 1.8 — Retry logic (`api/retry.rs`)
- [ ] `with_retry<F>(config, operation) -> Result<T, ApiError>` generic retry wrapper
- [ ] Retry policy per error type (see retry policy table in main plan)
- [ ] Exponential backoff: `retry_delay_secs * 2^attempt`
- [ ] Log each retry attempt with tracing

### 1.9 — API endpoints (`api/endpoints.rs`)
- [ ] `get_user_info()` → `UserInfo` struct (maxthreads, quotas, requeststoday, etc.)
- [ ] `get_infra_info()` → `InfraInfo` struct (CPU loads, active threads, API status)
- [ ] `get_systems_list()` → `Vec<System>` (id, names, extensions, type)
- [ ] JSON response structs matching ScreenScraper API format
- [ ] System list caching: save to `~/.config/scrauper/systems_cache.json`, load if fresh

### 1.10 — Wire up `main.rs`
- [ ] Parse CLI args
- [ ] Load config (or print error with guidance)
- [ ] Init tracing subscriber (with `--verbose` support)
- [ ] Dispatch to subcommand handlers
- [ ] `scrauper info`: call `get_user_info()` + `get_infra_info()`, print formatted table
- [ ] `scrauper systems`: call `get_systems_list()`, print formatted table (id, name, type, extensions)

## Verification
- [ ] `cargo build` compiles cleanly
- [ ] `cargo test` passes (unit tests for config parsing, region fallback, media type mapping)
- [ ] `scrauper --help` shows all subcommands
- [ ] `scrauper info` with valid credentials prints user quota info
- [ ] `scrauper systems` prints system list (cached on second run)
- [ ] `scrauper info` with bad credentials prints clear error message

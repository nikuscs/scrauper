# Scrauper: Master Plan

ScreenScraper API scraper for ES-DE, written in Rust. Multi-threaded, cross-platform (including Android ARM).

## Phase Overview

| Phase | Name | Deliverable | Status |
|-------|------|-------------|--------|
| 1 | [Foundation + API](phases/phase1-foundation.md) | `scrauper info` + `scrauper systems` work | Not started |
| 2 | [Scrape Pipeline](phases/phase2-scrape-pipeline.md) | `scrauper scrape` downloads metadata + media | Not started |
| 3 | [Miximage Generation](phases/phase3-miximage.md) | Pixel-accurate ES-DE miximages | Not started |
| 4 | [Cache + Advanced](phases/phase4-cache-advanced.md) | Offline mode, interactive, cleanup | Not started |
| 5 | [Release Polish](phases/phase5-release.md) | Cross-platform builds, CI/CD, docs | Not started |

---

## Phase 1: Foundation + API Client
> [Full plan](phases/phase1-foundation.md)

- [ ] 1.1 — Cargo.toml + project init
- [ ] 1.2 — Config parsing (scrauper.toml)
- [ ] 1.3 — CLI argument parsing (clap)
- [ ] 1.4 — Data models (game, system, media, region, gamelist)
- [ ] 1.5 — API error types
- [ ] 1.6 — API client (reqwest + auth)
- [ ] 1.7 — Rate limiter (semaphore + governor + daily counter)
- [ ] 1.8 — Retry logic (exponential backoff)
- [ ] 1.9 — API endpoints (user info, infra info, systems list)
- [ ] 1.10 — Wire up main.rs (`scrauper info`, `scrauper systems`)

## Phase 2: Scrape Pipeline
> [Full plan](phases/phase2-scrape-pipeline.md)

- [ ] 2.1 — ROM scanner (directory walking, extension filtering, BIOS exclusion)
- [ ] 2.2 — ROM hashing (CRC32 + MD5 + SHA1 via spawn_blocking)
- [ ] 2.3 — Game matching (hash lookup → filename fallback)
- [ ] 2.4 — Completeness checker (skip already-scraped games)
- [ ] 2.5 — Media downloader (streaming + CRC skip optimization)
- [ ] 2.6 — Gamelist writer (merge + preserve user fields)
- [ ] 2.7 — Media organizer (ES-DE directory structure)
- [ ] 2.8 — Progress reporting (indicatif)
- [ ] 2.9 — Pipeline orchestrator (parallel workers via JoinSet)
- [ ] 2.10 — Wire up `scrauper scrape`

## Phase 3: Miximage Generation
> [Full plan](phases/phase3-miximage.md)

- [ ] 3.1 — Image processing utilities (fit, crop, contain, shadow, frame color sampling)
- [ ] 3.2 — Pillarbox/letterbox removal
- [ ] 3.3 — Miximage generator (ES-DE pixel-accurate compositing)
- [ ] 3.4 — Integrate into scrape pipeline
- [ ] 3.5 — Offline generation command (`scrauper generate-miximages`)
- [ ] 3.6 — Update completeness checker for miximages

## Phase 4: Cache + Advanced Features
> [Full plan](phases/phase4-cache-advanced.md)

- [ ] 4.1 — Cache store (per-game metadata + media)
- [ ] 4.2 — Cache index (ROM hash → cached resources)
- [ ] 4.3 — Integrate cache into scrape pipeline
- [ ] 4.4 — `scrauper cache-stats` command
- [ ] 4.5 — Interactive mode (confidence scoring + user prompts)
- [ ] 4.6 — M3U multi-disc refinement
- [ ] 4.7 — Orphan cleanup (`scrauper cleanup`)
- [ ] 4.8 — Enhanced progress reporting (multi-progress, ETA, summary table)

## Phase 5: Release Polish
> [Full plan](phases/phase5-release.md)

- [ ] 5.1 — Cross-compilation setup (Android ARM, RPi, Steam Deck)
- [ ] 5.2 — CI/CD (GitHub Actions)
- [ ] 5.3 — Binary size optimization
- [ ] 5.4 — Error messages and UX (`scrauper init`, Ctrl+C handling)
- [ ] 5.5 — Documentation (README, examples)
- [ ] 5.6 — Testing (integration tests, benchmarks)

---

## Reference

Detailed specifications live in:
- [API Spec](api-spec.md) — ScreenScraper API v2 endpoints, parameters, responses
- [Phase 3](phases/phase3-miximage.md) — ES-DE miximage pixel-accurate layout constants (from MiximageGenerator.cpp source)

### Key Architecture Decisions
- **Pure Rust deps** — no C dependencies, cross-compiles to Android ARM
- **reqwest + rustls-tls** — no OpenSSL needed
- **Parallelization** — N workers (N = API maxthreads), semaphore + rate limiter + daily counter
- **Gamelist preservation** — never overwrite playcount, lastplayed, favorite, kidgame, hidden
- **Local cache** — enables offline miximage regeneration
- **Atomic writes** — gamelist.xml written via .tmp + rename

### Config Reference
Full TOML schema with all options: see [Phase 1](phases/phase1-foundation.md) step 1.2, or the example config in the [original plan archive](phases/phase1-foundation.md).

### Thread Tiers (ScreenScraper)
- Free: 1 thread
- Financial level 2: +1 (= 2 total)
- Financial level 3+: +5 (= 6+ total)
- Auto-detected via `ssuserInfos.php` on startup

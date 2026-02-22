# Phase 4: Local Cache + Advanced Features

## Goal
Add local resource caching for offline regeneration, interactive mode with confidence scoring, and cleanup tools. This phase turns scrauper from "works" to "works well for power users".

## Dependencies
- Phase 3 complete (scrape pipeline + miximage generation working)

## Files to Create/Modify

```
src/
  cache/
    mod.rs
    store.rs
    index.rs
  output/
    cleanup.rs
  scraper/
    matcher.rs    (extend with confidence scoring + interactive)
    scanner.rs    (extend with M3U support refinement)
    pipeline.rs   (integrate cache layer)
```

## Steps

### 4.1 — Cache store (`cache/store.rs`)
- [ ] Cache location: `~/.config/scrauper/cache/<system>/`
- [ ] Per-game cache structure:
  ```
  cache/<system>/<rom_crc32>/
    metadata.json      — API response (ScrapedGame serialized)
    screenshot.png     — downloaded media files
    cover.png
    marquee.png
    box3d.png
    ...
  ```
- [ ] `CacheStore::save_metadata(system, rom_hash, scraped_game)` — serialize + write
- [ ] `CacheStore::load_metadata(system, rom_hash) -> Option<ScrapedGame>` — read + deserialize
- [ ] `CacheStore::save_media(system, rom_hash, media_type, bytes)` — write media file
- [ ] `CacheStore::get_media_path(system, rom_hash, media_type) -> Option<PathBuf>` — check if cached
- [ ] `CacheStore::clear_system(system)` — delete system cache
- [ ] `CacheStore::stats() -> CacheStats` — per-system entry counts and total size

### 4.2 — Cache index (`cache/index.rs`)
- [ ] `CacheIndex` — maps ROM filename/hash to cache directory
- [ ] `index.json` per system: `{ "rom_filename": { "hash": "...", "game_id": N, "cached_at": "..." } }`
- [ ] Lookup: by hash first, then by filename
- [ ] Used by `generate-miximages` to find cached media without API calls

### 4.3 — Integrate cache into scrape pipeline
- [ ] **Before API call**: check cache for existing metadata
  - If cached and `skip_complete = true`: use cached data, skip API call entirely
  - If cached and `--force`: re-fetch from API, update cache
- [ ] **After API call**: cache the response
- [ ] **After media download**: cache the media file (in addition to placing in ES-DE dir)
- [ ] **Offline generate-miximages**: read media from cache, not ES-DE dirs
  - This lets users regenerate miximages with different settings without re-downloading

### 4.4 — Cache stats command (`scrauper cache-stats`)
- [ ] List each system with: entry count, total cache size
- [ ] Show total cache size across all systems
- [ ] Show cache directory path

### 4.5 — Interactive mode (`scraper/matcher.rs` extension)
- [ ] `MatchConfidence` enum: `HashMatch(1.0)`, `ExactName(0.95)`, `FuzzyMatch(score)`, `UserSelected`
- [ ] When `interactive = true` and confidence < `min_auto_accept_confidence`:
  - Display search results with confidence scores
  - Use `dialoguer::Select` for user to pick correct match (or skip)
  - `dialoguer::Confirm` for single low-confidence match
- [ ] When `interactive = false` and confidence < threshold:
  - Log warning, skip game, add to "unmatched" report
- [ ] Fuzzy matching via `strsim::normalized_damerau_levenshtein` on cleaned filenames
- [ ] Filename cleaning: strip `(USA)`, `(Rev A)`, `[!]`, file extension, etc.

### 4.6 — M3U multi-disc refinement
- [ ] Parse `.m3u` file: extract list of disc file paths (relative to .m3u location)
- [ ] Validate all referenced disc files exist
- [ ] Hash the first disc file for API matching
- [ ] When `hide_individual_discs = true`:
  - In gamelist.xml, set `<hidden>true</hidden>` on individual disc entries
  - Only the .m3u entry gets full metadata
- [ ] Handle case where .m3u references files in subdirectories

### 4.7 — Orphan cleanup (`output/cleanup.rs`)
- [ ] `scrauper cleanup` command
- [ ] Scan ES-DE media directories for each system
- [ ] For each media file, check if corresponding ROM exists
- [ ] If ROM is gone: list the orphaned media file
- [ ] Prompt user for confirmation before deletion (unless `--yes` flag)
- [ ] Support `--dry-run` to show what would be deleted
- [ ] Respect `remove_orphaned_media` config toggle

### 4.8 — Enhanced progress reporting
- [ ] Multi-progress with `indicatif::MultiProgress`:
  - Top bar: overall progress across all systems
  - Per-system bar: current system progress
  - Status line: current game being processed + action (hashing/looking up/downloading)
- [ ] Thread utilization display: "Active workers: 4/6"
- [ ] Quota display: "Requests today: 234/10000"
- [ ] ETA calculation based on average time per game
- [ ] Final summary table:
  ```
  System       | Scraped | Skipped | Not Found | Errors
  snes         |      42 |      18 |         3 |      0
  megadrive    |      67 |       5 |        12 |      1
  Total        |     109 |      23 |        15 |      1
  ```

## Verification
- [ ] Cache populated after first scrape run
- [ ] Second scrape run with cache hits: no API calls for already-cached games
- [ ] `scrauper generate-miximages` works from cache without network
- [ ] `scrauper cache-stats` shows correct per-system data
- [ ] Interactive mode prompts correctly for ambiguous matches
- [ ] Interactive mode auto-accepts high-confidence hash matches
- [ ] M3U games appear as single entries in gamelist.xml
- [ ] Individual disc entries hidden when configured
- [ ] `scrauper cleanup --dry-run` lists orphans without deleting
- [ ] `scrauper cleanup` deletes orphans after confirmation
- [ ] Progress bars update smoothly during multi-threaded operation
- [ ] ETA is reasonably accurate after ~10 games

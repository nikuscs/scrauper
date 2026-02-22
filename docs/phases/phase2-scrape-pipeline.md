# Phase 2: Scrape Pipeline

## Goal
Working `scrauper scrape` that discovers ROMs, looks up games on ScreenScraper, downloads metadata + media, and writes gamelist.xml with media files in ES-DE's directory structure. No miximage generation yet.

## Dependencies
- Phase 1 complete (config, CLI, API client, models, rate limiter)

## Files to Create/Modify

```
src/
  scraper/
    mod.rs
    scanner.rs
    matcher.rs
    completeness.rs
    pipeline.rs
    downloader.rs
  output/
    mod.rs
    gamelist_writer.rs
    media_organizer.rs
  progress.rs
```

## Steps

### 2.1 — ROM scanner (`scraper/scanner.rs`)
- [ ] `scan_system(rom_dir, system_name, extensions, exclude_patterns) -> Vec<RomFile>`
- [ ] Walk directory with `walkdir`, filter by known ROM extensions per system
- [ ] Apply `exclude_patterns` via `globset`
- [ ] Skip BIOS files when `skip_bios = true` (maintain list of known BIOS filenames)
- [ ] Detect `.m3u` files: parse contents to find referenced disc files
- [ ] For M3U: create single `RomFile` entry for the .m3u, store first disc path for hashing
- [ ] `scan_all_systems(config) -> HashMap<String, Vec<RomFile>>` — discover system folders, map to system IDs

### 2.2 — ROM hashing (`scraper/scanner.rs`)
- [ ] `hash_rom(path, max_size_mb) -> Option<RomHashes>` — compute CRC32 + MD5 + SHA1
- [ ] Run via `tokio::task::spawn_blocking` (CPU-bound)
- [ ] Skip hashing if file size > `hash_max_file_size_mb`
- [ ] Stream file in chunks (8KB buffer) to avoid loading entire file into memory
- [ ] For M3U games: hash the first referenced disc file
- [ ] Return `RomHashes { crc32, md5, sha1, file_size }`

### 2.3 — Game matching (`scraper/matcher.rs`)
- [ ] `lookup_game(client, rom_file, config) -> Result<(ScrapedGame, MatchConfidence), ApiError>`
- [ ] **Strategy 1 — Hash lookup**: Call `jeuInfos.php` with CRC/MD5/SHA1 + systemeid + romnom + romtaille
- [ ] **Strategy 2 — Filename fallback**: On 404, clean ROM filename (strip region tags, hashes, extensions), call `jeuRecherche.php` with cleaned name + systemeid
- [ ] From search results (up to 30), pick best match or prompt user
- [ ] `MatchConfidence` enum: `HashMatch`, `ExactNameMatch`, `FuzzyMatch(f32)`, `UserSelected`
- [ ] Extract metadata from API response using region + language priority:
  - Name: `noms.nom_{region}` with region_priority fallback
  - Description: `synopsis_{language}`
  - Rating: `note / 20.0` (API gives 0-20, ES-DE wants 0.0-1.0)
  - Release date: `dates.date_{region}` → format as `yyyyMMddT000000`
  - Publisher: `editeur`, Developer: `developpeur`
  - Genre: `genres_{language}`, join multiple with ", "
  - Players: `joueurs`

### 2.4 — Completeness checker (`scraper/completeness.rs`)
- [ ] `is_complete(entry, config, media_dir, system_name) -> bool`
- [ ] Check each enabled metadata field against existing gamelist.xml entry
- [ ] Check each enabled media type: does file exist at `<media_dir>/<system>/<type>/<rom_stem>.{png,jpg,mp4,pdf}`?
- [ ] Game with no gamelist entry → always incomplete
- [ ] Disabled toggles → not checked

### 2.5 — Media downloader (`scraper/downloader.rs`)
- [ ] `download_media(client, game_id, system_id, media_type, region_priority, dest_path) -> Result<DownloadResult>`
- [ ] Try each region in `region_priority` until one returns actual media
- [ ] CRC skip optimization: if local file exists, compute its CRC, send to API
- [ ] If response is `CRCOK`/`MD5OK`/`SHA1OK` → skip, return `DownloadResult::Unchanged`
- [ ] If response is `NOMEDIA` → try next region, or return `DownloadResult::NotAvailable`
- [ ] Otherwise: stream response bytes to temp file, then rename to final path
- [ ] Use `mediaJeu.php` for images, `mediaVideoJeu.php` for video, `mediaManuelJeu.php` for manuals
- [ ] Fallback: if 3D box unavailable and `cover_fallback = true`, download 2D cover to 3dboxes/ directory

### 2.6 — Gamelist writer (`output/gamelist_writer.rs`)
- [ ] `read_gamelist(path) -> GameList` — parse existing gamelist.xml with quick-xml
- [ ] `merge_game(gamelist, rom_path, scraped_game) -> GameList` — merge scraped data into existing
- [ ] Preserve user fields: `playcount`, `lastplayed`, `favorite`, `kidgame`, `hidden`, `altemulator`
- [ ] Only update scraped fields that are currently empty (unless `--force`)
- [ ] `write_gamelist(gamelist, path)` — serialize to XML, atomic write (.tmp + rename)
- [ ] Handle M3U: if `hide_individual_discs`, mark disc entries with `<hidden>true</hidden>`

### 2.7 — Media organizer (`output/media_organizer.rs`)
- [ ] `get_media_path(media_dir, system, media_type, rom_stem) -> PathBuf`
- [ ] Create directory structure: `<media_dir>/<system>/<media_type>/`
- [ ] Handle subdirectories for ROMs in subfolders (mirror ROM dir structure under media dir)
- [ ] File extension mapping: screenshots/box art → `.png`, video → `.mp4`, manual → `.pdf`

### 2.8 — Progress reporting (`progress.rs`)
- [ ] `ScrapeProgress` struct wrapping `indicatif::MultiProgress`
- [ ] Overall progress bar: "Scraping [system]: [X/Y games]"
- [ ] Per-game status line: current ROM being processed
- [ ] Summary at end: "Scraped: X, Skipped (complete): Y, Not found: Z, Errors: W"
- [ ] Thread/quota display: "Threads: N/M, Today: X/Y requests"

### 2.9 — Pipeline orchestrator (`scraper/pipeline.rs`)
- [ ] `run_scrape(config) -> Result<ScrapeReport>`
- [ ] **Init**: Call `ssuserInfos.php`, init rate limiter with quotas
- [ ] **Discover**: Scan ROM dirs, map to system IDs, count totals
- [ ] **Filter**: Per system, load existing gamelist.xml, run completeness check, build work queue
- [ ] **Scrape**: Spawn `tokio::JoinSet` of tasks, capped by semaphore
  - Each task: hash → lookup → download media → return result
  - Acquire semaphore + rate limiter before each API call
  - Increment daily counter after each API call
  - On 430/431: signal all workers to stop gracefully
- [ ] **Output**: Collect results, merge into gamelist per system, write atomically
- [ ] **Summary**: Print stats

### 2.10 — Wire up `scrauper scrape` in main.rs
- [ ] Handle `--system` filter (single system or all)
- [ ] Handle `--force` (skip completeness check)
- [ ] Handle `--interactive` (store in runtime config for matcher)
- [ ] Call `pipeline::run_scrape(config)`

## Verification
- [ ] `scrauper scrape --system megadrive` with test ROMs downloads metadata + media
- [ ] gamelist.xml is created with correct XML structure
- [ ] Media files land in correct ES-DE directories (screenshots/, covers/, marquees/, etc.)
- [ ] Re-running skips already-complete games (skip_complete = true)
- [ ] `--force` re-scrapes even complete games
- [ ] User fields (playcount, favorite) survive re-scrape
- [ ] Rate limiting works: no 429 errors during normal operation
- [ ] Progress bars show meaningful information
- [ ] Games not found are logged but don't crash the session

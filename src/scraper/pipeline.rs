use std::sync::Arc;

use anyhow::Result;
use tokio::sync::watch;

use crate::api::client::ScreenScraperClient;
use crate::api::endpoints;
use crate::api::error::ApiError;
use crate::api::rate_limiter::ApiQuotaTracker;
use crate::api::retry::with_retry;
use crate::cache::index::CacheIndex;
use crate::cache::store::CacheStore;
use crate::config::Config;
use crate::media::miximage::MiximageGenerator;
use crate::models::game::ScrapedGame;
use crate::models::media::MediaType;
use crate::output::{gamelist_writer, media_organizer};
use crate::progress::ScrapeProgress;
use crate::scraper::{completeness, downloader, matcher, scanner};

/// Runtime options for a scrape session.
pub struct ScrapeOptions {
    pub system_filter: Option<String>,
    pub force: bool,
    pub interactive: bool,
}

/// Result of scraping a single game.
struct GameResult {
    rom_path: String,
    game: ScrapedGame,
}

/// Run the full scrape pipeline.
pub async fn run_scrape(config: &Config, options: &ScrapeOptions) -> Result<()> {
    // 1. Init: fetch user info, set up rate limiter
    let client = ScreenScraperClient::new(config)?;

    tracing::info!("Fetching account info...");
    let user_info = with_retry(&config.network, || client.get_user_info()).await?;

    let max_threads = if config.network.max_threads_override > 0 {
        config.network.max_threads_override
    } else {
        user_info.max_threads
    };

    tracing::info!(
        "User: {} | Threads: {} | Requests today: {}/{}",
        user_info.username,
        max_threads,
        user_info.requests_today,
        user_info.max_requests_per_day
    );

    let quota = Arc::new(ApiQuotaTracker::new(
        max_threads,
        user_info.max_requests_per_min,
        user_info.max_requests_per_day,
        user_info.requests_today,
    ));

    // Init cache store
    let cache = Arc::new(CacheStore::new(&CacheStore::default_dir()));

    // 2. Discover: get systems list, scan ROM directories
    let systems = if let Some(s) = endpoints::load_systems_cache()? {
        s
    } else {
        let s = client.get_systems_list().await?;
        endpoints::save_systems_cache(&s)?;
        s
    };

    tracing::info!("Scanning ROM directories...");
    let discovered = scanner::scan_all_systems(config, &systems, options.system_filter.as_deref())?;

    if discovered.is_empty() {
        tracing::info!("No ROMs found to scrape.");
        return Ok(());
    }

    let total_roms: usize = discovered.values().map(|(_, roms)| roms.len()).sum();
    tracing::info!("Found {} ROMs across {} system(s)", total_roms, discovered.len());

    // Abort signal for fatal errors and Ctrl+C
    let (abort_tx, abort_rx) = watch::channel(false);
    let mut progress = ScrapeProgress::new();
    progress.set_total(total_roms as u64);

    // Wire Ctrl+C into the abort channel
    let ctrlc_abort_tx = abort_tx.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            tracing::warn!("Ctrl+C received — finishing current games then stopping...");
            let _ = ctrlc_abort_tx.send(true);
        }
    });

    // 3. Process each system
    for (system_id, (system_name, roms)) in &discovered {
        // Check abort signal between systems
        if *abort_rx.borrow() {
            tracing::warn!("Aborting remaining systems");
            break;
        }

        let system_id = *system_id;
        let rom_dir = config.rom_directory().join(system_name);
        let media_dir = config.media_directory();
        let gamelist_dir = config.gamelist_directory();
        let gamelist_path = gamelist_dir.join(system_name).join("gamelist.xml");

        // Load existing gamelist for completeness check
        let mut gamelist = gamelist_writer::read_gamelist(&gamelist_path)?;

        // 4. Filter: build work queue (skip complete games unless --force)
        let work_queue: Vec<_> = if options.force {
            roms.iter().collect()
        } else {
            roms.iter()
                .filter(|rom| {
                    let stem = media_organizer::rom_stem(&rom.path);
                    let gl_path = media_organizer::gamelist_rom_path(&rom.path, &rom_dir);
                    let entry = gamelist.find_by_path(&gl_path);
                    !completeness::is_complete(entry, &stem, config, &media_dir, system_name)
                })
                .collect()
        };

        let skipped = roms.len() - work_queue.len();
        if skipped > 0 {
            tracing::info!("{}: Skipping {} already-complete games", system_name, skipped);
            progress
                .stats()
                .skipped
                .fetch_add(skipped as u32, std::sync::atomic::Ordering::Relaxed);
        }

        if work_queue.is_empty() {
            tracing::info!("{}: All games already complete, skipping", system_name);
            progress.record_system(system_name, 0, skipped as u32, 0, 0);
            continue;
        }

        // Snapshot stats before this system
        let (pre_scraped, _, pre_not_found, pre_errors) = progress.stats().snapshot();

        progress.start_system(system_name, work_queue.len() as u64);

        // 5. Scrape: parallel workers via JoinSet
        let mut results: Vec<GameResult> = Vec::new();
        let mut join_set = tokio::task::JoinSet::new();
        let stats = progress.stats();

        for rom in work_queue {
            // Check abort signal
            if *abort_rx.borrow() {
                tracing::warn!("Abort signal received, stopping scrape");
                break;
            }

            let client = client.clone();
            let config = config.clone();
            let quota = quota.clone();
            let cache = cache.clone();
            let rom = rom.clone();
            let abort_tx = abort_tx.clone();
            let media_dir = media_dir.clone();
            let rom_dir = rom_dir.clone();
            let system_name = system_name.clone();
            let stats = stats.clone();
            let interactive = options.interactive;

            join_set.spawn(async move {
                let result = scrape_single_game(
                    &client,
                    &config,
                    &quota,
                    &cache,
                    &rom,
                    system_id,
                    &system_name,
                    &rom_dir,
                    &media_dir,
                    interactive,
                )
                .await;

                match result {
                    Ok(Some(game_result)) => {
                        stats.inc_scraped();
                        Some(game_result)
                    }
                    Ok(None) => {
                        stats.inc_not_found();
                        None
                    }
                    Err(e) => {
                        if is_fatal_error(&e) {
                            tracing::error!("Fatal error, aborting: {}", e);
                            let _ = abort_tx.send(true);
                        } else {
                            tracing::error!("Error scraping {}: {}", rom.filename, e);
                        }
                        stats.inc_errors();
                        None
                    }
                }
            });
        }

        // Collect results
        while let Some(result) = join_set.join_next().await {
            progress.inc_system();
            match result {
                Ok(Some(game_result)) => results.push(game_result),
                Ok(None) => {}
                Err(e) => tracing::error!("Task panicked: {}", e),
            }
        }

        progress.finish_system();

        // Record per-system stats (delta from pre-snapshot)
        let (post_scraped, _, post_not_found, post_errors) = progress.stats().snapshot();
        progress.record_system(
            system_name,
            post_scraped - pre_scraped,
            skipped as u32,
            post_not_found - pre_not_found,
            post_errors - pre_errors,
        );

        // 6. Output: merge results into gamelist
        for result in &results {
            let entry = gamelist_writer::scraped_to_entry(&result.game, &result.rom_path);
            gamelist.merge_or_insert(&result.rom_path, &entry, options.force);
        }

        // Hide individual disc entries when M3U playlists are present
        if config.scraping.hide_individual_discs {
            hide_m3u_disc_entries(roms, &rom_dir, &mut gamelist);
        }

        if !results.is_empty() {
            gamelist_writer::write_gamelist(&gamelist, &gamelist_path)?;
            tracing::info!(
                "{}: Updated gamelist.xml ({} games scraped)",
                system_name,
                results.len()
            );
        }
    }

    // 7. Summary
    progress.print_summary();
    tracing::info!("Requests used: {}/{}", quota.requests_today(), quota.daily_limit());

    Ok(())
}

/// Scrape a single game: check cache -> hash -> lookup -> download media -> cache.
async fn scrape_single_game(
    client: &ScreenScraperClient,
    config: &Config,
    quota: &ApiQuotaTracker,
    cache: &CacheStore,
    rom: &crate::models::game::RomFile,
    system_id: u64,
    system_name: &str,
    rom_dir: &std::path::Path,
    media_dir: &std::path::Path,
    interactive: bool,
) -> Result<Option<GameResult>> {
    // Hash the ROM
    let hash_target = rom.hash_target.as_deref().unwrap_or(&rom.path);
    let hashes = scanner::hash_rom(hash_target, config.scraping.hash_max_file_size_mb).await?;

    let rom_hash = hashes.as_ref().map(|h| h.crc32.clone());

    // Check cache before API call (--force is handled upstream by work queue filter)
    let (game, from_cache) = if let Some(ref hash) = rom_hash {
        if let Some(cached) = cache.load_metadata(system_name, hash)? {
            tracing::debug!("Cache hit: {} -> {}", rom.filename, cached.name);
            (Some(cached), true)
        } else {
            (None, false)
        }
    } else {
        (None, false)
    };

    let game = if let Some(game) = game {
        game
    } else {
        // Acquire rate limiter permit for API call
        let _permit = quota.acquire().await?;

        // Look up game
        let match_result = with_retry(&config.network, || {
            matcher::lookup_game(client, rom, hashes.as_ref(), system_id, config, interactive)
        })
        .await?;

        let Some((game, confidence)) = match_result else {
            tracing::info!("Not found: {}", rom.filename);
            return Ok(None);
        };

        tracing::info!("Matched: {} -> {} (confidence: {:?})", rom.filename, game.name, confidence);

        // Cache the API response
        if let Some(ref hash) = rom_hash {
            if let Err(e) = cache.save_metadata(system_name, hash, &game) {
                tracing::debug!("Failed to cache metadata for {}: {}", rom.filename, e);
            }

            // Update cache index
            if let Ok(mut index) = CacheIndex::load(cache.base_dir(), system_name) {
                index.upsert(&rom.filename, hash, game.game_id);
                let _ = index.save(cache.base_dir(), system_name);
            }
        }

        game
    };

    // Download enabled media
    let rom_stem = media_organizer::rom_stem(&rom.path);
    let media_types = downloader::enabled_media_types(config);

    for media_type in &media_types {
        let dest = media_organizer::get_media_path(media_dir, system_name, media_type, &rom_stem);

        // If we got a cache hit and the media already exists on disk, skip download
        if from_cache && dest.exists() {
            tracing::debug!("Media exists (cached game): {}", dest.display());
            continue;
        }

        // Check if media is cached locally
        if let Some(ref hash) = rom_hash {
            if let Some(cached_path) = cache.get_media_path(system_name, hash, media_type) {
                if dest.exists() {
                    continue;
                }
                // Copy from cache to ES-DE directory
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                if let Err(e) = std::fs::copy(&cached_path, &dest) {
                    tracing::debug!("Failed to copy cached media: {}", e);
                } else {
                    tracing::debug!("Restored from cache: {}", dest.display());
                    continue;
                }
            }
        }

        match downloader::download_media(client, game.game_id, system_id, media_type, &dest, config)
            .await
        {
            Ok(downloader::DownloadResult::Downloaded(p, bytes)) => {
                tracing::debug!("Downloaded: {}", p.display());
                // Cache the downloaded media
                if let Some(ref hash) = rom_hash {
                    if let Err(e) = cache.save_media(system_name, hash, media_type, &bytes) {
                        tracing::debug!("Failed to cache media: {}", e);
                    }
                }
            }
            Ok(downloader::DownloadResult::Unchanged) => {
                tracing::debug!("Unchanged: {}", dest.display());
            }
            Ok(downloader::DownloadResult::NotAvailable) => {
                tracing::debug!(
                    "Not available: {} for {}",
                    media_type.es_de_dirname(),
                    rom.filename
                );
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to download {} for {}: {}",
                    media_type.es_de_dirname(),
                    rom.filename,
                    e
                );
            }
        }
    }

    // Generate miximage if enabled
    if config.miximage.enabled {
        generate_miximage(config, media_dir, system_name, &rom_stem, &rom.filename).await;
    }

    let rom_path = media_organizer::gamelist_rom_path(&rom.path, rom_dir);
    Ok(Some(GameResult { rom_path, game }))
}

/// Generate miximage from available media files.
async fn generate_miximage(
    config: &Config,
    media_dir: &std::path::Path,
    system_name: &str,
    rom_stem: &str,
    rom_filename: &str,
) {
    let screenshot_path =
        media_organizer::get_media_path(media_dir, system_name, &MediaType::Screenshot, rom_stem);

    if !screenshot_path.exists() {
        return;
    }

    let miximage_path =
        media_organizer::get_media_path(media_dir, system_name, &MediaType::Miximage, rom_stem);

    if !config.miximage.overwrite_existing && miximage_path.exists() {
        return;
    }

    let marquee_path =
        media_organizer::get_media_path(media_dir, system_name, &MediaType::Marquee, rom_stem);
    let box_path =
        media_organizer::get_media_path(media_dir, system_name, &MediaType::Box3d, rom_stem);
    let box_path = if box_path.exists() {
        Some(box_path)
    } else {
        let cover =
            media_organizer::get_media_path(media_dir, system_name, &MediaType::BoxFront, rom_stem);
        if cover.exists() {
            Some(cover)
        } else {
            None
        }
    };
    let pm_path = media_organizer::get_media_path(
        media_dir,
        system_name,
        &MediaType::PhysicalMedia,
        rom_stem,
    );

    let config_clone = config.clone();
    let screenshot_path_clone = screenshot_path.clone();
    let marquee_exists = marquee_path.exists();
    let pm_exists = pm_path.exists();
    let rom_filename = rom_filename.to_string();

    // Run in spawn_blocking (CPU-bound image processing)
    if let Err(e) = tokio::task::spawn_blocking(move || {
        let generator = MiximageGenerator::new(
            &config_clone.miximage,
            config_clone.post_processing.remove_pillarboxes,
        );
        generator.generate(
            &screenshot_path_clone,
            if marquee_exists { Some(marquee_path.as_path()) } else { None },
            box_path.as_deref(),
            if pm_exists { Some(pm_path.as_path()) } else { None },
            &miximage_path,
        )
    })
    .await
    .unwrap_or_else(|e| Err(anyhow::anyhow!("Spawn panic: {}", e)))
    {
        tracing::warn!("Failed to generate miximage for {}: {}", rom_filename, e);
    }
}

/// For M3U multi-disc games, set `hidden=true` on individual disc entries in the gamelist.
fn hide_m3u_disc_entries(
    roms: &[crate::models::game::RomFile],
    rom_dir: &std::path::Path,
    gamelist: &mut crate::models::gamelist::GameList,
) {
    for rom in roms {
        if rom.path.extension().and_then(|e| e.to_str()) != Some("m3u") {
            continue;
        }

        let Ok(disc_paths) = scanner::get_m3u_disc_paths(&rom.path, rom_dir) else {
            continue;
        };

        for disc_path in &disc_paths {
            let gl_path = media_organizer::gamelist_rom_path(disc_path, rom_dir);
            if let Some(entry) = gamelist.find_by_path_mut(&gl_path) {
                if entry.hidden.as_deref() != Some("true") {
                    entry.hidden = Some("true".to_string());
                    tracing::debug!("Hiding disc entry: {}", gl_path);
                }
            }
        }
    }
}

fn is_fatal_error(err: &anyhow::Error) -> bool {
    err.downcast_ref::<ApiError>().is_some_and(ApiError::is_fatal)
}

/// Generate miximages from existing media files (offline, no API calls).
#[allow(clippy::print_stderr)]
pub async fn run_generate_miximages(config: &Config, system_filter: Option<&str>) -> Result<()> {
    let media_dir = config.media_directory();

    if !media_dir.exists() {
        anyhow::bail!("Media directory not found: {}", media_dir.display());
    }

    // List system directories under media_dir
    let entries: Vec<_> = std::fs::read_dir(&media_dir)?
        .filter_map(std::result::Result::ok)
        .filter(|e| e.path().is_dir())
        .collect();

    let mut total_generated = 0u32;
    let mut total_skipped = 0u32;

    for entry in entries {
        let system_name = entry.file_name().to_str().unwrap_or("").to_string();

        if let Some(filter) = system_filter {
            if system_name.to_lowercase() != filter.to_lowercase() {
                continue;
            }
        }

        // Find screenshots directory
        let screenshots_dir = media_dir.join(&system_name).join("screenshots");
        if !screenshots_dir.exists() {
            continue;
        }

        // Collect screenshot files
        let screenshots: Vec<_> = std::fs::read_dir(&screenshots_dir)?
            .filter_map(std::result::Result::ok)
            .filter(|e| {
                let path = e.path();
                path.is_file()
                    && matches!(
                        path.extension().and_then(|e| e.to_str()),
                        Some("png" | "jpg" | "jpeg")
                    )
            })
            .collect();

        if screenshots.is_empty() {
            continue;
        }

        eprintln!(
            "Generating miximages for {}: {} screenshots found",
            system_name,
            screenshots.len()
        );

        let bar = indicatif::ProgressBar::new(screenshots.len() as u64);
        bar.set_style(
            indicatif::ProgressStyle::default_bar()
                .template("{prefix} [{bar:30.cyan/blue}] {pos}/{len} {msg}")
                .unwrap_or_else(|_| indicatif::ProgressStyle::default_bar())
                .progress_chars("=> "),
        );
        bar.set_prefix(format!("Miximages {system_name}"));

        for ss_entry in &screenshots {
            let ss_path = ss_entry.path();
            let rom_stem = ss_path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();

            let miximage_path = media_organizer::get_media_path(
                &media_dir,
                &system_name,
                &MediaType::Miximage,
                &rom_stem,
            );

            // Skip if exists and not overwriting
            if miximage_path.exists() && !config.miximage.overwrite_existing {
                total_skipped += 1;
                bar.inc(1);
                continue;
            }

            // Find companion media
            let marquee_path = media_organizer::get_media_path(
                &media_dir,
                &system_name,
                &MediaType::Marquee,
                &rom_stem,
            );
            let box3d_path = media_organizer::get_media_path(
                &media_dir,
                &system_name,
                &MediaType::Box3d,
                &rom_stem,
            );
            let cover_path = media_organizer::get_media_path(
                &media_dir,
                &system_name,
                &MediaType::BoxFront,
                &rom_stem,
            );
            let pm_path = media_organizer::get_media_path(
                &media_dir,
                &system_name,
                &MediaType::PhysicalMedia,
                &rom_stem,
            );

            let box_path = if box3d_path.exists() {
                Some(box3d_path)
            } else if cover_path.exists() {
                Some(cover_path)
            } else {
                None
            };

            let config_clone = config.clone();
            let ss_path_clone = ss_path.clone();
            let marquee_exists = marquee_path.exists();
            let pm_exists = pm_path.exists();

            let result = tokio::task::spawn_blocking(move || {
                let generator = MiximageGenerator::new(
                    &config_clone.miximage,
                    config_clone.post_processing.remove_pillarboxes,
                );
                generator.generate(
                    &ss_path_clone,
                    if marquee_exists { Some(marquee_path.as_path()) } else { None },
                    box_path.as_deref(),
                    if pm_exists { Some(pm_path.as_path()) } else { None },
                    &miximage_path,
                )
            })
            .await?;

            match result {
                Ok(()) => total_generated += 1,
                Err(e) => tracing::warn!("Failed to generate miximage for {}: {}", rom_stem, e),
            }

            bar.inc(1);
        }

        bar.finish_and_clear();
    }

    eprintln!();
    eprintln!("=== Miximage Generation Summary ===");
    eprintln!("  Generated: {total_generated}");
    eprintln!("  Skipped:   {total_skipped}");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::game::{RomFile, RomType};
    use std::path::PathBuf;

    #[test]
    fn test_is_fatal_error_daily_quota() {
        let err = anyhow::Error::new(ApiError::DailyQuotaExceeded);
        assert!(is_fatal_error(&err));
    }

    #[test]
    fn test_is_fatal_error_rate_limited() {
        let err = anyhow::Error::new(ApiError::RateLimited);
        assert!(!is_fatal_error(&err));
    }

    #[test]
    fn test_is_fatal_error_game_not_found() {
        let err = anyhow::Error::new(ApiError::GameNotFound);
        assert!(!is_fatal_error(&err));
    }

    #[test]
    fn test_is_fatal_error_non_api_error() {
        let err = anyhow::anyhow!("some random error");
        assert!(!is_fatal_error(&err));
    }

    #[test]
    fn test_is_fatal_error_blacklisted() {
        let err = anyhow::Error::new(ApiError::SoftwareBlacklisted);
        assert!(is_fatal_error(&err));
    }

    #[test]
    fn test_is_fatal_error_invalid_credentials() {
        let err = anyhow::Error::new(ApiError::InvalidCredentials);
        assert!(is_fatal_error(&err));
    }

    #[test]
    fn test_hide_m3u_disc_entries_no_m3u() {
        let roms = vec![RomFile {
            path: PathBuf::from("game.zip"),
            filename: "game.zip".to_string(),
            file_size: 1000,
            rom_type: RomType::Rom,
            hash_target: None,
        }];
        let rom_dir = PathBuf::from("/tmp/nonexistent");
        let mut gamelist = crate::models::gamelist::GameList::default();
        hide_m3u_disc_entries(&roms, &rom_dir, &mut gamelist);
        assert!(gamelist.games.is_empty());
    }

    #[test]
    fn test_hide_m3u_disc_entries_hides_discs() {
        let tmp = tempfile::TempDir::new().unwrap();
        let rom_dir = tmp.path();

        std::fs::write(rom_dir.join("game (Disc 1).cue"), "disc1").unwrap();
        std::fs::write(rom_dir.join("game (Disc 2).cue"), "disc2").unwrap();

        let m3u_path = rom_dir.join("game.m3u");
        std::fs::write(&m3u_path, "game (Disc 1).cue\ngame (Disc 2).cue\n").unwrap();

        let roms = vec![RomFile {
            path: m3u_path,
            filename: "game.m3u".to_string(),
            file_size: 100,
            rom_type: RomType::Rom,
            hash_target: None,
        }];

        let mut gamelist = crate::models::gamelist::GameList {
            games: vec![
                crate::models::gamelist::GameListEntry {
                    path: "./game (Disc 1).cue".to_string(),
                    name: Some("Game Disc 1".to_string()),
                    ..Default::default()
                },
                crate::models::gamelist::GameListEntry {
                    path: "./game (Disc 2).cue".to_string(),
                    name: Some("Game Disc 2".to_string()),
                    ..Default::default()
                },
            ],
        };

        hide_m3u_disc_entries(&roms, rom_dir, &mut gamelist);
        assert_eq!(gamelist.games[0].hidden.as_deref(), Some("true"));
        assert_eq!(gamelist.games[1].hidden.as_deref(), Some("true"));
    }

    #[test]
    fn test_hide_m3u_disc_entries_no_double_hide() {
        let tmp = tempfile::TempDir::new().unwrap();
        let rom_dir = tmp.path();

        std::fs::write(rom_dir.join("game (Disc 1).cue"), "disc1").unwrap();
        let m3u_path = rom_dir.join("game.m3u");
        std::fs::write(&m3u_path, "game (Disc 1).cue\n").unwrap();

        let roms = vec![RomFile {
            path: m3u_path,
            filename: "game.m3u".to_string(),
            file_size: 50,
            rom_type: RomType::Rom,
            hash_target: None,
        }];

        let mut gamelist = crate::models::gamelist::GameList {
            games: vec![crate::models::gamelist::GameListEntry {
                path: "./game (Disc 1).cue".to_string(),
                hidden: Some("true".to_string()),
                ..Default::default()
            }],
        };

        hide_m3u_disc_entries(&roms, rom_dir, &mut gamelist);
        assert_eq!(gamelist.games[0].hidden.as_deref(), Some("true"));
    }
}

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::models::game::ScrapedGame;
use crate::models::media::MediaType;

/// Per-system cache statistics.
#[derive(Debug, Default)]
pub struct CacheStats {
    pub system: String,
    pub entry_count: usize,
    pub total_bytes: u64,
}

/// Local disk cache for scraped metadata and media files.
///
/// Layout:
/// ```text
/// <cache_dir>/<system>/<rom_hash>/
///   metadata.json
///   screenshot.png
///   cover.png
///   ...
/// ```
pub struct CacheStore {
    base_dir: PathBuf,
}

impl CacheStore {
    pub fn new(base_dir: &Path) -> Self {
        Self { base_dir: base_dir.to_path_buf() }
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// Directory for a single cached game: `<base>/<system>/<rom_hash>/`.
    fn game_dir(&self, system: &str, rom_hash: &str) -> PathBuf {
        self.base_dir.join(system).join(rom_hash)
    }

    /// Save scraped game metadata to cache.
    pub fn save_metadata(&self, system: &str, rom_hash: &str, game: &ScrapedGame) -> Result<()> {
        let dir = self.game_dir(system, rom_hash);
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create cache dir: {}", dir.display()))?;

        let path = dir.join("metadata.json");
        let json = serde_json::to_string_pretty(game)
            .with_context(|| "Failed to serialize game metadata")?;

        std::fs::write(&path, json)
            .with_context(|| format!("Failed to write cache metadata: {}", path.display()))?;

        Ok(())
    }

    /// Load cached game metadata.
    pub fn load_metadata(&self, system: &str, rom_hash: &str) -> Result<Option<ScrapedGame>> {
        let path = self.game_dir(system, rom_hash).join("metadata.json");
        if !path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read cache metadata: {}", path.display()))?;

        let game: ScrapedGame = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse cache metadata: {}", path.display()))?;

        Ok(Some(game))
    }

    /// Save a downloaded media file to cache.
    pub fn save_media(
        &self,
        system: &str,
        rom_hash: &str,
        media_type: &MediaType,
        data: &[u8],
    ) -> Result<()> {
        let dir = self.game_dir(system, rom_hash);
        std::fs::create_dir_all(&dir)?;

        let filename = format!("{}.{}", media_type.es_de_dirname(), media_type.extension());
        let path = dir.join(filename);
        std::fs::write(&path, data)
            .with_context(|| format!("Failed to write cached media: {}", path.display()))?;

        Ok(())
    }

    /// Get the path to a cached media file, if it exists.
    pub fn get_media_path(
        &self,
        system: &str,
        rom_hash: &str,
        media_type: &MediaType,
    ) -> Option<PathBuf> {
        let filename = format!("{}.{}", media_type.es_de_dirname(), media_type.extension());
        let path = self.game_dir(system, rom_hash).join(filename);
        if path.exists() {
            Some(path)
        } else {
            None
        }
    }

    /// Delete all cached data for a system.
    #[allow(dead_code)]
    pub fn clear_system(&self, system: &str) -> Result<()> {
        let dir = self.base_dir.join(system);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)
                .with_context(|| format!("Failed to clear cache for system: {}", system))?;
        }
        Ok(())
    }

    /// Collect cache statistics for all systems.
    pub fn all_stats(&self) -> Result<Vec<CacheStats>> {
        let mut stats = Vec::new();

        if !self.base_dir.exists() {
            return Ok(stats);
        }

        let entries = std::fs::read_dir(&self.base_dir)?;
        for entry in entries.filter_map(std::result::Result::ok) {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let system = entry.file_name().to_str().unwrap_or("").to_string();
            if system.is_empty() {
                continue;
            }

            let mut cs = CacheStats { system, ..Default::default() };

            // Count game directories and sum file sizes
            if let Ok(game_entries) = std::fs::read_dir(&path) {
                for game_entry in game_entries.filter_map(std::result::Result::ok) {
                    let game_path = game_entry.path();
                    if !game_path.is_dir() {
                        continue;
                    }
                    cs.entry_count += 1;
                    cs.total_bytes += dir_size(&game_path);
                }
            }

            stats.push(cs);
        }

        stats.sort_by(|a, b| a.system.cmp(&b.system));
        Ok(stats)
    }
}

/// Recursively compute total size of all files in a directory.
fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.filter_map(std::result::Result::ok) {
            let p = entry.path();
            if p.is_file() {
                total += p.metadata().map_or(0, |m| m.len());
            } else if p.is_dir() {
                total += dir_size(&p);
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_cache() -> (tempfile::TempDir, CacheStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = CacheStore::new(dir.path());
        (dir, store)
    }

    fn sample_game() -> ScrapedGame {
        ScrapedGame {
            game_id: 1234,
            rom_id: Some(5678),
            name: "Sonic The Hedgehog".to_string(),
            description: Some("A fast blue hedgehog".to_string()),
            rating: Some(0.8),
            release_date: Some("19910623T000000".to_string()),
            developer: Some("Sonic Team".to_string()),
            publisher: Some("Sega".to_string()),
            genre: Some("Platform".to_string()),
            players: Some("1".to_string()),
            system_id: 1,
            available_media: Vec::new(),
        }
    }

    #[test]
    fn test_save_and_load_metadata() {
        let (_dir, store) = temp_cache();
        let game = sample_game();

        store.save_metadata("megadrive", "ABCD1234", &game).unwrap();
        let loaded = store.load_metadata("megadrive", "ABCD1234").unwrap();

        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.game_id, 1234);
        assert_eq!(loaded.name, "Sonic The Hedgehog");
        assert_eq!(loaded.publisher.as_deref(), Some("Sega"));
    }

    #[test]
    fn test_load_metadata_missing() {
        let (_dir, store) = temp_cache();
        let loaded = store.load_metadata("snes", "NONEXIST").unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_save_and_get_media() {
        let (_dir, store) = temp_cache();
        let data = b"fake png data";

        store.save_media("snes", "HASH1", &MediaType::Screenshot, data).unwrap();

        let path = store.get_media_path("snes", "HASH1", &MediaType::Screenshot);
        assert!(path.is_some());
        let content = std::fs::read(path.unwrap()).unwrap();
        assert_eq!(content, data);
    }

    #[test]
    fn test_get_media_missing() {
        let (_dir, store) = temp_cache();
        assert!(store.get_media_path("snes", "NOPE", &MediaType::Screenshot).is_none());
    }

    #[test]
    fn test_clear_system() {
        let (_dir, store) = temp_cache();
        let game = sample_game();
        store.save_metadata("megadrive", "HASH1", &game).unwrap();
        store.save_metadata("megadrive", "HASH2", &game).unwrap();
        store.save_metadata("snes", "HASH3", &game).unwrap();

        store.clear_system("megadrive").unwrap();

        assert!(store.load_metadata("megadrive", "HASH1").unwrap().is_none());
        assert!(store.load_metadata("megadrive", "HASH2").unwrap().is_none());
        // snes untouched
        assert!(store.load_metadata("snes", "HASH3").unwrap().is_some());
    }

    #[test]
    fn test_all_stats() {
        let (_dir, store) = temp_cache();
        let game = sample_game();
        store.save_metadata("megadrive", "HASH1", &game).unwrap();
        store.save_metadata("megadrive", "HASH2", &game).unwrap();
        store.save_metadata("snes", "HASH3", &game).unwrap();

        let stats = store.all_stats().unwrap();
        assert_eq!(stats.len(), 2);

        let md = stats.iter().find(|s| s.system == "megadrive").unwrap();
        assert_eq!(md.entry_count, 2);
        assert!(md.total_bytes > 0);

        let snes = stats.iter().find(|s| s.system == "snes").unwrap();
        assert_eq!(snes.entry_count, 1);
    }

    #[test]
    fn test_all_stats_empty() {
        let (_dir, store) = temp_cache();
        let stats = store.all_stats().unwrap();
        assert!(stats.is_empty());
    }

    #[test]
    fn test_cache_dir_from_config() {
        let config = crate::config::Config::default();
        let dir = config.cache_directory();
        assert_eq!(dir, std::path::PathBuf::from("./cache"));
    }

    #[test]
    fn test_dir_size_nested() {
        let tmp = tempfile::tempdir().unwrap();
        let sub = tmp.path().join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(tmp.path().join("file1.txt"), "hello").unwrap();
        std::fs::write(sub.join("file2.txt"), "world!").unwrap();

        let size = dir_size(tmp.path());
        assert_eq!(size, 11); // "hello" (5) + "world!" (6)
    }

    #[test]
    fn test_dir_size_empty() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(dir_size(tmp.path()), 0);
    }

    #[test]
    fn test_dir_size_nonexistent() {
        let size = dir_size(Path::new("/tmp/nonexistent_dir_12345"));
        assert_eq!(size, 0);
    }

    #[test]
    fn test_all_stats_skips_files_in_base() {
        let (_dir, store) = temp_cache();
        // Create a file (not directory) in the base dir
        std::fs::write(store.base_dir().join("stray_file.txt"), "data").unwrap();

        let stats = store.all_stats().unwrap();
        // Files in base dir are skipped (only directories counted as systems)
        assert!(stats.is_empty());
    }

    #[test]
    fn test_clear_system_nonexistent() {
        let (_dir, store) = temp_cache();
        // Clearing a non-existent system should be OK
        store.clear_system("nonexistent").unwrap();
    }

    #[test]
    fn test_save_multiple_media_types() {
        let (_dir, store) = temp_cache();
        store.save_media("snes", "HASH1", &MediaType::Screenshot, b"screenshot").unwrap();
        store.save_media("snes", "HASH1", &MediaType::BoxFront, b"cover").unwrap();

        let ss = store.get_media_path("snes", "HASH1", &MediaType::Screenshot);
        let cover = store.get_media_path("snes", "HASH1", &MediaType::BoxFront);
        assert!(ss.is_some());
        assert!(cover.is_some());

        // Different files
        assert_ne!(ss.unwrap(), cover.unwrap());
    }

    #[test]
    fn test_all_stats_with_media() {
        let (_dir, store) = temp_cache();
        let game = sample_game();
        store.save_metadata("snes", "HASH1", &game).unwrap();
        store.save_media("snes", "HASH1", &MediaType::Screenshot, b"png data here").unwrap();

        let stats = store.all_stats().unwrap();
        assert_eq!(stats.len(), 1);
        let snes = &stats[0];
        assert_eq!(snes.entry_count, 1);
        // Bytes should include both metadata.json and screenshot file
        assert!(snes.total_bytes > 0);
    }
}

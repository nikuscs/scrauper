use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Entry in the cache index for a single ROM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheIndexEntry {
    pub hash: String,
    pub game_id: u64,
    pub cached_at: String,
}

/// Per-system cache index mapping ROM filenames to cache entries.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CacheIndex {
    #[serde(flatten)]
    pub entries: HashMap<String, CacheIndexEntry>,
}

impl CacheIndex {
    /// Load a system's cache index from disk.
    pub fn load(cache_dir: &Path, system: &str) -> Result<Self> {
        let path = Self::index_path(cache_dir, system);
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read cache index: {}", path.display()))?;

        let index: Self = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse cache index: {}", path.display()))?;

        Ok(index)
    }

    /// Save the cache index to disk.
    pub fn save(&self, cache_dir: &Path, system: &str) -> Result<()> {
        let path = Self::index_path(cache_dir, system);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)
            .with_context(|| "Failed to serialize cache index")?;

        std::fs::write(&path, json)
            .with_context(|| format!("Failed to write cache index: {}", path.display()))?;

        Ok(())
    }

    /// Look up a ROM by hash first, then by filename.
    #[allow(dead_code)]
    pub fn lookup(&self, hash: Option<&str>, filename: &str) -> Option<&CacheIndexEntry> {
        // Try hash lookup first
        if let Some(hash) = hash {
            let found = self.entries.values().find(|e| e.hash == hash);
            if found.is_some() {
                return found;
            }
        }

        // Fall back to filename lookup
        self.entries.get(filename)
    }

    /// Insert or update an entry.
    pub fn upsert(&mut self, filename: &str, hash: &str, game_id: u64) {
        let now = chrono::Utc::now().to_rfc3339();
        self.entries.insert(
            filename.to_string(),
            CacheIndexEntry { hash: hash.to_string(), game_id, cached_at: now },
        );
    }

    fn index_path(cache_dir: &Path, system: &str) -> PathBuf {
        cache_dir.join(system).join("index.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_upsert_and_lookup_by_filename() {
        let mut index = CacheIndex::default();
        index.upsert("Sonic.zip", "ABCD1234", 42);

        let entry = index.lookup(None, "Sonic.zip");
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert_eq!(entry.hash, "ABCD1234");
        assert_eq!(entry.game_id, 42);
        assert!(!entry.cached_at.is_empty());
    }

    #[test]
    fn test_lookup_by_hash() {
        let mut index = CacheIndex::default();
        index.upsert("Sonic.zip", "ABCD1234", 42);

        let entry = index.lookup(Some("ABCD1234"), "Other.zip");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().game_id, 42);
    }

    #[test]
    fn test_lookup_hash_priority() {
        let mut index = CacheIndex::default();
        index.upsert("Sonic.zip", "HASH_A", 1);
        index.upsert("Mario.zip", "HASH_B", 2);

        // Hash should take priority over filename
        let entry = index.lookup(Some("HASH_B"), "Sonic.zip");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().game_id, 2);
    }

    #[test]
    fn test_lookup_missing() {
        let index = CacheIndex::default();
        assert!(index.lookup(None, "missing.zip").is_none());
        assert!(index.lookup(Some("NOPE"), "missing.zip").is_none());
    }

    #[test]
    fn test_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let mut index = CacheIndex::default();
        index.upsert("Sonic.zip", "ABCD1234", 42);
        index.upsert("Mario.zip", "EFGH5678", 99);

        index.save(dir.path(), "megadrive").unwrap();

        let loaded = CacheIndex::load(dir.path(), "megadrive").unwrap();
        assert_eq!(loaded.entries.len(), 2);
        assert_eq!(loaded.entries["Sonic.zip"].game_id, 42);
        assert_eq!(loaded.entries["Mario.zip"].hash, "EFGH5678");
    }

    #[test]
    fn test_load_missing() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = CacheIndex::load(dir.path(), "nonexistent").unwrap();
        assert!(loaded.entries.is_empty());
    }

    #[test]
    fn test_upsert_overwrites() {
        let mut index = CacheIndex::default();
        index.upsert("Sonic.zip", "HASH_OLD", 1);
        index.upsert("Sonic.zip", "HASH_NEW", 2);

        assert_eq!(index.entries.len(), 1);
        assert_eq!(index.entries["Sonic.zip"].hash, "HASH_NEW");
        assert_eq!(index.entries["Sonic.zip"].game_id, 2);
    }
}

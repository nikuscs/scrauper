use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use globset::{Glob, GlobSetBuilder};
use walkdir::WalkDir;

use crate::config::Config;
use crate::models::game::{RomFile, RomHashes, RomType};
use crate::models::system::System;

/// Known BIOS filenames to skip during scanning.
const BIOS_FILES: &[&str] = &[
    "bios.bin",
    "bios.rom",
    "bios_CD_E.bin",
    "bios_CD_J.bin",
    "bios_CD_U.bin",
    "disksys.rom",
    "gba_bios.bin",
    "pgm.cal",
    "scph1001.bin",
    "scph5500.bin",
    "scph5501.bin",
    "scph5502.bin",
    "scph7003.bin",
    "scph10000.bin",
    "scph39001.bin",
    "SCPH-70012.bin",
    "dc_boot.bin",
    "dc_flash.bin",
    "neogeo.zip",
    "qsound.zip",
    "kick13.rom",
    "kick31.rom",
    "kick34005.A500",
];

/// ISO-based ROM extensions (determines RomType).
const ISO_EXTENSIONS: &[&str] = &[
    "iso", "bin", "cue", "img", "mdf", "mds", "cdi", "chd", "gdi", "cso", "pbp", "rvz", "wbfs",
    "nrg", "gcz", "wia",
];

/// Scan a single system directory for ROM files.
pub fn scan_system(
    rom_dir: &Path,
    extensions: &[String],
    exclude_patterns: &[String],
) -> Result<Vec<RomFile>> {
    if !rom_dir.exists() {
        return Ok(Vec::new());
    }

    // Build exclude glob set
    let mut exclude_builder = GlobSetBuilder::new();
    for pattern in exclude_patterns {
        if let Ok(glob) = Glob::new(pattern) {
            exclude_builder.add(glob);
        }
    }
    let exclude_set = exclude_builder.build().unwrap_or_default();

    let ext_set: std::collections::HashSet<String> =
        extensions.iter().map(|e| e.to_lowercase()).collect();

    let mut rom_files = Vec::new();
    let mut m3u_files = Vec::new();

    for entry in WalkDir::new(rom_dir).min_depth(1).into_iter().filter_map(std::result::Result::ok)
    {
        let path = entry.path();

        // Skip directories
        if path.is_dir() {
            continue;
        }

        // Apply exclude patterns
        if let Ok(relative) = path.strip_prefix(rom_dir) {
            if exclude_set.is_match(relative) || exclude_set.is_match(path) {
                continue;
            }
        }

        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        // Skip BIOS files
        if is_bios_file(&filename) {
            tracing::debug!("Skipping BIOS file: {}", filename);
            continue;
        }

        let ext =
            path.extension().and_then(|e| e.to_str()).map(str::to_lowercase).unwrap_or_default();

        // Collect M3U files separately for multi-disc handling
        if ext == "m3u" {
            m3u_files.push(path.to_path_buf());
            continue;
        }

        // Check if extension matches
        if !ext_set.contains(&ext) {
            continue;
        }

        let metadata = std::fs::metadata(path)?;
        let rom_type =
            if ISO_EXTENSIONS.contains(&ext.as_str()) { RomType::Iso } else { RomType::Rom };

        rom_files.push(RomFile {
            path: path.to_path_buf(),
            filename,
            file_size: metadata.len(),
            rom_type,
            hash_target: None,
        });
    }

    // Process M3U files: create single entries, suppress individual disc files
    for m3u_path in &m3u_files {
        match parse_m3u_validated(m3u_path, rom_dir) {
            Ok((first_disc, _disc_paths)) => {
                let filename = m3u_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown.m3u")
                    .to_string();

                let metadata = std::fs::metadata(m3u_path)?;

                // Remove individual disc files that are referenced by this M3U
                if let Ok(disc_paths) = get_m3u_disc_paths(m3u_path, rom_dir) {
                    rom_files.retain(|rf| !disc_paths.contains(&rf.path));
                }

                rom_files.push(RomFile {
                    path: m3u_path.clone(),
                    filename,
                    file_size: metadata.len(),
                    rom_type: RomType::Iso,
                    hash_target: Some(first_disc),
                });
            }
            Err(e) => {
                tracing::warn!("Skipping invalid M3U {}: {}", m3u_path.display(), e);
            }
        }
    }

    Ok(rom_files)
}

/// Scan all system directories and map to system IDs.
pub fn scan_all_systems(
    config: &Config,
    systems: &[System],
    system_filter: Option<&str>,
) -> Result<HashMap<u64, (String, Vec<RomFile>)>> {
    let rom_dir = config.rom_directory();
    let mut result = HashMap::new();

    if !rom_dir.exists() {
        anyhow::bail!("ROM directory not found: {}", rom_dir.display());
    }

    // List subdirectories in ROM dir — each should be a system
    let entries: Vec<_> = std::fs::read_dir(&rom_dir)?
        .filter_map(std::result::Result::ok)
        .filter(|e| e.path().is_dir())
        .collect();

    for entry in entries {
        let dir_name = entry.file_name().to_str().unwrap_or("").to_string();

        // Apply system filter if provided
        if let Some(filter) = system_filter {
            if dir_name.to_lowercase() != filter.to_lowercase() {
                continue;
            }
        }

        // Also check config systems filter
        if !config.scraping.systems.is_empty()
            && !config.scraping.systems.iter().any(|s| s.to_lowercase() == dir_name.to_lowercase())
        {
            continue;
        }

        // Find matching system by folder name
        let Some(system) = find_system_by_folder(&dir_name, systems) else {
            tracing::warn!("No matching system found for folder: {}", dir_name);
            continue;
        };

        let roms =
            scan_system(&entry.path(), &system.extensions, &config.scraping.exclude_patterns)?;

        if !roms.is_empty() {
            tracing::info!("Found {} ROMs in {} (system ID: {})", roms.len(), dir_name, system.id);
            result.insert(system.id, (dir_name, roms));
        }
    }

    Ok(result)
}

/// Hash a ROM file computing CRC32, MD5, and SHA1 simultaneously.
pub async fn hash_rom(path: &Path, max_size_mb: u64) -> Result<Option<RomHashes>> {
    let path = path.to_path_buf();
    let result = tokio::task::spawn_blocking(move || hash_rom_blocking(&path, max_size_mb)).await?;
    result
}

fn hash_rom_blocking(path: &Path, max_size_mb: u64) -> Result<Option<RomHashes>> {
    let metadata = std::fs::metadata(path)
        .with_context(|| format!("Failed to read ROM file: {}", path.display()))?;

    let file_size = metadata.len();

    // Skip hashing if file is too large
    if file_size > max_size_mb * 1024 * 1024 {
        tracing::debug!(
            "Skipping hash for {} ({}MB > {}MB limit)",
            path.display(),
            file_size / (1024 * 1024),
            max_size_mb
        );
        return Ok(None);
    }

    let mut file = std::fs::File::open(path)?;
    let mut crc_hasher = crc32fast::Hasher::new();
    let mut md5_hasher = <md5::Md5 as digest::Digest>::new();
    let mut sha1_hasher = <sha1::Sha1 as digest::Digest>::new();

    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        crc_hasher.update(&buf[..n]);
        <md5::Md5 as digest::Digest>::update(&mut md5_hasher, &buf[..n]);
        <sha1::Sha1 as digest::Digest>::update(&mut sha1_hasher, &buf[..n]);
    }

    let crc32 = format!("{:08X}", crc_hasher.finalize());
    let md5 = format!("{:x}", <md5::Md5 as digest::Digest>::finalize(md5_hasher));
    let sha1 = format!("{:x}", <sha1::Sha1 as digest::Digest>::finalize(sha1_hasher));

    Ok(Some(RomHashes { crc32, md5, sha1, file_size }))
}

/// Parse an M3U file, validate all disc entries, and return (first_disc, all_disc_paths).
/// Warns about missing disc files but succeeds if at least one exists.
fn parse_m3u_validated(m3u_path: &Path, rom_dir: &Path) -> Result<(PathBuf, Vec<PathBuf>)> {
    let content = std::fs::read_to_string(m3u_path)
        .with_context(|| format!("Failed to read M3U file: {}", m3u_path.display()))?;

    let m3u_parent = m3u_path.parent().unwrap_or(rom_dir);
    let mut found_discs = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let disc_path = m3u_parent.join(line);
        if disc_path.exists() {
            found_discs.push(disc_path);
        } else {
            tracing::warn!(
                "M3U '{}' references missing disc: {}",
                m3u_path.display(),
                disc_path.display()
            );
        }
    }

    if found_discs.is_empty() {
        anyhow::bail!("No valid disc files found in M3U: {}", m3u_path.display());
    }

    let first = found_discs[0].clone();
    Ok((first, found_discs))
}

/// Get all disc file paths referenced in an M3U.
pub fn get_m3u_disc_paths(m3u_path: &Path, rom_dir: &Path) -> Result<Vec<PathBuf>> {
    let content = std::fs::read_to_string(m3u_path)?;
    let m3u_parent = m3u_path.parent().unwrap_or(rom_dir);

    Ok(content
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| m3u_parent.join(l))
        .filter(|p| p.exists())
        .collect())
}

/// Find a system by ES-DE folder name (case-insensitive match against system name).
fn find_system_by_folder(folder_name: &str, systems: &[System]) -> Option<System> {
    let folder_lower = folder_name.to_lowercase();

    // Direct name match (most common)
    systems.iter().find(|s| s.name.to_lowercase() == folder_lower).cloned()
}

fn is_bios_file(filename: &str) -> bool {
    let lower = filename.to_lowercase();
    BIOS_FILES.iter().any(|b| b.to_lowercase() == lower)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_bios_file() {
        assert!(is_bios_file("bios.bin"));
        assert!(is_bios_file("BIOS.BIN"));
        assert!(is_bios_file("scph1001.bin"));
        assert!(is_bios_file("neogeo.zip"));
        assert!(!is_bios_file("Sonic.zip"));
        assert!(!is_bios_file("game.rom"));
    }

    #[test]
    fn test_find_system_by_folder() {
        let systems = vec![
            System {
                id: 1,
                name: "megadrive".to_string(),
                extensions: vec!["bin".to_string(), "gen".to_string()],
                system_type: "Console".to_string(),
                company: Some("Sega".to_string()),
            },
            System {
                id: 4,
                name: "snes".to_string(),
                extensions: vec!["sfc".to_string(), "smc".to_string()],
                system_type: "Console".to_string(),
                company: Some("Nintendo".to_string()),
            },
        ];

        let found = find_system_by_folder("snes", &systems);
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, 4);

        let found = find_system_by_folder("SNES", &systems);
        assert!(found.is_some());

        let found = find_system_by_folder("n64", &systems);
        assert!(found.is_none());
    }

    #[test]
    fn test_scan_system_empty_dir() {
        let dir = std::env::temp_dir().join("scrauper_test_scan_empty");
        std::fs::create_dir_all(&dir).unwrap();
        let result = scan_system(&dir, &["zip".to_string()], &[]).unwrap();
        assert!(result.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_scan_system_with_roms() {
        let dir = std::env::temp_dir().join("scrauper_test_scan_roms");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(dir.join("Sonic.zip"), b"fake rom").unwrap();
        std::fs::write(dir.join("Mario.zip"), b"fake rom 2").unwrap();
        std::fs::write(dir.join("readme.txt"), b"not a rom").unwrap();

        let result = scan_system(&dir, &["zip".to_string()], &[]).unwrap();
        assert_eq!(result.len(), 2);

        let filenames: Vec<&str> = result.iter().map(|r| r.filename.as_str()).collect();
        assert!(filenames.contains(&"Sonic.zip"));
        assert!(filenames.contains(&"Mario.zip"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_scan_system_skips_bios() {
        let dir = std::env::temp_dir().join("scrauper_test_scan_bios");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(dir.join("bios.bin"), b"bios data").unwrap();
        std::fs::write(dir.join("Game.bin"), b"game data").unwrap();

        let result = scan_system(&dir, &["bin".to_string()], &[]).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].filename, "Game.bin");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_scan_system_nonexistent_dir() {
        let result =
            scan_system(Path::new("/nonexistent_dir_scrauper"), &["zip".to_string()], &[]).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_scan_system_iso_detection() {
        let dir = std::env::temp_dir().join("scrauper_test_scan_iso");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(dir.join("Game.iso"), b"iso data").unwrap();
        std::fs::write(dir.join("Game.rom"), b"rom data").unwrap();

        let result = scan_system(&dir, &["iso".to_string(), "rom".to_string()], &[]).unwrap();
        assert_eq!(result.len(), 2);

        let iso_file = result.iter().find(|r| r.filename == "Game.iso").unwrap();
        assert!(matches!(iso_file.rom_type, RomType::Iso));

        let rom_file = result.iter().find(|r| r.filename == "Game.rom").unwrap();
        assert!(matches!(rom_file.rom_type, RomType::Rom));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_hash_rom_blocking() {
        let dir = std::env::temp_dir().join("scrauper_test_hash");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let path = dir.join("test.bin");
        std::fs::write(&path, b"hello world").unwrap();

        let result = hash_rom_blocking(&path, 300).unwrap();
        assert!(result.is_some());
        let hashes = result.unwrap();
        assert!(!hashes.crc32.is_empty());
        assert!(!hashes.md5.is_empty());
        assert!(!hashes.sha1.is_empty());
        assert_eq!(hashes.file_size, 11);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_hash_rom_blocking_too_large() {
        let dir = std::env::temp_dir().join("scrauper_test_hash_large");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let path = dir.join("large.bin");
        std::fs::write(&path, vec![0u8; 1024]).unwrap();

        let result = hash_rom_blocking(&path, 0).unwrap();
        assert!(result.is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_m3u_scan_creates_single_entry() {
        let dir = std::env::temp_dir().join("scrauper_test_m3u_scan");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Create disc files
        std::fs::write(dir.join("Game (Disc 1).chd"), b"disc1").unwrap();
        std::fs::write(dir.join("Game (Disc 2).chd"), b"disc2").unwrap();
        // Create M3U
        std::fs::write(dir.join("Game.m3u"), "Game (Disc 1).chd\nGame (Disc 2).chd\n").unwrap();

        let result = scan_system(&dir, &["chd".to_string()], &[]).unwrap();

        // Should have 1 M3U entry, individual discs suppressed
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].filename, "Game.m3u");
        assert!(result[0].hash_target.is_some());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_m3u_disc_paths() {
        let dir = std::env::temp_dir().join("scrauper_test_m3u_paths");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(dir.join("Disc1.chd"), b"d1").unwrap();
        std::fs::write(dir.join("Disc2.chd"), b"d2").unwrap();

        let m3u = dir.join("Game.m3u");
        std::fs::write(&m3u, "Disc1.chd\nDisc2.chd\n").unwrap();

        let paths = get_m3u_disc_paths(&m3u, &dir).unwrap();
        assert_eq!(paths.len(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_m3u_missing_disc_still_works() {
        let dir = std::env::temp_dir().join("scrauper_test_m3u_missing");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Only create first disc
        std::fs::write(dir.join("Disc1.chd"), b"d1").unwrap();

        let m3u = dir.join("Game.m3u");
        std::fs::write(&m3u, "Disc1.chd\nDisc2_Missing.chd\n").unwrap();

        // Should still succeed (first disc exists)
        let result = scan_system(&dir, &["chd".to_string()], &[]).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].filename, "Game.m3u");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_m3u_no_valid_discs() {
        let dir = std::env::temp_dir().join("scrauper_test_m3u_empty");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let m3u = dir.join("Game.m3u");
        std::fs::write(&m3u, "NonExistent1.chd\nNonExistent2.chd\n").unwrap();

        // M3U with no valid discs should be skipped
        let result = scan_system(&dir, &["chd".to_string()], &[]).unwrap();
        assert!(result.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_scan_system_exclude_patterns() {
        let dir = std::env::temp_dir().join("scrauper_test_scan_exclude");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(dir.join("Sonic.zip"), b"rom").unwrap();
        std::fs::write(dir.join(".DS_Store"), b"junk").unwrap();
        std::fs::write(dir.join("thumbs.db"), b"junk").unwrap();

        let result =
            scan_system(&dir, &["zip".to_string(), "db".to_string()], &["thumbs.db".to_string()])
                .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].filename, "Sonic.zip");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_is_bios_file_case_insensitive() {
        assert!(is_bios_file("BIOS.BIN"));
        assert!(is_bios_file("Bios.Bin"));
        assert!(is_bios_file("SCPH-70012.bin"));
        assert!(is_bios_file("dc_boot.bin"));
        assert!(is_bios_file("DC_BOOT.BIN"));
    }

    #[test]
    fn test_is_bios_file_non_bios() {
        assert!(!is_bios_file("game.bin"));
        assert!(!is_bios_file("bios_helper.txt"));
        assert!(!is_bios_file(""));
    }

    #[test]
    fn test_scan_system_multiple_extensions() {
        let dir = std::env::temp_dir().join("scrauper_test_scan_multi_ext");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(dir.join("Game1.sfc"), b"rom1").unwrap();
        std::fs::write(dir.join("Game2.smc"), b"rom2").unwrap();
        std::fs::write(dir.join("Game3.zip"), b"rom3").unwrap();
        std::fs::write(dir.join("Game4.txt"), b"not").unwrap();

        let result =
            scan_system(&dir, &["sfc".to_string(), "smc".to_string(), "zip".to_string()], &[])
                .unwrap();
        assert_eq!(result.len(), 3);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_m3u_with_comments() {
        let dir = std::env::temp_dir().join("scrauper_test_m3u_comments");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(dir.join("Disc1.chd"), b"d1").unwrap();

        let m3u = dir.join("Game.m3u");
        std::fs::write(&m3u, "# This is a comment\n\nDisc1.chd\n# Another comment\n").unwrap();

        let paths = get_m3u_disc_paths(&m3u, &dir).unwrap();
        assert_eq!(paths.len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_find_system_by_folder_case_insensitive() {
        let systems = vec![System {
            id: 1,
            name: "MegaDrive".to_string(),
            extensions: vec![],
            system_type: "Console".to_string(),
            company: None,
        }];

        assert!(find_system_by_folder("megadrive", &systems).is_some());
        assert!(find_system_by_folder("MEGADRIVE", &systems).is_some());
        assert!(find_system_by_folder("MegaDrive", &systems).is_some());
    }

    #[test]
    fn test_find_system_by_folder_empty_list() {
        let systems: Vec<System> = vec![];
        assert!(find_system_by_folder("snes", &systems).is_none());
    }

    #[test]
    fn test_hash_rom_blocking_deterministic() {
        let dir = std::env::temp_dir().join("scrauper_test_hash_det");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let path = dir.join("test.bin");
        std::fs::write(&path, b"deterministic content").unwrap();

        let h1 = hash_rom_blocking(&path, 300).unwrap().unwrap();
        let h2 = hash_rom_blocking(&path, 300).unwrap().unwrap();
        assert_eq!(h1.crc32, h2.crc32);
        assert_eq!(h1.md5, h2.md5);
        assert_eq!(h1.sha1, h2.sha1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_scan_system_rom_type_chd() {
        let dir = std::env::temp_dir().join("scrauper_test_scan_chd");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(dir.join("Game.chd"), b"chd data").unwrap();

        let result = scan_system(&dir, &["chd".to_string()], &[]).unwrap();
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0].rom_type, RomType::Iso));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_scan_system_nested_files() {
        let tmp = tempfile::TempDir::new().unwrap();
        // Create a nested ROM file
        let subdir = tmp.path().join("subdir");
        std::fs::create_dir_all(&subdir).unwrap();
        std::fs::write(subdir.join("game.sfc"), b"rom data").unwrap();

        let roms = scan_system(tmp.path(), &["sfc".to_string()], &[]).unwrap();
        // Files in subdirectories should still be found (WalkDir recurses)
        assert_eq!(roms.len(), 1);
    }

    #[test]
    fn test_hash_rom_blocking_file_too_large() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("large.sfc");
        // Write a small file but set the size limit to 0 MB
        std::fs::write(&path, b"data").unwrap();
        let hashes = hash_rom_blocking(&path, 0).unwrap();
        assert!(hashes.is_none());
    }

    #[test]
    fn test_hash_rom_blocking_nonexistent() {
        let result = hash_rom_blocking(Path::new("/tmp/nonexistent_rom_12345.sfc"), 100);
        // File doesn't exist, should error
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_m3u_empty_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let m3u_path = tmp.path().join("test.m3u");
        std::fs::write(&m3u_path, "").unwrap();

        let result = parse_m3u_validated(&m3u_path, tmp.path());
        // Empty M3U has no valid disc entries, should error
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_m3u_all_comments() {
        let tmp = tempfile::TempDir::new().unwrap();
        let m3u_path = tmp.path().join("test.m3u");
        std::fs::write(&m3u_path, "# comment 1\n# comment 2\n").unwrap();

        let result = parse_m3u_validated(&m3u_path, tmp.path());
        // All lines are comments - no valid disc entries, should error
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_m3u_all_discs_missing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let m3u_path = tmp.path().join("test.m3u");
        std::fs::write(&m3u_path, "disc1.chd\ndisc2.chd\n").unwrap();

        let result = parse_m3u_validated(&m3u_path, tmp.path());
        // Referenced files don't exist, should error
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_hash_rom_async_too_large() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("large.sfc");
        std::fs::write(&path, b"data").unwrap();
        let hashes = hash_rom(&path, 0).await.unwrap();
        assert!(hashes.is_none());
    }

    fn test_config() -> Config {
        let mut config = Config::default();
        config.credentials.devid = "test".to_string();
        config.credentials.devpassword = "test".to_string();
        config
    }

    #[test]
    fn test_scan_all_systems_nonexistent_rom_dir() {
        let mut config = test_config();
        config.paths.rom_directory = "/nonexistent_scrauper_dir_12345".to_string();
        let systems: Vec<System> = vec![];
        let result = scan_all_systems(&config, &systems, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_scan_all_systems_empty_rom_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut config = test_config();
        config.paths.rom_directory = tmp.path().to_str().unwrap().to_string();
        let systems = vec![System {
            id: 1,
            name: "snes".to_string(),
            extensions: vec!["sfc".to_string()],
            system_type: "Console".to_string(),
            company: None,
        }];
        let result = scan_all_systems(&config, &systems, None).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_scan_all_systems_with_roms() {
        let tmp = tempfile::TempDir::new().unwrap();
        let snes_dir = tmp.path().join("snes");
        std::fs::create_dir_all(&snes_dir).unwrap();
        std::fs::write(snes_dir.join("Game.sfc"), b"rom").unwrap();

        let mut config = test_config();
        config.paths.rom_directory = tmp.path().to_str().unwrap().to_string();
        let systems = vec![System {
            id: 4,
            name: "snes".to_string(),
            extensions: vec!["sfc".to_string()],
            system_type: "Console".to_string(),
            company: Some("Nintendo".to_string()),
        }];
        let result = scan_all_systems(&config, &systems, None).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result.contains_key(&4));
        let (name, roms) = &result[&4];
        assert_eq!(name, "snes");
        assert_eq!(roms.len(), 1);
    }

    #[test]
    fn test_scan_all_systems_with_filter() {
        let tmp = tempfile::TempDir::new().unwrap();
        let snes_dir = tmp.path().join("snes");
        let md_dir = tmp.path().join("megadrive");
        std::fs::create_dir_all(&snes_dir).unwrap();
        std::fs::create_dir_all(&md_dir).unwrap();
        std::fs::write(snes_dir.join("Game.sfc"), b"rom").unwrap();
        std::fs::write(md_dir.join("Sonic.bin"), b"rom").unwrap();

        let mut config = test_config();
        config.paths.rom_directory = tmp.path().to_str().unwrap().to_string();
        let systems = vec![
            System {
                id: 4,
                name: "snes".to_string(),
                extensions: vec!["sfc".to_string()],
                system_type: "Console".to_string(),
                company: None,
            },
            System {
                id: 1,
                name: "megadrive".to_string(),
                extensions: vec!["bin".to_string()],
                system_type: "Console".to_string(),
                company: None,
            },
        ];
        // Filter to only snes
        let result = scan_all_systems(&config, &systems, Some("snes")).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result.contains_key(&4));
    }

    #[test]
    fn test_scan_all_systems_config_systems_filter() {
        let tmp = tempfile::TempDir::new().unwrap();
        let snes_dir = tmp.path().join("snes");
        let md_dir = tmp.path().join("megadrive");
        std::fs::create_dir_all(&snes_dir).unwrap();
        std::fs::create_dir_all(&md_dir).unwrap();
        std::fs::write(snes_dir.join("Game.sfc"), b"rom").unwrap();
        std::fs::write(md_dir.join("Sonic.bin"), b"rom").unwrap();

        let mut config = test_config();
        config.paths.rom_directory = tmp.path().to_str().unwrap().to_string();
        config.scraping.systems = vec!["megadrive".to_string()];
        let systems = vec![
            System {
                id: 4,
                name: "snes".to_string(),
                extensions: vec!["sfc".to_string()],
                system_type: "Console".to_string(),
                company: None,
            },
            System {
                id: 1,
                name: "megadrive".to_string(),
                extensions: vec!["bin".to_string()],
                system_type: "Console".to_string(),
                company: None,
            },
        ];
        let result = scan_all_systems(&config, &systems, None).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result.contains_key(&1));
    }

    #[test]
    fn test_scan_all_systems_no_matching_system() {
        let tmp = tempfile::TempDir::new().unwrap();
        let unknown_dir = tmp.path().join("unknownsystem");
        std::fs::create_dir_all(&unknown_dir).unwrap();
        std::fs::write(unknown_dir.join("game.zip"), b"rom").unwrap();

        let mut config = test_config();
        config.paths.rom_directory = tmp.path().to_str().unwrap().to_string();
        let systems = vec![System {
            id: 4,
            name: "snes".to_string(),
            extensions: vec!["sfc".to_string()],
            system_type: "Console".to_string(),
            company: None,
        }];
        // Folder "unknownsystem" doesn't match any system
        let result = scan_all_systems(&config, &systems, None).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_scan_all_systems_empty_dir_skipped() {
        let tmp = tempfile::TempDir::new().unwrap();
        let snes_dir = tmp.path().join("snes");
        std::fs::create_dir_all(&snes_dir).unwrap();
        // No ROM files

        let mut config = test_config();
        config.paths.rom_directory = tmp.path().to_str().unwrap().to_string();
        let systems = vec![System {
            id: 4,
            name: "snes".to_string(),
            extensions: vec!["sfc".to_string()],
            system_type: "Console".to_string(),
            company: None,
        }];
        // snes dir exists but has no ROMs
        let result = scan_all_systems(&config, &systems, None).unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_hash_rom_async_success() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.bin");
        std::fs::write(&path, b"hello").unwrap();
        let hashes = hash_rom(&path, 300).await.unwrap();
        assert!(hashes.is_some());
        let h = hashes.unwrap();
        assert_eq!(h.file_size, 5);
        assert!(!h.crc32.is_empty());
    }

    #[test]
    fn test_get_m3u_disc_paths_empty() {
        let tmp = tempfile::TempDir::new().unwrap();
        let m3u = tmp.path().join("empty.m3u");
        std::fs::write(&m3u, "").unwrap();
        let paths = get_m3u_disc_paths(&m3u, tmp.path()).unwrap();
        assert!(paths.is_empty());
    }
}

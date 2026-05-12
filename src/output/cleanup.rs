use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Result;
use walkdir::WalkDir;

use crate::config::Config;

/// Options for the cleanup command.
pub struct CleanupOptions {
    pub dry_run: bool,
    pub auto_confirm: bool,
}

/// Summary of cleanup results.
#[allow(dead_code)]
pub struct CleanupSummary {
    pub orphans_found: usize,
    pub orphans_deleted: usize,
    pub bytes_freed: u64,
}

/// All media subdirectory names to scan.
const MEDIA_SUBDIRS: &[&str] = &[
    "screenshots",
    "titlescreens",
    "videos",
    "covers",
    "backcovers",
    "3dboxes",
    "marquees",
    "physicalmedia",
    "fanart",
    "manuals",
    "miximages",
];

/// Find orphaned media files (media without corresponding ROMs) and optionally delete them.
#[allow(clippy::print_stderr)]
pub fn run_cleanup(config: &Config, options: &CleanupOptions) -> Result<CleanupSummary> {
    let rom_dir = config.rom_directory();
    let media_dir = config.media_directory();

    if !media_dir.exists() {
        anyhow::bail!("Media directory not found: {}", media_dir.display());
    }

    let mut total_orphans = Vec::new();

    // Scan each system's media directory
    let entries: Vec<_> = std::fs::read_dir(&media_dir)?
        .filter_map(std::result::Result::ok)
        .filter(|e| e.path().is_dir())
        .collect();

    for entry in entries {
        let system_name = entry.file_name().to_str().unwrap_or("").to_string();
        if system_name.is_empty() {
            continue;
        }

        let system_rom_dir = rom_dir.join(&system_name);

        // Collect known ROM stems for this system
        let rom_stems = collect_rom_stems(&system_rom_dir);

        // Scan each media subdirectory
        for subdir_name in MEDIA_SUBDIRS {
            let subdir_path = media_dir.join(&system_name).join(subdir_name);
            if !subdir_path.exists() {
                continue;
            }

            let orphans = find_orphans_in_dir(&subdir_path, &rom_stems)?;
            if !orphans.is_empty() {
                tracing::info!(
                    "{}/{}: Found {} orphaned file(s)",
                    system_name,
                    subdir_name,
                    orphans.len()
                );
            }
            total_orphans.extend(orphans);
        }
    }

    if total_orphans.is_empty() {
        eprintln!("No orphaned media files found.");
        return Ok(CleanupSummary { orphans_found: 0, orphans_deleted: 0, bytes_freed: 0 });
    }

    // Print orphan list
    eprintln!("Found {} orphaned media file(s):", total_orphans.len());
    let mut total_size = 0u64;
    for path in &total_orphans {
        let size = path.metadata().map_or(0, |m| m.len());
        total_size += size;
        let rel = path
            .strip_prefix(&media_dir)
            .map_or_else(|_| path.display().to_string(), |p| p.display().to_string());
        eprintln!("  {} ({})", rel, format_bytes(size));
    }
    eprintln!();
    eprintln!("Total: {} files, {}", total_orphans.len(), format_bytes(total_size));

    if options.dry_run {
        eprintln!();
        eprintln!("Dry run — no files deleted.");
        return Ok(CleanupSummary {
            orphans_found: total_orphans.len(),
            orphans_deleted: 0,
            bytes_freed: 0,
        });
    }

    // Confirm unless --yes
    if !options.auto_confirm {
        eprintln!();
        eprintln!("Delete these files? Pass --yes to confirm.");
        return Ok(CleanupSummary {
            orphans_found: total_orphans.len(),
            orphans_deleted: 0,
            bytes_freed: 0,
        });
    }

    // Delete orphaned files
    let mut deleted = 0;
    let mut freed = 0u64;
    for path in &total_orphans {
        let size = path.metadata().map_or(0, |m| m.len());
        match std::fs::remove_file(path) {
            Ok(()) => {
                deleted += 1;
                freed += size;
                tracing::debug!("Deleted: {}", path.display());
            }
            Err(e) => {
                tracing::warn!("Failed to delete {}: {}", path.display(), e);
            }
        }
    }

    eprintln!();
    eprintln!("Deleted {} file(s), freed {}", deleted, format_bytes(freed));

    Ok(CleanupSummary {
        orphans_found: total_orphans.len(),
        orphans_deleted: deleted,
        bytes_freed: freed,
    })
}

/// Collect all ROM file stems (filename without extension) from a system directory.
fn collect_rom_stems(rom_dir: &Path) -> HashSet<String> {
    let mut stems = HashSet::new();

    if !rom_dir.exists() {
        return stems;
    }

    for entry in WalkDir::new(rom_dir).min_depth(1).into_iter().filter_map(std::result::Result::ok)
    {
        let path = entry.path();
        if path.is_file() {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                stems.insert(stem.to_string());
            }
        }
    }

    stems
}

/// Find media files in a directory that don't have a matching ROM stem.
fn find_orphans_in_dir(dir: &Path, rom_stems: &HashSet<String>) -> Result<Vec<PathBuf>> {
    let mut orphans = Vec::new();

    for entry in std::fs::read_dir(dir)?.filter_map(std::result::Result::ok) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();

        if stem.is_empty() {
            continue;
        }

        if !rom_stems.contains(&stem) {
            orphans.push(path);
        }
    }

    Ok(orphans)
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn test_collect_rom_stems() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Sonic.zip"), b"data").unwrap();
        std::fs::write(dir.path().join("Mario.sfc"), b"data").unwrap();
        std::fs::write(dir.path().join("readme.txt"), b"data").unwrap();

        let stems = collect_rom_stems(dir.path());
        assert!(stems.contains("Sonic"));
        assert!(stems.contains("Mario"));
        assert!(stems.contains("readme"));
    }

    #[test]
    fn test_collect_rom_stems_empty() {
        let stems = collect_rom_stems(Path::new("/nonexistent_dir_scrauper"));
        assert!(stems.is_empty());
    }

    #[test]
    fn test_find_orphans_in_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Sonic.png"), b"img").unwrap();
        std::fs::write(dir.path().join("Mario.png"), b"img").unwrap();
        std::fs::write(dir.path().join("Deleted.png"), b"img").unwrap();

        let mut rom_stems = HashSet::new();
        rom_stems.insert("Sonic".to_string());
        rom_stems.insert("Mario".to_string());

        let orphans = find_orphans_in_dir(dir.path(), &rom_stems).unwrap();
        assert_eq!(orphans.len(), 1);
        assert!(orphans[0].file_name().unwrap().to_str().unwrap().contains("Deleted"));
    }

    #[test]
    fn test_find_orphans_none() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Sonic.png"), b"img").unwrap();

        let mut rom_stems = HashSet::new();
        rom_stems.insert("Sonic".to_string());

        let orphans = find_orphans_in_dir(dir.path(), &rom_stems).unwrap();
        assert!(orphans.is_empty());
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1_048_576), "1.0 MB");
        assert_eq!(format_bytes(1_073_741_824), "1.0 GB");
    }

    #[test]
    fn test_collect_rom_stems_nested() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("subdir");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(dir.path().join("TopLevel.zip"), b"data").unwrap();
        std::fs::write(sub.join("Nested.sfc"), b"data").unwrap();

        let stems = collect_rom_stems(dir.path());
        assert!(stems.contains("TopLevel"));
        assert!(stems.contains("Nested"));
    }

    #[test]
    fn test_find_orphans_skips_directories() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("subdir");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(dir.path().join("Sonic.png"), b"img").unwrap();

        let mut rom_stems = HashSet::new();
        rom_stems.insert("Sonic".to_string());

        let orphans = find_orphans_in_dir(dir.path(), &rom_stems).unwrap();
        assert!(orphans.is_empty());
    }

    #[test]
    fn test_run_cleanup_no_media_dir() {
        let mut config = Config::default();
        config.credentials.devid = "test".to_string();
        config.credentials.devpassword = "test".to_string();
        config.paths.media_directory = "/nonexistent_scrauper_cleanup_test".to_string();
        config.paths.rom_directory = "/nonexistent_scrauper_cleanup_test".to_string();

        let options = CleanupOptions { dry_run: true, auto_confirm: false };
        let result = run_cleanup(&config, &options);
        assert!(result.is_err());
    }

    #[test]
    fn test_run_cleanup_dry_run_no_orphans() {
        let tmp = tempfile::tempdir().unwrap();
        let rom_dir = tmp.path().join("roms");
        let media_dir = tmp.path().join("media");
        let snes_rom = rom_dir.join("snes");
        let snes_media = media_dir.join("snes").join("screenshots");

        std::fs::create_dir_all(&snes_rom).unwrap();
        std::fs::create_dir_all(&snes_media).unwrap();

        std::fs::write(snes_rom.join("Sonic.zip"), b"rom").unwrap();
        std::fs::write(snes_media.join("Sonic.png"), b"img").unwrap();

        let mut config = Config::default();
        config.credentials.devid = "test".to_string();
        config.credentials.devpassword = "test".to_string();
        config.paths.rom_directory = rom_dir.to_string_lossy().to_string();
        config.paths.media_directory = media_dir.to_string_lossy().to_string();

        let options = CleanupOptions { dry_run: true, auto_confirm: false };
        let summary = run_cleanup(&config, &options).unwrap();
        assert_eq!(summary.orphans_found, 0);
        assert_eq!(summary.orphans_deleted, 0);
    }

    #[test]
    fn test_run_cleanup_dry_run_with_orphans() {
        let tmp = tempfile::tempdir().unwrap();
        let rom_dir = tmp.path().join("roms");
        let media_dir = tmp.path().join("media");
        let snes_rom = rom_dir.join("snes");
        let snes_media = media_dir.join("snes").join("screenshots");

        std::fs::create_dir_all(&snes_rom).unwrap();
        std::fs::create_dir_all(&snes_media).unwrap();

        std::fs::write(snes_rom.join("Sonic.zip"), b"rom").unwrap();
        std::fs::write(snes_media.join("Sonic.png"), b"img").unwrap();
        std::fs::write(snes_media.join("Deleted.png"), b"orphan").unwrap();

        let mut config = Config::default();
        config.credentials.devid = "test".to_string();
        config.credentials.devpassword = "test".to_string();
        config.paths.rom_directory = rom_dir.to_string_lossy().to_string();
        config.paths.media_directory = media_dir.to_string_lossy().to_string();

        let options = CleanupOptions { dry_run: true, auto_confirm: false };
        let summary = run_cleanup(&config, &options).unwrap();
        assert_eq!(summary.orphans_found, 1);
        assert_eq!(summary.orphans_deleted, 0);
        assert!(snes_media.join("Deleted.png").exists());
    }

    #[test]
    fn test_run_cleanup_auto_confirm_deletes() {
        let tmp = tempfile::tempdir().unwrap();
        let rom_dir = tmp.path().join("roms");
        let media_dir = tmp.path().join("media");
        let snes_rom = rom_dir.join("snes");
        let snes_media = media_dir.join("snes").join("screenshots");

        std::fs::create_dir_all(&snes_rom).unwrap();
        std::fs::create_dir_all(&snes_media).unwrap();

        std::fs::write(snes_rom.join("Sonic.zip"), b"rom").unwrap();
        std::fs::write(snes_media.join("Orphan.png"), b"orphan").unwrap();

        let mut config = Config::default();
        config.credentials.devid = "test".to_string();
        config.credentials.devpassword = "test".to_string();
        config.paths.rom_directory = rom_dir.to_string_lossy().to_string();
        config.paths.media_directory = media_dir.to_string_lossy().to_string();

        let options = CleanupOptions { dry_run: false, auto_confirm: true };
        let summary = run_cleanup(&config, &options).unwrap();
        assert_eq!(summary.orphans_found, 1);
        assert_eq!(summary.orphans_deleted, 1);
        assert!(summary.bytes_freed > 0);
        assert!(!snes_media.join("Orphan.png").exists());
    }

    #[test]
    fn test_run_cleanup_no_confirm_no_delete() {
        let tmp = tempfile::tempdir().unwrap();
        let rom_dir = tmp.path().join("roms");
        let media_dir = tmp.path().join("media");
        let snes_rom = rom_dir.join("snes");
        let snes_media = media_dir.join("snes").join("screenshots");

        std::fs::create_dir_all(&snes_rom).unwrap();
        std::fs::create_dir_all(&snes_media).unwrap();

        std::fs::write(snes_rom.join("Sonic.zip"), b"rom").unwrap();
        std::fs::write(snes_media.join("Orphan.png"), b"orphan").unwrap();

        let mut config = Config::default();
        config.credentials.devid = "test".to_string();
        config.credentials.devpassword = "test".to_string();
        config.paths.rom_directory = rom_dir.to_string_lossy().to_string();
        config.paths.media_directory = media_dir.to_string_lossy().to_string();

        let options = CleanupOptions { dry_run: false, auto_confirm: false };
        let summary = run_cleanup(&config, &options).unwrap();
        assert_eq!(summary.orphans_found, 1);
        assert_eq!(summary.orphans_deleted, 0);
        assert!(snes_media.join("Orphan.png").exists());
    }

    #[test]
    fn test_run_cleanup_multiple_media_types() {
        let tmp = tempfile::tempdir().unwrap();
        let rom_dir = tmp.path().join("roms");
        let media_dir = tmp.path().join("media");
        let snes_rom = rom_dir.join("snes");

        std::fs::create_dir_all(&snes_rom).unwrap();
        std::fs::write(snes_rom.join("Sonic.zip"), b"rom").unwrap();

        for subdir in &["screenshots", "covers"] {
            let dir = media_dir.join("snes").join(subdir);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("Orphan.png"), b"orphan").unwrap();
        }

        let mut config = Config::default();
        config.credentials.devid = "test".to_string();
        config.credentials.devpassword = "test".to_string();
        config.paths.rom_directory = rom_dir.to_string_lossy().to_string();
        config.paths.media_directory = media_dir.to_string_lossy().to_string();

        let options = CleanupOptions { dry_run: true, auto_confirm: false };
        let summary = run_cleanup(&config, &options).unwrap();
        assert_eq!(summary.orphans_found, 2);
    }

    #[cfg(unix)]
    #[test]
    fn test_run_cleanup_delete_failure_counts_remaining() {
        let tmp = tempfile::tempdir().unwrap();
        let rom_dir = tmp.path().join("roms");
        let media_dir = tmp.path().join("media");
        let snes_rom = rom_dir.join("snes");
        let snes_media = media_dir.join("snes").join("screenshots");

        std::fs::create_dir_all(&snes_rom).unwrap();
        std::fs::create_dir_all(&snes_media).unwrap();
        std::fs::write(snes_rom.join("Sonic.zip"), b"rom").unwrap();
        let orphan = snes_media.join("Orphan.png");
        std::fs::write(&orphan, b"orphan").unwrap();

        // Make directory non-writable to force remove_file failure.
        let original_perms = std::fs::metadata(&snes_media).unwrap().permissions();
        let mut read_only_perms = original_perms.clone();
        read_only_perms.set_mode(0o555);
        std::fs::set_permissions(&snes_media, read_only_perms).unwrap();

        let mut config = Config::default();
        config.credentials.devid = "test".to_string();
        config.credentials.devpassword = "test".to_string();
        config.paths.rom_directory = rom_dir.to_string_lossy().to_string();
        config.paths.media_directory = media_dir.to_string_lossy().to_string();

        let options = CleanupOptions { dry_run: false, auto_confirm: true };
        let summary = run_cleanup(&config, &options).unwrap();

        // Restore permissions so tempdir can be cleaned up.
        std::fs::set_permissions(&snes_media, original_perms).unwrap();

        assert_eq!(summary.orphans_found, 1);
        assert_eq!(summary.orphans_deleted, 0);
        assert_eq!(summary.bytes_freed, 0);
        assert!(orphan.exists());
    }
}

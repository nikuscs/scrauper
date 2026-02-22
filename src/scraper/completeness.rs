use std::path::Path;

use crate::config::Config;
use crate::models::gamelist::GameListEntry;
use crate::models::media::MediaType;

/// Check if a game is already completely scraped based on enabled content toggles.
pub fn is_complete(
    entry: Option<&GameListEntry>,
    rom_stem: &str,
    config: &Config,
    media_dir: &Path,
    system_name: &str,
) -> bool {
    let Some(entry) = entry else {
        return false; // No gamelist entry = always incomplete
    };

    // Check enabled metadata fields
    if config.content.name && entry.name.is_none() {
        return false;
    }
    if config.content.description && entry.desc.is_none() {
        return false;
    }
    if config.content.rating && entry.rating.is_none() {
        return false;
    }
    if config.content.release_date && entry.releasedate.is_none() {
        return false;
    }
    if config.content.developer && entry.developer.is_none() {
        return false;
    }
    if config.content.publisher && entry.publisher.is_none() {
        return false;
    }
    if config.content.genre && entry.genre.is_none() {
        return false;
    }
    if config.content.players && entry.players.is_none() {
        return false;
    }

    // Check enabled media types
    let media_checks: Vec<(bool, MediaType)> = vec![
        (config.content.media.screenshot, MediaType::Screenshot),
        (config.content.media.titlescreen, MediaType::TitleScreen),
        (config.content.media.video, MediaType::Video),
        (config.content.media.box_front, MediaType::BoxFront),
        (config.content.media.box_back, MediaType::BoxBack),
        (config.content.media.box_3d, MediaType::Box3d),
        (config.content.media.marquee, MediaType::Marquee),
        (config.content.media.physical_media, MediaType::PhysicalMedia),
        (config.content.media.fan_art, MediaType::FanArt),
        (config.content.media.manual, MediaType::Manual),
    ];

    for (enabled, media_type) in media_checks {
        if enabled && !media_file_exists(media_dir, system_name, &media_type, rom_stem) {
            return false;
        }
    }

    // Check miximage if enabled
    if config.miximage.enabled
        && !media_file_exists(media_dir, system_name, &MediaType::Miximage, rom_stem)
    {
        return false;
    }

    true
}

/// Check if a media file exists on disk.
fn media_file_exists(
    media_dir: &Path,
    system_name: &str,
    media_type: &MediaType,
    rom_stem: &str,
) -> bool {
    let ext = media_type.extension();
    let path = media_dir
        .join(system_name)
        .join(media_type.es_de_dirname())
        .join(format!("{}.{}", rom_stem, ext));

    if path.exists() {
        return true;
    }

    // Also check alternate extensions for images
    if ext == "png" {
        let jpg_path = media_dir
            .join(system_name)
            .join(media_type.es_de_dirname())
            .join(format!("{}.jpg", rom_stem));
        if jpg_path.exists() {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn default_config() -> Config {
        let mut config = Config::default();
        config.credentials.devid = "test".to_string();
        config.credentials.devpassword = "test".to_string();
        config
    }

    #[test]
    fn test_no_entry_is_incomplete() {
        let config = default_config();
        let tmp = TempDir::new().unwrap();
        assert!(!is_complete(None, "sonic", &config, tmp.path(), "snes"));
    }

    #[test]
    fn test_complete_metadata_no_media() {
        let mut config = default_config();
        // Disable all media
        config.content.media.screenshot = false;
        config.content.media.box_front = false;
        config.content.media.marquee = false;
        config.content.media.box_3d = false;
        config.miximage.enabled = false;

        let entry = GameListEntry {
            path: "./sonic.zip".to_string(),
            name: Some("Sonic".to_string()),
            desc: Some("Fast hedgehog".to_string()),
            rating: Some("0.8".to_string()),
            releasedate: Some("19910601".to_string()),
            developer: Some("Sega".to_string()),
            publisher: Some("Sega".to_string()),
            genre: Some("Platform".to_string()),
            players: Some("1".to_string()),
            ..Default::default()
        };

        let tmp = TempDir::new().unwrap();
        assert!(is_complete(Some(&entry), "sonic", &config, tmp.path(), "snes"));
    }

    #[test]
    fn test_incomplete_missing_name() {
        let mut config = default_config();
        config.content.media.screenshot = false;
        config.content.media.box_front = false;
        config.content.media.marquee = false;
        config.content.media.box_3d = false;
        config.miximage.enabled = false;

        let entry = GameListEntry {
            path: "./sonic.zip".to_string(),
            name: None, // Missing!
            desc: Some("desc".to_string()),
            rating: Some("0.8".to_string()),
            releasedate: Some("19910601".to_string()),
            developer: Some("Sega".to_string()),
            publisher: Some("Sega".to_string()),
            genre: Some("Platform".to_string()),
            players: Some("1".to_string()),
            ..Default::default()
        };

        let tmp = TempDir::new().unwrap();
        assert!(!is_complete(Some(&entry), "sonic", &config, tmp.path(), "snes"));
    }

    #[test]
    fn test_disabled_field_not_checked() {
        let mut config = default_config();
        config.content.description = false; // Disable description check
        config.content.media.screenshot = false;
        config.content.media.box_front = false;
        config.content.media.marquee = false;
        config.content.media.box_3d = false;
        config.miximage.enabled = false;

        let entry = GameListEntry {
            path: "./sonic.zip".to_string(),
            name: Some("Sonic".to_string()),
            desc: None, // Missing but disabled
            rating: Some("0.8".to_string()),
            releasedate: Some("19910601".to_string()),
            developer: Some("Sega".to_string()),
            publisher: Some("Sega".to_string()),
            genre: Some("Platform".to_string()),
            players: Some("1".to_string()),
            ..Default::default()
        };

        let tmp = TempDir::new().unwrap();
        assert!(is_complete(Some(&entry), "sonic", &config, tmp.path(), "snes"));
    }

    #[test]
    fn test_incomplete_missing_media_file() {
        let mut config = default_config();
        config.content.media.screenshot = true;
        config.content.media.box_front = false;
        config.content.media.marquee = false;
        config.content.media.box_3d = false;
        config.miximage.enabled = false;

        let entry = GameListEntry {
            path: "./sonic.zip".to_string(),
            name: Some("Sonic".to_string()),
            desc: Some("desc".to_string()),
            rating: Some("0.8".to_string()),
            releasedate: Some("19910601".to_string()),
            developer: Some("Sega".to_string()),
            publisher: Some("Sega".to_string()),
            genre: Some("Platform".to_string()),
            players: Some("1".to_string()),
            ..Default::default()
        };

        let tmp = TempDir::new().unwrap();
        // Screenshot file doesn't exist
        assert!(!is_complete(Some(&entry), "sonic", &config, tmp.path(), "snes"));
    }

    #[test]
    fn test_complete_with_media_file() {
        let mut config = default_config();
        config.content.media.screenshot = true;
        config.content.media.box_front = false;
        config.content.media.marquee = false;
        config.content.media.box_3d = false;
        config.miximage.enabled = false;

        let entry = GameListEntry {
            path: "./sonic.zip".to_string(),
            name: Some("Sonic".to_string()),
            desc: Some("desc".to_string()),
            rating: Some("0.8".to_string()),
            releasedate: Some("19910601".to_string()),
            developer: Some("Sega".to_string()),
            publisher: Some("Sega".to_string()),
            genre: Some("Platform".to_string()),
            players: Some("1".to_string()),
            ..Default::default()
        };

        let tmp = TempDir::new().unwrap();
        // Create the screenshot file
        let ss_dir = tmp.path().join("snes").join("screenshots");
        std::fs::create_dir_all(&ss_dir).unwrap();
        std::fs::write(ss_dir.join("sonic.png"), b"fake").unwrap();

        assert!(is_complete(Some(&entry), "sonic", &config, tmp.path(), "snes"));
    }

    #[test]
    fn test_media_file_exists_jpg_fallback() {
        let tmp = TempDir::new().unwrap();
        let ss_dir = tmp.path().join("snes").join("screenshots");
        std::fs::create_dir_all(&ss_dir).unwrap();
        // Create jpg instead of png
        std::fs::write(ss_dir.join("sonic.jpg"), b"fake").unwrap();

        assert!(media_file_exists(tmp.path(), "snes", &MediaType::Screenshot, "sonic"));
    }

    #[test]
    fn test_incomplete_missing_miximage() {
        let mut config = default_config();
        config.content.media.screenshot = false;
        config.content.media.box_front = false;
        config.content.media.marquee = false;
        config.content.media.box_3d = false;
        config.miximage.enabled = true; // Miximage required

        let entry = GameListEntry {
            path: "./sonic.zip".to_string(),
            name: Some("Sonic".to_string()),
            desc: Some("desc".to_string()),
            rating: Some("0.8".to_string()),
            releasedate: Some("19910601".to_string()),
            developer: Some("Sega".to_string()),
            publisher: Some("Sega".to_string()),
            genre: Some("Platform".to_string()),
            players: Some("1".to_string()),
            ..Default::default()
        };

        let tmp = TempDir::new().unwrap();
        assert!(!is_complete(Some(&entry), "sonic", &config, tmp.path(), "snes"));
    }

    #[test]
    fn test_all_metadata_none_is_incomplete() {
        let mut config = default_config();
        config.content.media.screenshot = false;
        config.content.media.box_front = false;
        config.content.media.marquee = false;
        config.content.media.box_3d = false;
        config.miximage.enabled = false;

        let entry = GameListEntry { path: "./sonic.zip".to_string(), ..Default::default() };

        let tmp = TempDir::new().unwrap();
        assert!(!is_complete(Some(&entry), "sonic", &config, tmp.path(), "snes"));
    }

    #[test]
    fn test_all_content_disabled_is_complete() {
        let mut config = default_config();
        config.content.name = false;
        config.content.description = false;
        config.content.rating = false;
        config.content.release_date = false;
        config.content.developer = false;
        config.content.publisher = false;
        config.content.genre = false;
        config.content.players = false;
        config.content.media.screenshot = false;
        config.content.media.box_front = false;
        config.content.media.marquee = false;
        config.content.media.box_3d = false;
        config.miximage.enabled = false;

        let entry = GameListEntry { path: "./sonic.zip".to_string(), ..Default::default() };

        let tmp = TempDir::new().unwrap();
        assert!(is_complete(Some(&entry), "sonic", &config, tmp.path(), "snes"));
    }

    #[test]
    fn test_incomplete_missing_rating() {
        let mut config = default_config();
        config.content.media.screenshot = false;
        config.content.media.box_front = false;
        config.content.media.marquee = false;
        config.content.media.box_3d = false;
        config.miximage.enabled = false;

        let entry = GameListEntry {
            path: "./sonic.zip".to_string(),
            name: Some("Sonic".to_string()),
            desc: Some("desc".to_string()),
            rating: None,
            releasedate: Some("19910601".to_string()),
            developer: Some("Sega".to_string()),
            publisher: Some("Sega".to_string()),
            genre: Some("Platform".to_string()),
            players: Some("1".to_string()),
            ..Default::default()
        };

        let tmp = TempDir::new().unwrap();
        assert!(!is_complete(Some(&entry), "sonic", &config, tmp.path(), "snes"));
    }

    #[test]
    fn test_video_media_file_check() {
        let mut config = default_config();
        config.content.media.screenshot = false;
        config.content.media.box_front = false;
        config.content.media.marquee = false;
        config.content.media.box_3d = false;
        config.content.media.video = true;
        config.miximage.enabled = false;

        let entry = GameListEntry {
            path: "./sonic.zip".to_string(),
            name: Some("Sonic".to_string()),
            desc: Some("desc".to_string()),
            rating: Some("0.8".to_string()),
            releasedate: Some("19910601".to_string()),
            developer: Some("Sega".to_string()),
            publisher: Some("Sega".to_string()),
            genre: Some("Platform".to_string()),
            players: Some("1".to_string()),
            ..Default::default()
        };

        let tmp = TempDir::new().unwrap();
        assert!(!is_complete(Some(&entry), "sonic", &config, tmp.path(), "snes"));

        let vid_dir = tmp.path().join("snes").join("videos");
        std::fs::create_dir_all(&vid_dir).unwrap();
        std::fs::write(vid_dir.join("sonic.mp4"), b"video").unwrap();
        assert!(is_complete(Some(&entry), "sonic", &config, tmp.path(), "snes"));
    }

    #[test]
    fn test_complete_with_miximage_on_disk() {
        let mut config = default_config();
        config.content.media.screenshot = false;
        config.content.media.box_front = false;
        config.content.media.marquee = false;
        config.content.media.box_3d = false;
        config.miximage.enabled = true;

        let entry = GameListEntry {
            path: "./sonic.zip".to_string(),
            name: Some("Sonic".to_string()),
            desc: Some("desc".to_string()),
            rating: Some("0.8".to_string()),
            releasedate: Some("19910601".to_string()),
            developer: Some("Sega".to_string()),
            publisher: Some("Sega".to_string()),
            genre: Some("Platform".to_string()),
            players: Some("1".to_string()),
            ..Default::default()
        };

        let tmp = TempDir::new().unwrap();
        let mix_dir = tmp.path().join("snes").join("miximages");
        std::fs::create_dir_all(&mix_dir).unwrap();
        std::fs::write(mix_dir.join("sonic.png"), b"fake").unwrap();
        assert!(is_complete(Some(&entry), "sonic", &config, tmp.path(), "snes"));
    }

    #[test]
    fn test_incomplete_missing_developer() {
        let mut config = default_config();
        config.content.media.screenshot = false;
        config.content.media.box_front = false;
        config.content.media.marquee = false;
        config.content.media.box_3d = false;
        config.miximage.enabled = false;

        let entry = GameListEntry {
            path: "./sonic.zip".to_string(),
            name: Some("Sonic".to_string()),
            desc: Some("desc".to_string()),
            rating: Some("0.8".to_string()),
            releasedate: Some("19910601".to_string()),
            developer: None, // Missing
            publisher: Some("Sega".to_string()),
            genre: Some("Platform".to_string()),
            players: Some("1".to_string()),
            ..Default::default()
        };

        let tmp = TempDir::new().unwrap();
        assert!(!is_complete(Some(&entry), "sonic", &config, tmp.path(), "snes"));
    }

    #[test]
    fn test_incomplete_missing_publisher() {
        let mut config = default_config();
        config.content.media.screenshot = false;
        config.content.media.box_front = false;
        config.content.media.marquee = false;
        config.content.media.box_3d = false;
        config.miximage.enabled = false;

        let entry = GameListEntry {
            path: "./sonic.zip".to_string(),
            name: Some("Sonic".to_string()),
            desc: Some("desc".to_string()),
            rating: Some("0.8".to_string()),
            releasedate: Some("19910601".to_string()),
            developer: Some("Sonic Team".to_string()),
            publisher: None, // Missing
            genre: Some("Platform".to_string()),
            players: Some("1".to_string()),
            ..Default::default()
        };

        let tmp = TempDir::new().unwrap();
        assert!(!is_complete(Some(&entry), "sonic", &config, tmp.path(), "snes"));
    }

    #[test]
    fn test_incomplete_missing_genre() {
        let mut config = default_config();
        config.content.media.screenshot = false;
        config.content.media.box_front = false;
        config.content.media.marquee = false;
        config.content.media.box_3d = false;
        config.miximage.enabled = false;

        let entry = GameListEntry {
            path: "./sonic.zip".to_string(),
            name: Some("Sonic".to_string()),
            desc: Some("desc".to_string()),
            rating: Some("0.8".to_string()),
            releasedate: Some("19910601".to_string()),
            developer: Some("Sonic Team".to_string()),
            publisher: Some("Sega".to_string()),
            genre: None, // Missing
            players: Some("1".to_string()),
            ..Default::default()
        };

        let tmp = TempDir::new().unwrap();
        assert!(!is_complete(Some(&entry), "sonic", &config, tmp.path(), "snes"));
    }

    #[test]
    fn test_incomplete_missing_players() {
        let mut config = default_config();
        config.content.media.screenshot = false;
        config.content.media.box_front = false;
        config.content.media.marquee = false;
        config.content.media.box_3d = false;
        config.miximage.enabled = false;

        let entry = GameListEntry {
            path: "./sonic.zip".to_string(),
            name: Some("Sonic".to_string()),
            desc: Some("desc".to_string()),
            rating: Some("0.8".to_string()),
            releasedate: Some("19910601".to_string()),
            developer: Some("Sonic Team".to_string()),
            publisher: Some("Sega".to_string()),
            genre: Some("Platform".to_string()),
            players: None, // Missing
            ..Default::default()
        };

        let tmp = TempDir::new().unwrap();
        assert!(!is_complete(Some(&entry), "sonic", &config, tmp.path(), "snes"));
    }

    #[test]
    fn test_incomplete_missing_release_date() {
        let mut config = default_config();
        config.content.media.screenshot = false;
        config.content.media.box_front = false;
        config.content.media.marquee = false;
        config.content.media.box_3d = false;
        config.miximage.enabled = false;

        let entry = GameListEntry {
            path: "./sonic.zip".to_string(),
            name: Some("Sonic".to_string()),
            desc: Some("desc".to_string()),
            rating: Some("0.8".to_string()),
            releasedate: None, // Missing
            developer: Some("Sonic Team".to_string()),
            publisher: Some("Sega".to_string()),
            genre: Some("Platform".to_string()),
            players: Some("1".to_string()),
            ..Default::default()
        };

        let tmp = TempDir::new().unwrap();
        assert!(!is_complete(Some(&entry), "sonic", &config, tmp.path(), "snes"));
    }

    #[test]
    fn test_media_file_exists_no_jpg_fallback_for_video() {
        let tmp = TempDir::new().unwrap();
        // Video has extension "mp4", not "png", so no jpg fallback
        assert!(!media_file_exists(tmp.path(), "snes", &MediaType::Video, "sonic"));
    }

    #[test]
    fn test_complete_with_box_front_on_disk() {
        let mut config = default_config();
        config.content.media.screenshot = false;
        config.content.media.box_front = true;
        config.content.media.marquee = false;
        config.content.media.box_3d = false;
        config.miximage.enabled = false;

        let entry = GameListEntry {
            path: "./sonic.zip".to_string(),
            name: Some("Sonic".to_string()),
            desc: Some("desc".to_string()),
            rating: Some("0.8".to_string()),
            releasedate: Some("19910601".to_string()),
            developer: Some("Sega".to_string()),
            publisher: Some("Sega".to_string()),
            genre: Some("Platform".to_string()),
            players: Some("1".to_string()),
            ..Default::default()
        };

        let tmp = TempDir::new().unwrap();
        let cover_dir = tmp.path().join("snes").join("covers");
        std::fs::create_dir_all(&cover_dir).unwrap();
        std::fs::write(cover_dir.join("sonic.png"), b"fake").unwrap();
        assert!(is_complete(Some(&entry), "sonic", &config, tmp.path(), "snes"));
    }
}

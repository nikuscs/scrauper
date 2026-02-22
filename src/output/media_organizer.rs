use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::models::media::MediaType;

/// Get the full path where a media file should be stored for a given ROM.
pub fn get_media_path(
    media_dir: &Path,
    system_name: &str,
    media_type: &MediaType,
    rom_stem: &str,
) -> PathBuf {
    media_dir.join(system_name).join(media_type.es_de_dirname()).join(format!(
        "{}.{}",
        rom_stem,
        media_type.extension()
    ))
}

/// Ensure the directory structure exists for a media path.
#[allow(dead_code)]
pub fn ensure_media_dir(media_dir: &Path, system_name: &str, media_type: &MediaType) -> Result<()> {
    let dir = media_dir.join(system_name).join(media_type.es_de_dirname());

    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }

    Ok(())
}

/// Get the ROM stem (filename without extension) from a path.
pub fn rom_stem(path: &Path) -> String {
    path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown").to_string()
}

/// Get the relative ROM path for gamelist.xml (e.g., "./Sonic The Hedgehog.zip").
pub fn gamelist_rom_path(rom_path: &Path, rom_dir: &Path) -> String {
    rom_path.strip_prefix(rom_dir).map_or_else(
        |_| format!("./{}", rom_path.file_name().unwrap_or_default().to_string_lossy()),
        |relative| format!("./{}", relative.display()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rom_stem_basic() {
        assert_eq!(rom_stem(Path::new("/roms/snes/Sonic.zip")), "Sonic");
        assert_eq!(rom_stem(Path::new("/roms/Mario Kart.sfc")), "Mario Kart");
    }

    #[test]
    fn test_rom_stem_no_extension() {
        assert_eq!(rom_stem(Path::new("/roms/GameName")), "GameName");
    }

    #[test]
    fn test_get_media_path() {
        let path = get_media_path(Path::new("/media"), "snes", &MediaType::Screenshot, "Sonic");
        assert_eq!(path, PathBuf::from("/media/snes/screenshots/Sonic.png"));
    }

    #[test]
    fn test_get_media_path_video() {
        let path = get_media_path(Path::new("/media"), "megadrive", &MediaType::Video, "Sonic");
        assert_eq!(path, PathBuf::from("/media/megadrive/videos/Sonic.mp4"));
    }

    #[test]
    fn test_get_media_path_manual() {
        let path = get_media_path(Path::new("/media"), "snes", &MediaType::Manual, "Mario");
        assert_eq!(path, PathBuf::from("/media/snes/manuals/Mario.pdf"));
    }

    #[test]
    fn test_gamelist_rom_path_relative() {
        let rom = Path::new("/roms/snes/Sonic.zip");
        let dir = Path::new("/roms/snes");
        assert_eq!(gamelist_rom_path(rom, dir), "./Sonic.zip");
    }

    #[test]
    fn test_gamelist_rom_path_nested() {
        let rom = Path::new("/roms/snes/subdir/Game.zip");
        let dir = Path::new("/roms/snes");
        assert_eq!(gamelist_rom_path(rom, dir), "./subdir/Game.zip");
    }

    #[test]
    fn test_gamelist_rom_path_no_prefix_match() {
        let rom = Path::new("/other/path/Sonic.zip");
        let dir = Path::new("/roms/snes");
        assert_eq!(gamelist_rom_path(rom, dir), "./Sonic.zip");
    }

    #[test]
    fn test_rom_stem_no_stem() {
        assert_eq!(rom_stem(Path::new("")), "unknown");
    }

    #[test]
    fn test_ensure_media_dir() {
        let tmp = tempfile::tempdir().unwrap();
        ensure_media_dir(tmp.path(), "snes", &MediaType::Screenshot).unwrap();
        assert!(tmp.path().join("snes").join("screenshots").exists());
    }

    #[test]
    fn test_ensure_media_dir_already_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("snes").join("screenshots");
        std::fs::create_dir_all(&dir).unwrap();
        ensure_media_dir(tmp.path(), "snes", &MediaType::Screenshot).unwrap();
        assert!(dir.exists());
    }

    #[test]
    fn test_ensure_media_dir_video() {
        let tmp = tempfile::tempdir().unwrap();
        ensure_media_dir(tmp.path(), "megadrive", &MediaType::Video).unwrap();
        assert!(tmp.path().join("megadrive").join("videos").exists());
    }

    #[test]
    fn test_get_media_path_all_types() {
        let types_and_dirs = vec![
            (MediaType::Screenshot, "screenshots", "png"),
            (MediaType::TitleScreen, "titlescreens", "png"),
            (MediaType::BoxFront, "covers", "png"),
            (MediaType::BoxBack, "backcovers", "png"),
            (MediaType::Box3d, "3dboxes", "png"),
            (MediaType::Marquee, "marquees", "png"),
            (MediaType::PhysicalMedia, "physicalmedia", "png"),
            (MediaType::FanArt, "fanart", "png"),
            (MediaType::Miximage, "miximages", "png"),
        ];
        for (mt, dir, ext) in types_and_dirs {
            let path = get_media_path(Path::new("/media"), "snes", &mt, "Sonic");
            assert_eq!(path, PathBuf::from(format!("/media/snes/{dir}/Sonic.{ext}")));
        }
    }
}

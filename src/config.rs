use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::models::region::{Language, Region};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub credentials: Credentials,
    pub paths: Paths,
    pub scraping: Scraping,
    pub content: Content,
    pub miximage: Miximage,
    pub locale: Locale,
    pub network: Network,
    pub post_processing: PostProcessing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Credentials {
    pub devid: String,
    pub devpassword: String,
    pub softname: String,
    pub ssid: String,
    pub sspassword: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Paths {
    pub rom_directory: String,
    pub media_directory: String,
    pub gamelist_directory: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Scraping {
    pub systems: Vec<String>,
    pub skip_complete: bool,
    pub hash_max_file_size_mb: u64,
    pub scrape_folders: bool,
    pub exclude_patterns: Vec<String>,
    pub search: SearchConfig,
    pub hide_individual_discs: bool,
    pub min_auto_accept_confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchConfig {
    pub strategy: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Content {
    pub name: bool,
    pub description: bool,
    pub rating: bool,
    pub release_date: bool,
    pub developer: bool,
    pub publisher: bool,
    pub genre: bool,
    pub players: bool,
    pub media: MediaContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MediaContent {
    pub video: bool,
    pub screenshot: bool,
    pub titlescreen: bool,
    pub box_front: bool,
    pub box_back: bool,
    pub marquee: bool,
    pub box_3d: bool,
    pub physical_media: bool,
    pub fan_art: bool,
    pub manual: bool,
    pub fallbacks: MediaFallbacks,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MediaFallbacks {
    pub cover_for_missing_3d_box: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Miximage {
    pub enabled: bool,
    pub overwrite_existing: bool,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub screenshot: MiximageScreenshot,
    pub layout: MiximageLayout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MiximageScreenshot {
    pub horizontal_fit: String,
    pub vertical_fit: String,
    pub aspect_ratio_threshold: f32,
    pub scaling: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MiximageLayout {
    pub box_size: String,
    pub physical_media_size: String,
    pub include_physical_media: bool,
    pub blank_fill_color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Locale {
    pub region_priority: Vec<Region>,
    pub language: Language,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Network {
    pub max_retries: u32,
    pub retry_base_delay_ms: u64,
    pub max_threads_override: u32,
    pub request_timeout_secs: u64,
    pub download_timeout_secs: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PostProcessing {
    pub remove_orphaned_media: bool,
    pub remove_pillarboxes: bool,
}

// --- Defaults ---

impl Default for Credentials {
    fn default() -> Self {
        Self {
            devid: String::new(),
            devpassword: String::new(),
            softname: "scrauper".to_string(),
            ssid: String::new(),
            sspassword: String::new(),
        }
    }
}

impl Default for Paths {
    fn default() -> Self {
        Self {
            rom_directory: "~/ROMs".to_string(),
            media_directory: "~/.emulationstation/downloaded_media".to_string(),
            gamelist_directory: "~/.emulationstation/gamelists".to_string(),
        }
    }
}

impl Default for Scraping {
    fn default() -> Self {
        Self {
            systems: Vec::new(),
            skip_complete: true,
            hash_max_file_size_mb: 300,
            scrape_folders: false,
            exclude_patterns: vec!["**/.DS_Store".to_string()],
            search: SearchConfig::default(),
            hide_individual_discs: true,
            min_auto_accept_confidence: 0.8,
        }
    }
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self { strategy: vec!["hash".to_string(), "filename".to_string()] }
    }
}

impl Default for Content {
    fn default() -> Self {
        Self {
            name: true,
            description: true,
            rating: true,
            release_date: true,
            developer: true,
            publisher: true,
            genre: true,
            players: true,
            media: MediaContent::default(),
        }
    }
}

impl Default for MediaContent {
    fn default() -> Self {
        Self {
            video: false,
            screenshot: true,
            titlescreen: false,
            box_front: true,
            box_back: false,
            marquee: true,
            box_3d: true,
            physical_media: false,
            fan_art: false,
            manual: false,
            fallbacks: MediaFallbacks::default(),
        }
    }
}

impl Default for MediaFallbacks {
    fn default() -> Self {
        Self { cover_for_missing_3d_box: true }
    }
}

impl Default for Miximage {
    fn default() -> Self {
        Self {
            enabled: true,
            overwrite_existing: false,
            width: 1280,
            height: 960,
            format: "png".to_string(),
            screenshot: MiximageScreenshot::default(),
            layout: MiximageLayout::default(),
        }
    }
}

impl Default for MiximageScreenshot {
    fn default() -> Self {
        Self {
            horizontal_fit: "crop".to_string(),
            vertical_fit: "contain".to_string(),
            aspect_ratio_threshold: 1.4,
            scaling: "sharp".to_string(),
        }
    }
}

impl Default for MiximageLayout {
    fn default() -> Self {
        Self {
            box_size: "medium".to_string(),
            physical_media_size: "medium".to_string(),
            include_physical_media: true,
            blank_fill_color: "#000000".to_string(),
        }
    }
}

impl Default for Locale {
    fn default() -> Self {
        Self {
            region_priority: vec![Region::Wor, Region::Us, Region::Eu, Region::Jp],
            language: Language::En,
        }
    }
}

impl Default for Network {
    fn default() -> Self {
        Self {
            max_retries: 3,
            retry_base_delay_ms: 1000,
            max_threads_override: 0,
            request_timeout_secs: 30,
            download_timeout_secs: 120,
        }
    }
}

// --- Loading ---

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;
        let config: Config =
            toml::from_str(&content).with_context(|| "Failed to parse config file")?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.credentials.devid.is_empty() || self.credentials.devpassword.is_empty() {
            anyhow::bail!("Developer credentials (devid, devpassword) are required in config file");
        }
        if self.credentials.ssid.is_empty() || self.credentials.sspassword.is_empty() {
            tracing::warn!("ScreenScraper user credentials (ssid, sspassword) not set — some features may be limited");
        }
        Ok(())
    }

    /// Resolve a path with tilde expansion.
    pub fn resolve_path(path: &str) -> PathBuf {
        if let Some(rest) = path.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(rest);
            }
        }
        PathBuf::from(path)
    }

    pub fn rom_directory(&self) -> PathBuf {
        Self::resolve_path(&self.paths.rom_directory)
    }

    pub fn media_directory(&self) -> PathBuf {
        Self::resolve_path(&self.paths.media_directory)
    }

    pub fn gamelist_directory(&self) -> PathBuf {
        Self::resolve_path(&self.paths.gamelist_directory)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.credentials.softname, "scrauper");
        assert!(config.credentials.devid.is_empty());
        assert_eq!(config.paths.rom_directory, "~/ROMs");
        assert!(config.scraping.skip_complete);
        assert_eq!(config.scraping.hash_max_file_size_mb, 300);
        assert!(config.content.name);
        assert!(config.content.description);
        assert!(config.content.media.screenshot);
        assert!(!config.content.media.video);
        assert!(config.content.media.box_3d);
        assert!(config.content.media.fallbacks.cover_for_missing_3d_box);
        assert!(config.miximage.enabled);
        assert_eq!(config.miximage.width, 1280);
        assert_eq!(config.miximage.height, 960);
        assert_eq!(config.miximage.format, "png");
        assert_eq!(config.miximage.screenshot.horizontal_fit, "crop");
        assert_eq!(config.miximage.screenshot.vertical_fit, "contain");
        assert_eq!(config.miximage.screenshot.scaling, "sharp");
        assert_eq!(config.locale.language, Language::En);
        assert_eq!(
            config.locale.region_priority,
            vec![Region::Wor, Region::Us, Region::Eu, Region::Jp]
        );
        assert_eq!(config.network.max_retries, 3);
        assert_eq!(config.network.request_timeout_secs, 30);
        assert!(!config.post_processing.remove_orphaned_media);
    }

    #[test]
    fn test_search_strategy_default() {
        let config = Config::default();
        assert_eq!(config.scraping.search.strategy, vec!["hash", "filename"]);
    }

    #[test]
    fn test_resolve_path_tilde() {
        let path = Config::resolve_path("~/ROMs");
        // Should expand ~ to home directory
        assert!(!path.starts_with("~"));
        assert!(path.to_string_lossy().contains("ROMs"));
    }

    #[test]
    fn test_resolve_path_absolute() {
        let path = Config::resolve_path("/absolute/path");
        assert_eq!(path, PathBuf::from("/absolute/path"));
    }

    #[test]
    fn test_resolve_path_relative() {
        let path = Config::resolve_path("relative/path");
        assert_eq!(path, PathBuf::from("relative/path"));
    }

    #[test]
    fn test_validate_missing_dev_credentials() {
        let config = Config::default();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_with_dev_credentials() {
        let mut config = Config::default();
        config.credentials.devid = "test_id".to_string();
        config.credentials.devpassword = "test_pass".to_string();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_toml_parsing_minimal() {
        let toml_str = r#"
[credentials]
devid = "mydev"
devpassword = "mypass"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.credentials.devid, "mydev");
        assert_eq!(config.credentials.devpassword, "mypass");
        // Defaults should fill in
        assert_eq!(config.credentials.softname, "scrauper");
        assert!(config.content.name);
        assert_eq!(config.miximage.width, 1280);
    }

    #[test]
    fn test_toml_parsing_with_overrides() {
        let toml_str = r#"
[credentials]
devid = "mydev"
devpassword = "mypass"

[miximage]
width = 1920
height = 1440
format = "jpg"

[locale]
language = "fr"
region_priority = ["fr", "eu", "wor"]
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.miximage.width, 1920);
        assert_eq!(config.miximage.height, 1440);
        assert_eq!(config.miximage.format, "jpg");
        assert_eq!(config.locale.language, Language::Fr);
        assert_eq!(config.locale.region_priority, vec![Region::Fr, Region::Eu, Region::Wor]);
    }

    #[test]
    fn test_toml_parsing_media_toggles() {
        let toml_str = r#"
[credentials]
devid = "x"
devpassword = "x"

[content.media]
video = true
screenshot = false
fan_art = true
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.content.media.video);
        assert!(!config.content.media.screenshot);
        assert!(config.content.media.fan_art);
        // Defaults for unspecified
        assert!(config.content.media.box_front);
    }

    #[test]
    fn test_exclude_patterns_default() {
        let config = Config::default();
        assert_eq!(config.scraping.exclude_patterns, vec!["**/.DS_Store"]);
    }

    #[test]
    fn test_rom_directory() {
        let mut config = Config::default();
        config.paths.rom_directory = "/absolute/roms".to_string();
        assert_eq!(config.rom_directory(), PathBuf::from("/absolute/roms"));
    }

    #[test]
    fn test_media_directory() {
        let mut config = Config::default();
        config.paths.media_directory = "/absolute/media".to_string();
        assert_eq!(config.media_directory(), PathBuf::from("/absolute/media"));
    }

    #[test]
    fn test_gamelist_directory() {
        let mut config = Config::default();
        config.paths.gamelist_directory = "/absolute/gamelists".to_string();
        assert_eq!(config.gamelist_directory(), PathBuf::from("/absolute/gamelists"));
    }

    #[test]
    fn test_rom_directory_tilde() {
        let config = Config::default();
        let path = config.rom_directory();
        assert!(!path.to_string_lossy().starts_with('~'));
        assert!(path.to_string_lossy().contains("ROMs"));
    }

    #[test]
    fn test_media_directory_tilde() {
        let config = Config::default();
        let path = config.media_directory();
        assert!(!path.to_string_lossy().starts_with('~'));
        assert!(path.to_string_lossy().contains("downloaded_media"));
    }

    #[test]
    fn test_gamelist_directory_tilde() {
        let config = Config::default();
        let path = config.gamelist_directory();
        assert!(!path.to_string_lossy().starts_with('~'));
        assert!(path.to_string_lossy().contains("gamelists"));
    }

    #[test]
    fn test_load_config_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.toml");
        std::fs::write(
            &path,
            r#"
[credentials]
devid = "testdev"
devpassword = "testpass"
ssid = "user"
sspassword = "pass"

[paths]
rom_directory = "/roms"
"#,
        )
        .unwrap();

        let config = Config::load(&path).unwrap();
        assert_eq!(config.credentials.devid, "testdev");
        assert_eq!(config.paths.rom_directory, "/roms");
        // defaults filled in
        assert_eq!(config.credentials.softname, "scrauper");
        assert!(config.content.name);
    }

    #[test]
    fn test_load_config_missing_file() {
        let result = Config::load(Path::new("/nonexistent_scrauper_config.toml"));
        assert!(result.is_err());
    }

    #[test]
    fn test_load_config_invalid_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.toml");
        std::fs::write(&path, "this is not valid toml!!!{{{").unwrap();
        let result = Config::load(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_empty_devid() {
        let mut config = Config::default();
        config.credentials.devpassword = "pass".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_empty_devpassword() {
        let mut config = Config::default();
        config.credentials.devid = "id".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_resolve_path_tilde_only() {
        // "~" alone without trailing slash
        let path = Config::resolve_path("~");
        // Should remain "~" since strip_prefix("~/") won't match
        assert_eq!(path, PathBuf::from("~"));
    }

    #[test]
    fn test_default_scraping_values() {
        let config = Config::default();
        assert!(config.scraping.hide_individual_discs);
        assert!((config.scraping.min_auto_accept_confidence - 0.8).abs() < 0.01);
        assert!(config.scraping.systems.is_empty());
        assert!(!config.scraping.scrape_folders);
    }

    #[test]
    fn test_default_network_values() {
        let config = Config::default();
        assert_eq!(config.network.max_retries, 3);
        assert_eq!(config.network.retry_base_delay_ms, 1000);
        assert_eq!(config.network.max_threads_override, 0);
        assert_eq!(config.network.request_timeout_secs, 30);
        assert_eq!(config.network.download_timeout_secs, 120);
    }

    #[test]
    fn test_default_miximage_screenshot() {
        let config = Config::default();
        assert_eq!(config.miximage.screenshot.horizontal_fit, "crop");
        assert_eq!(config.miximage.screenshot.vertical_fit, "contain");
        assert!((config.miximage.screenshot.aspect_ratio_threshold - 1.4).abs() < 0.01);
        assert_eq!(config.miximage.screenshot.scaling, "sharp");
    }

    #[test]
    fn test_default_miximage_layout() {
        let config = Config::default();
        assert_eq!(config.miximage.layout.box_size, "medium");
        assert_eq!(config.miximage.layout.physical_media_size, "medium");
        assert!(config.miximage.layout.include_physical_media);
        assert_eq!(config.miximage.layout.blank_fill_color, "#000000");
    }

    #[test]
    fn test_toml_parsing_network_override() {
        let toml_str = r#"
[credentials]
devid = "x"
devpassword = "x"

[network]
max_retries = 5
retry_base_delay_ms = 2000
max_threads_override = 2
request_timeout_secs = 60
download_timeout_secs = 300
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.network.max_retries, 5);
        assert_eq!(config.network.retry_base_delay_ms, 2000);
        assert_eq!(config.network.max_threads_override, 2);
        assert_eq!(config.network.request_timeout_secs, 60);
        assert_eq!(config.network.download_timeout_secs, 300);
    }

    #[test]
    fn test_toml_parsing_post_processing() {
        let toml_str = r#"
[credentials]
devid = "x"
devpassword = "x"

[post_processing]
remove_orphaned_media = true
remove_pillarboxes = true
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.post_processing.remove_orphaned_media);
        assert!(config.post_processing.remove_pillarboxes);
    }

    #[test]
    fn test_config_serde_roundtrip() {
        let mut config = Config::default();
        config.credentials.devid = "test".to_string();
        config.credentials.devpassword = "pass".to_string();
        let toml_str = toml::to_string(&config).unwrap();
        let back: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(back.credentials.devid, "test");
        assert_eq!(back.miximage.width, 1280);
        assert_eq!(back.locale.language, Language::En);
    }
}

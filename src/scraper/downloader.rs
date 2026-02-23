use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::io::AsyncWriteExt;

use crate::api::client::ScreenScraperClient;
use crate::api::error::ApiError;
use crate::config::Config;
use crate::models::media::MediaType;

/// Result of a media download attempt.
pub enum DownloadResult {
    /// File was downloaded successfully, includes the raw bytes for caching.
    Downloaded(PathBuf, Vec<u8>),
    /// Local file matches server (CRC/MD5/SHA1 OK), no download needed.
    Unchanged,
    /// Media is not available on the server.
    NotAvailable,
}

/// Download a media file for a game, trying regions in priority order.
pub async fn download_media(
    client: &ScreenScraperClient,
    game_id: u64,
    system_id: u64,
    media_type: &MediaType,
    dest_path: &Path,
    config: &Config,
) -> Result<DownloadResult> {
    let regions = &config.locale.region_priority;
    let timeout = Duration::from_secs(config.network.download_timeout_secs);

    // Determine which API endpoint to use
    let endpoint = match media_type {
        MediaType::Video => "mediaVideoJeu.php",
        _ => "mediaJeu.php",
    };

    // Try each region in priority order
    for region in regions {
        let media_id = media_type.api_media_id(region);

        let mut params: Vec<(&str, String)> = vec![
            ("systemeid", system_id.to_string()),
            ("jeuid", game_id.to_string()),
            ("media", media_id),
        ];

        // CRC skip optimization: if file already exists, send its CRC
        if dest_path.exists() {
            if let Ok(local_crc) = compute_file_crc(dest_path).await {
                params.push(("crc", local_crc));
            }
        }

        match client.get_raw(endpoint, &params, timeout).await {
            Ok(response) => {
                let body = response.bytes().await.map_err(ApiError::from)?;
                let body_str = String::from_utf8_lossy(&body);

                // Check for skip/no-media signals
                if body_str.starts_with("CRCOK")
                    || body_str.starts_with("MD5OK")
                    || body_str.starts_with("SHA1OK")
                {
                    tracing::debug!("Media unchanged (CRC OK): {}", dest_path.display());
                    return Ok(DownloadResult::Unchanged);
                }

                if body_str.starts_with("NOMEDIA") || body.is_empty() {
                    tracing::debug!("No media for region {}: {}", region, dest_path.display());
                    continue; // Try next region
                }

                // Write to temp file then rename (atomic)
                write_media_atomic(dest_path, &body).await?;
                tracing::debug!(
                    "Downloaded {} ({} bytes, region {})",
                    dest_path.display(),
                    body.len(),
                    region
                );
                return Ok(DownloadResult::Downloaded(dest_path.to_path_buf(), body.to_vec()));
            }
            Err(ApiError::GameNotFound) => {} // Try next region
            Err(e) => return Err(e.into()),
        }
    }

    // If media_type is Box3d and cover fallback is enabled, try 2D cover
    if *media_type == MediaType::Box3d && config.content.media.fallbacks.cover_for_missing_3d_box {
        tracing::debug!("3D box not found, trying 2D cover fallback");
        return download_cover_fallback(client, game_id, system_id, dest_path, config).await;
    }

    // If media_type is Marquee (wheel-hd), fall back to standard wheel
    if *media_type == MediaType::Marquee {
        tracing::debug!("wheel-hd not found, trying standard wheel fallback");
        return download_wheel_fallback(client, game_id, system_id, dest_path, config).await;
    }

    Ok(DownloadResult::NotAvailable)
}

/// Download 2D cover as a fallback for missing 3D box art.
async fn download_cover_fallback(
    client: &ScreenScraperClient,
    game_id: u64,
    system_id: u64,
    dest_path: &Path,
    config: &Config,
) -> Result<DownloadResult> {
    let timeout = Duration::from_secs(config.network.download_timeout_secs);

    for region in &config.locale.region_priority {
        let media_id = MediaType::BoxFront.api_media_id(region);

        let params: Vec<(&str, String)> = vec![
            ("systemeid", system_id.to_string()),
            ("jeuid", game_id.to_string()),
            ("media", media_id),
        ];

        match client.get_raw("mediaJeu.php", &params, timeout).await {
            Ok(response) => {
                let body = response.bytes().await.map_err(ApiError::from)?;
                let body_str = String::from_utf8_lossy(&body);

                if body_str.starts_with("NOMEDIA") || body.is_empty() {
                    continue;
                }
                if body_str.starts_with("CRCOK")
                    || body_str.starts_with("MD5OK")
                    || body_str.starts_with("SHA1OK")
                {
                    return Ok(DownloadResult::Unchanged);
                }

                write_media_atomic(dest_path, &body).await?;
                return Ok(DownloadResult::Downloaded(dest_path.to_path_buf(), body.to_vec()));
            }
            Err(ApiError::GameNotFound) => {}
            Err(e) => return Err(e.into()),
        }
    }

    Ok(DownloadResult::NotAvailable)
}

/// Download standard wheel as a fallback for missing wheel-hd marquee.
async fn download_wheel_fallback(
    client: &ScreenScraperClient,
    game_id: u64,
    system_id: u64,
    dest_path: &Path,
    config: &Config,
) -> Result<DownloadResult> {
    let timeout = Duration::from_secs(config.network.download_timeout_secs);

    for region in &config.locale.region_priority {
        let media_id = format!("wheel({})", region.api_suffix());

        let params: Vec<(&str, String)> = vec![
            ("systemeid", system_id.to_string()),
            ("jeuid", game_id.to_string()),
            ("media", media_id),
        ];

        match client.get_raw("mediaJeu.php", &params, timeout).await {
            Ok(response) => {
                let body = response.bytes().await.map_err(ApiError::from)?;
                let body_str = String::from_utf8_lossy(&body);

                if body_str.starts_with("NOMEDIA") || body.is_empty() {
                    continue;
                }
                if body_str.starts_with("CRCOK")
                    || body_str.starts_with("MD5OK")
                    || body_str.starts_with("SHA1OK")
                {
                    return Ok(DownloadResult::Unchanged);
                }

                write_media_atomic(dest_path, &body).await?;
                return Ok(DownloadResult::Downloaded(dest_path.to_path_buf(), body.to_vec()));
            }
            Err(ApiError::GameNotFound) => {}
            Err(e) => return Err(e.into()),
        }
    }

    Ok(DownloadResult::NotAvailable)
}

/// Write bytes to a file atomically (write to .tmp, then rename).
async fn write_media_atomic(dest: &Path, data: &[u8]) -> Result<()> {
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    let tmp_path = dest.with_extension("tmp");
    let mut file = tokio::fs::File::create(&tmp_path)
        .await
        .with_context(|| format!("Failed to create temp file: {}", tmp_path.display()))?;

    file.write_all(data).await?;
    file.flush().await?;
    drop(file);

    tokio::fs::rename(&tmp_path, dest).await.with_context(|| {
        format!("Failed to rename {} to {}", tmp_path.display(), dest.display())
    })?;

    Ok(())
}

/// Compute CRC32 of a local file.
async fn compute_file_crc(path: &Path) -> Result<String> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        use std::io::Read;
        let mut file = std::fs::File::open(&path)?;
        let mut hasher = crc32fast::Hasher::new();
        let mut buf = [0u8; 8192];
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(format!("{:08X}", hasher.finalize()))
    })
    .await?
}

/// Get the list of enabled media types from config.
pub fn enabled_media_types(config: &Config) -> Vec<MediaType> {
    let mut types = Vec::new();
    if config.content.media.screenshot {
        types.push(MediaType::Screenshot);
    }
    if config.content.media.titlescreen {
        types.push(MediaType::TitleScreen);
    }
    if config.content.media.video {
        types.push(MediaType::Video);
    }
    if config.content.media.box_front {
        types.push(MediaType::BoxFront);
    }
    if config.content.media.box_back {
        types.push(MediaType::BoxBack);
    }
    if config.content.media.box_3d {
        types.push(MediaType::Box3d);
    }
    if config.content.media.marquee {
        types.push(MediaType::Marquee);
    }
    if config.content.media.physical_media {
        types.push(MediaType::PhysicalMedia);
    }
    if config.content.media.fan_art {
        types.push(MediaType::FanArt);
    }
    if config.content.media.manual {
        types.push(MediaType::Manual);
    }
    types
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> Config {
        let mut config = Config::default();
        config.credentials.devid = "test".to_string();
        config.credentials.devpassword = "test".to_string();
        config
    }

    #[test]
    fn test_enabled_media_types_default() {
        let config = default_config();
        let types = enabled_media_types(&config);
        assert!(types.contains(&MediaType::Screenshot));
        assert!(types.contains(&MediaType::BoxFront));
        assert!(types.contains(&MediaType::Marquee));
        assert!(types.contains(&MediaType::Box3d));
        assert!(!types.contains(&MediaType::Video));
        assert!(!types.contains(&MediaType::TitleScreen));
    }

    #[test]
    fn test_enabled_media_types_all_on() {
        let mut config = default_config();
        config.content.media.video = true;
        config.content.media.screenshot = true;
        config.content.media.titlescreen = true;
        config.content.media.box_front = true;
        config.content.media.box_back = true;
        config.content.media.box_3d = true;
        config.content.media.marquee = true;
        config.content.media.physical_media = true;
        config.content.media.fan_art = true;
        config.content.media.manual = true;

        let types = enabled_media_types(&config);
        assert_eq!(types.len(), 10);
    }

    #[test]
    fn test_enabled_media_types_all_off() {
        let mut config = default_config();
        config.content.media.video = false;
        config.content.media.screenshot = false;
        config.content.media.titlescreen = false;
        config.content.media.box_front = false;
        config.content.media.box_back = false;
        config.content.media.box_3d = false;
        config.content.media.marquee = false;
        config.content.media.physical_media = false;
        config.content.media.fan_art = false;
        config.content.media.manual = false;

        let types = enabled_media_types(&config);
        assert!(types.is_empty());
    }

    #[tokio::test]
    async fn test_write_media_atomic() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dest = tmp.path().join("subdir").join("test.png");

        write_media_atomic(&dest, b"test data").await.unwrap();

        assert!(dest.exists());
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "test data");
        assert!(!dest.with_extension("tmp").exists());
    }

    #[tokio::test]
    async fn test_compute_file_crc() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.bin");
        std::fs::write(&path, b"hello world").unwrap();

        let crc = compute_file_crc(&path).await.unwrap();
        assert!(!crc.is_empty());
        assert_eq!(crc.len(), 8); // 8 hex chars
    }

    #[tokio::test]
    async fn test_compute_file_crc_deterministic() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.bin");
        std::fs::write(&path, b"test data").unwrap();

        let crc1 = compute_file_crc(&path).await.unwrap();
        let crc2 = compute_file_crc(&path).await.unwrap();
        assert_eq!(crc1, crc2);
    }

    #[tokio::test]
    async fn test_write_media_atomic_overwrites_existing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dest = tmp.path().join("test.png");
        std::fs::write(&dest, "old data").unwrap();

        write_media_atomic(&dest, b"new data").await.unwrap();

        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "new data");
    }

    #[tokio::test]
    async fn test_write_media_atomic_no_tmp_leftover() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dest = tmp.path().join("test.png");

        write_media_atomic(&dest, b"data").await.unwrap();

        // No .tmp file should remain
        assert!(!tmp.path().join("test.tmp").exists());
    }

    #[tokio::test]
    async fn test_write_media_atomic_empty_data() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dest = tmp.path().join("empty.png");

        write_media_atomic(&dest, b"").await.unwrap();

        assert!(dest.exists());
        assert_eq!(std::fs::read(&dest).unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_compute_file_crc_known_value() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.bin");
        // CRC32 of empty file
        std::fs::write(&path, b"").unwrap();
        let crc = compute_file_crc(&path).await.unwrap();
        assert_eq!(crc, "00000000");
    }

    #[tokio::test]
    async fn test_compute_file_crc_nonexistent_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("nonexistent.bin");
        let result = compute_file_crc(&path).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_enabled_media_types_single_type() {
        let mut config = default_config();
        config.content.media.video = false;
        config.content.media.screenshot = false;
        config.content.media.titlescreen = false;
        config.content.media.box_front = false;
        config.content.media.box_back = false;
        config.content.media.box_3d = false;
        config.content.media.marquee = true; // Only this one
        config.content.media.physical_media = false;
        config.content.media.fan_art = false;
        config.content.media.manual = false;

        let types = enabled_media_types(&config);
        assert_eq!(types.len(), 1);
        assert_eq!(types[0], MediaType::Marquee);
    }

    #[test]
    fn test_enabled_media_types_order() {
        let mut config = default_config();
        config.content.media.video = true;
        config.content.media.screenshot = true;
        config.content.media.titlescreen = true;
        config.content.media.box_front = true;
        config.content.media.box_back = true;
        config.content.media.box_3d = true;
        config.content.media.marquee = true;
        config.content.media.physical_media = true;
        config.content.media.fan_art = true;
        config.content.media.manual = true;

        let types = enabled_media_types(&config);
        // Verify order matches the function
        assert_eq!(types[0], MediaType::Screenshot);
        assert_eq!(types[1], MediaType::TitleScreen);
        assert_eq!(types[2], MediaType::Video);
        assert_eq!(types[3], MediaType::BoxFront);
        assert_eq!(types[4], MediaType::BoxBack);
        assert_eq!(types[5], MediaType::Box3d);
        assert_eq!(types[6], MediaType::Marquee);
        assert_eq!(types[7], MediaType::PhysicalMedia);
        assert_eq!(types[8], MediaType::FanArt);
        assert_eq!(types[9], MediaType::Manual);
    }

    // --- wiremock integration tests for download_media ---

    use crate::api::client::ScreenScraperClient;
    use crate::models::region::Region;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;

    fn mock_config() -> Config {
        let mut config = Config::default();
        config.credentials.devid = "testdev".to_string();
        config.credentials.devpassword = "testpass".to_string();
        config.credentials.softname = "scrauper".to_string();
        config.credentials.ssid = "user".to_string();
        config.credentials.sspassword = "userpass".to_string();
        config
    }

    #[tokio::test]
    async fn test_download_media_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/mediaJeu.php"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"PNG_IMAGE_DATA".to_vec()))
            .mount(&mock_server)
            .await;

        let mut config = mock_config();
        config.locale.region_priority = vec![Region::Us];
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();

        let tmp = tempfile::TempDir::new().unwrap();
        let dest = tmp.path().join("screenshot.png");

        let result =
            download_media(&client, 123, 1, &MediaType::Screenshot, &dest, &config).await.unwrap();

        match result {
            DownloadResult::Downloaded(p, data) => {
                assert_eq!(p, dest);
                assert_eq!(data, b"PNG_IMAGE_DATA");
                assert!(dest.exists());
                assert_eq!(std::fs::read(&dest).unwrap(), b"PNG_IMAGE_DATA");
            }
            _ => panic!("Expected DownloadResult::Downloaded"),
        }
    }

    #[tokio::test]
    async fn test_download_media_crc_ok() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/mediaJeu.php"))
            .respond_with(ResponseTemplate::new(200).set_body_string("CRCOK"))
            .mount(&mock_server)
            .await;

        let mut config = mock_config();
        config.locale.region_priority = vec![Region::Us];
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();

        let tmp = tempfile::TempDir::new().unwrap();
        let dest = tmp.path().join("screenshot.png");

        let result =
            download_media(&client, 123, 1, &MediaType::Screenshot, &dest, &config).await.unwrap();

        assert!(matches!(result, DownloadResult::Unchanged));
    }

    #[tokio::test]
    async fn test_download_media_md5_ok() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/mediaJeu.php"))
            .respond_with(ResponseTemplate::new(200).set_body_string("MD5OK"))
            .mount(&mock_server)
            .await;

        let mut config = mock_config();
        config.locale.region_priority = vec![Region::Us];
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();

        let tmp = tempfile::TempDir::new().unwrap();
        let dest = tmp.path().join("screenshot.png");

        let result =
            download_media(&client, 123, 1, &MediaType::Screenshot, &dest, &config).await.unwrap();

        assert!(matches!(result, DownloadResult::Unchanged));
    }

    #[tokio::test]
    async fn test_download_media_nomedia() {
        let mock_server = MockServer::start().await;

        // All regions return NOMEDIA
        Mock::given(method("GET"))
            .and(path("/mediaJeu.php"))
            .respond_with(ResponseTemplate::new(200).set_body_string("NOMEDIA"))
            .mount(&mock_server)
            .await;

        let mut config = mock_config();
        config.locale.region_priority = vec![Region::Us, Region::Eu];
        config.content.media.fallbacks.cover_for_missing_3d_box = false;
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();

        let tmp = tempfile::TempDir::new().unwrap();
        let dest = tmp.path().join("screenshot.png");

        let result =
            download_media(&client, 123, 1, &MediaType::Screenshot, &dest, &config).await.unwrap();

        assert!(matches!(result, DownloadResult::NotAvailable));
    }

    #[tokio::test]
    async fn test_download_media_empty_body_tries_next_region() {
        let mock_server = MockServer::start().await;

        // First region (Us) returns empty body
        Mock::given(method("GET"))
            .and(path("/mediaJeu.php"))
            .and(query_param("media", "ss(us)"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(Vec::new()))
            .mount(&mock_server)
            .await;

        // Second region (Eu) returns data
        Mock::given(method("GET"))
            .and(path("/mediaJeu.php"))
            .and(query_param("media", "ss(eu)"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"IMAGE_DATA".to_vec()))
            .mount(&mock_server)
            .await;

        let mut config = mock_config();
        config.locale.region_priority = vec![Region::Us, Region::Eu];
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();

        let tmp = tempfile::TempDir::new().unwrap();
        let dest = tmp.path().join("screenshot.png");

        let result =
            download_media(&client, 123, 1, &MediaType::Screenshot, &dest, &config).await.unwrap();

        match result {
            DownloadResult::Downloaded(_, data) => {
                assert_eq!(data, b"IMAGE_DATA");
            }
            _ => panic!("Expected DownloadResult::Downloaded"),
        }
    }

    #[tokio::test]
    async fn test_download_media_404_tries_next_region() {
        let mock_server = MockServer::start().await;

        // First region (Us) returns 404
        Mock::given(method("GET"))
            .and(path("/mediaJeu.php"))
            .and(query_param("media", "ss(us)"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        // Second region (Eu) returns data
        Mock::given(method("GET"))
            .and(path("/mediaJeu.php"))
            .and(query_param("media", "ss(eu)"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"IMAGE_DATA".to_vec()))
            .mount(&mock_server)
            .await;

        let mut config = mock_config();
        config.locale.region_priority = vec![Region::Us, Region::Eu];
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();

        let tmp = tempfile::TempDir::new().unwrap();
        let dest = tmp.path().join("screenshot.png");

        let result =
            download_media(&client, 123, 1, &MediaType::Screenshot, &dest, &config).await.unwrap();

        match result {
            DownloadResult::Downloaded(_, data) => {
                assert_eq!(data, b"IMAGE_DATA");
            }
            _ => panic!("Expected DownloadResult::Downloaded"),
        }
    }

    #[tokio::test]
    async fn test_download_media_video_uses_video_endpoint() {
        let mock_server = MockServer::start().await;

        // Video endpoint should be called
        Mock::given(method("GET"))
            .and(path("/mediaVideoJeu.php"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"VIDEO_DATA".to_vec()))
            .expect(1)
            .mount(&mock_server)
            .await;

        // Image endpoint should NOT be called
        Mock::given(method("GET"))
            .and(path("/mediaJeu.php"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"WRONG".to_vec()))
            .expect(0)
            .mount(&mock_server)
            .await;

        let mut config = mock_config();
        config.locale.region_priority = vec![Region::Us];
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();

        let tmp = tempfile::TempDir::new().unwrap();
        let dest = tmp.path().join("video.mp4");

        let result =
            download_media(&client, 123, 1, &MediaType::Video, &dest, &config).await.unwrap();

        match result {
            DownloadResult::Downloaded(_, data) => {
                assert_eq!(data, b"VIDEO_DATA");
            }
            _ => panic!("Expected DownloadResult::Downloaded"),
        }
    }

    #[tokio::test]
    async fn test_download_media_box3d_fallback_to_cover() {
        let mock_server = MockServer::start().await;

        // Box-3D returns NOMEDIA for all regions
        Mock::given(method("GET"))
            .and(path("/mediaJeu.php"))
            .and(query_param("media", "box-3D(us)"))
            .respond_with(ResponseTemplate::new(200).set_body_string("NOMEDIA"))
            .mount(&mock_server)
            .await;

        // Box-2D (cover fallback) returns data
        Mock::given(method("GET"))
            .and(path("/mediaJeu.php"))
            .and(query_param("media", "box-2D(us)"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"COVER_DATA".to_vec()))
            .mount(&mock_server)
            .await;

        let mut config = mock_config();
        config.locale.region_priority = vec![Region::Us];
        config.content.media.fallbacks.cover_for_missing_3d_box = true;
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();

        let tmp = tempfile::TempDir::new().unwrap();
        let dest = tmp.path().join("box3d.png");

        let result =
            download_media(&client, 123, 1, &MediaType::Box3d, &dest, &config).await.unwrap();

        match result {
            DownloadResult::Downloaded(_, data) => {
                assert_eq!(data, b"COVER_DATA");
            }
            _ => panic!("Expected DownloadResult::Downloaded from cover fallback"),
        }
    }

    #[tokio::test]
    async fn test_download_media_box3d_no_fallback_when_disabled() {
        let mock_server = MockServer::start().await;

        // Box-3D returns NOMEDIA for all regions
        Mock::given(method("GET"))
            .and(path("/mediaJeu.php"))
            .and(query_param("media", "box-3D(us)"))
            .respond_with(ResponseTemplate::new(200).set_body_string("NOMEDIA"))
            .mount(&mock_server)
            .await;

        let mut config = mock_config();
        config.locale.region_priority = vec![Region::Us];
        config.content.media.fallbacks.cover_for_missing_3d_box = false;
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();

        let tmp = tempfile::TempDir::new().unwrap();
        let dest = tmp.path().join("box3d.png");

        let result =
            download_media(&client, 123, 1, &MediaType::Box3d, &dest, &config).await.unwrap();

        assert!(matches!(result, DownloadResult::NotAvailable));
    }

    #[tokio::test]
    async fn test_download_media_api_error_propagates() {
        let mock_server = MockServer::start().await;

        // 403 = InvalidCredentials, which is NOT GameNotFound, so it should propagate
        Mock::given(method("GET"))
            .and(path("/mediaJeu.php"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&mock_server)
            .await;

        let mut config = mock_config();
        config.locale.region_priority = vec![Region::Us];
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();

        let tmp = tempfile::TempDir::new().unwrap();
        let dest = tmp.path().join("screenshot.png");

        let result = download_media(&client, 123, 1, &MediaType::Screenshot, &dest, &config).await;

        assert!(result.is_err(), "Expected error to propagate");
    }

    #[tokio::test]
    async fn test_download_media_marquee_fallback_to_wheel() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/mediaJeu.php"))
            .and(query_param("media", "wheel-hd(us)"))
            .respond_with(ResponseTemplate::new(200).set_body_string("NOMEDIA"))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/mediaJeu.php"))
            .and(query_param("media", "wheel(us)"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"WHEEL_DATA".to_vec()))
            .mount(&mock_server)
            .await;

        let mut config = mock_config();
        config.locale.region_priority = vec![Region::Us];
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();

        let tmp = tempfile::TempDir::new().unwrap();
        let dest = tmp.path().join("marquee.png");

        let result =
            download_media(&client, 123, 1, &MediaType::Marquee, &dest, &config).await.unwrap();
        match result {
            DownloadResult::Downloaded(_, data) => assert_eq!(data, b"WHEEL_DATA"),
            _ => panic!("Expected DownloadResult::Downloaded from wheel fallback"),
        }
    }

    #[tokio::test]
    async fn test_download_media_marquee_fallback_unchanged() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/mediaJeu.php"))
            .and(query_param("media", "wheel-hd(us)"))
            .respond_with(ResponseTemplate::new(200).set_body_string("NOMEDIA"))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/mediaJeu.php"))
            .and(query_param("media", "wheel(us)"))
            .respond_with(ResponseTemplate::new(200).set_body_string("CRCOK"))
            .mount(&mock_server)
            .await;

        let mut config = mock_config();
        config.locale.region_priority = vec![Region::Us];
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();

        let tmp = tempfile::TempDir::new().unwrap();
        let dest = tmp.path().join("marquee.png");

        let result =
            download_media(&client, 123, 1, &MediaType::Marquee, &dest, &config).await.unwrap();
        assert!(matches!(result, DownloadResult::Unchanged));
    }

    #[tokio::test]
    async fn test_download_media_box3d_fallback_unchanged() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/mediaJeu.php"))
            .and(query_param("media", "box-3D(us)"))
            .respond_with(ResponseTemplate::new(200).set_body_string("NOMEDIA"))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/mediaJeu.php"))
            .and(query_param("media", "box-2D(us)"))
            .respond_with(ResponseTemplate::new(200).set_body_string("CRCOK"))
            .mount(&mock_server)
            .await;

        let mut config = mock_config();
        config.locale.region_priority = vec![Region::Us];
        config.content.media.fallbacks.cover_for_missing_3d_box = true;
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();

        let tmp = tempfile::TempDir::new().unwrap();
        let dest = tmp.path().join("box3d.png");

        let result =
            download_media(&client, 123, 1, &MediaType::Box3d, &dest, &config).await.unwrap();
        assert!(matches!(result, DownloadResult::Unchanged));
    }

    #[tokio::test]
    async fn test_download_media_box3d_fallback_not_available() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/mediaJeu.php"))
            .and(query_param("media", "box-3D(us)"))
            .respond_with(ResponseTemplate::new(200).set_body_string("NOMEDIA"))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/mediaJeu.php"))
            .and(query_param("media", "box-2D(us)"))
            .respond_with(ResponseTemplate::new(200).set_body_string("NOMEDIA"))
            .mount(&mock_server)
            .await;

        let mut config = mock_config();
        config.locale.region_priority = vec![Region::Us];
        config.content.media.fallbacks.cover_for_missing_3d_box = true;
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();

        let tmp = tempfile::TempDir::new().unwrap();
        let dest = tmp.path().join("box3d.png");

        let result =
            download_media(&client, 123, 1, &MediaType::Box3d, &dest, &config).await.unwrap();
        assert!(matches!(result, DownloadResult::NotAvailable));
    }

    #[tokio::test]
    async fn test_download_media_box3d_fallback_error_propagates() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/mediaJeu.php"))
            .and(query_param("media", "box-3D(us)"))
            .respond_with(ResponseTemplate::new(200).set_body_string("NOMEDIA"))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/mediaJeu.php"))
            .and(query_param("media", "box-2D(us)"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&mock_server)
            .await;

        let mut config = mock_config();
        config.locale.region_priority = vec![Region::Us];
        config.content.media.fallbacks.cover_for_missing_3d_box = true;
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();

        let tmp = tempfile::TempDir::new().unwrap();
        let dest = tmp.path().join("box3d.png");

        let result = download_media(&client, 123, 1, &MediaType::Box3d, &dest, &config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_download_media_marquee_fallback_not_available_after_game_not_found() {
        let mock_server = MockServer::start().await;

        // Primary marquee missing
        Mock::given(method("GET"))
            .and(path("/mediaJeu.php"))
            .and(query_param("media", "wheel-hd(us)"))
            .respond_with(ResponseTemplate::new(200).set_body_string("NOMEDIA"))
            .mount(&mock_server)
            .await;

        // Fallback wheel returns 404 for first region, then NOMEDIA for second region
        Mock::given(method("GET"))
            .and(path("/mediaJeu.php"))
            .and(query_param("media", "wheel(us)"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/mediaJeu.php"))
            .and(query_param("media", "wheel(eu)"))
            .respond_with(ResponseTemplate::new(200).set_body_string("NOMEDIA"))
            .mount(&mock_server)
            .await;

        let mut config = mock_config();
        config.locale.region_priority = vec![Region::Us, Region::Eu];
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();

        let tmp = tempfile::TempDir::new().unwrap();
        let dest = tmp.path().join("marquee.png");

        let result =
            download_media(&client, 123, 1, &MediaType::Marquee, &dest, &config).await.unwrap();
        assert!(matches!(result, DownloadResult::NotAvailable));
    }

    #[tokio::test]
    async fn test_download_media_existing_file_sends_crc() {
        let mock_server = MockServer::start().await;
        let tmp = tempfile::TempDir::new().unwrap();
        let dest = tmp.path().join("existing.png");
        std::fs::write(&dest, b"hello world").unwrap();
        let crc = compute_file_crc(&dest).await.unwrap();

        Mock::given(method("GET"))
            .and(path("/mediaJeu.php"))
            .and(query_param("media", "ss(us)"))
            .and(query_param("crc", crc.as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"UPDATED".to_vec()))
            .mount(&mock_server)
            .await;

        let mut config = mock_config();
        config.locale.region_priority = vec![Region::Us];
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();

        let result =
            download_media(&client, 123, 1, &MediaType::Screenshot, &dest, &config).await.unwrap();
        match result {
            DownloadResult::Downloaded(_, data) => assert_eq!(data, b"UPDATED"),
            _ => panic!("Expected DownloadResult::Downloaded"),
        }
    }
}

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::models::system::{System, SystemsCache};

use super::client::ScreenScraperClient;
use super::error::ApiError;

// --- Response types ---

/// User info response from ssuserInfos.php
#[derive(Debug, Deserialize)]
pub struct UserInfoResponse {
    pub response: UserInfoInner,
}

#[derive(Debug, Deserialize)]
pub struct UserInfoInner {
    pub ssuser: SsUser,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct SsUser {
    pub id: Option<String>,
    #[serde(rename = "numid")]
    pub num_id: Option<String>,
    pub niveau: Option<String>,
    pub contribution: Option<String>,
    pub uploadsysteme: Option<String>,
    pub uploadinfos: Option<String>,
    pub romassocaliases: Option<String>,
    pub uploadmedia: Option<String>,
    pub propositionok: Option<String>,
    pub propositionko: Option<String>,
    pub quotareq100: Option<String>,
    pub maxthreads: Option<String>,
    pub maxdownloadspeed: Option<String>,
    pub requeststoday: Option<String>,
    pub requestskotoday: Option<String>,
    pub maxrequestspermin: Option<String>,
    pub maxrequestsperday: Option<String>,
    pub maxrequestskoperday: Option<String>,
    pub visites: Option<String>,
    pub datedernierevisite: Option<String>,
    pub favregion: Option<String>,
}

/// Parsed user info for display.
#[derive(Debug)]
pub struct UserInfo {
    pub username: String,
    pub user_id: String,
    pub level: String,
    pub contribution: String,
    pub max_threads: u32,
    pub max_requests_per_min: u32,
    pub max_requests_per_day: u32,
    pub requests_today: u32,
    pub requests_ko_today: u32,
    pub max_download_speed: u32,
    pub favorite_region: String,
}

impl From<SsUser> for UserInfo {
    fn from(u: SsUser) -> Self {
        Self {
            username: u.id.unwrap_or_default(),
            user_id: u.num_id.unwrap_or_default(),
            level: u.niveau.unwrap_or_default(),
            contribution: u.contribution.unwrap_or_default(),
            max_threads: u.maxthreads.as_deref().unwrap_or("1").parse().unwrap_or(1),
            max_requests_per_min: u
                .maxrequestspermin
                .as_deref()
                .unwrap_or("0")
                .parse()
                .unwrap_or(0),
            max_requests_per_day: u
                .maxrequestsperday
                .as_deref()
                .unwrap_or("0")
                .parse()
                .unwrap_or(0),
            requests_today: u.requeststoday.as_deref().unwrap_or("0").parse().unwrap_or(0),
            requests_ko_today: u.requestskotoday.as_deref().unwrap_or("0").parse().unwrap_or(0),
            max_download_speed: u.maxdownloadspeed.as_deref().unwrap_or("0").parse().unwrap_or(0),
            favorite_region: u.favregion.unwrap_or_default(),
        }
    }
}

/// Infrastructure info response from ssinfraInfos.php
#[derive(Debug, Deserialize)]
pub struct InfraInfoResponse {
    pub response: InfraInfoInner,
}

#[derive(Debug, Deserialize)]
pub struct InfraInfoInner {
    pub ssinfra: SsInfra,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct SsInfra {
    pub cpu1: Option<String>,
    pub cpu2: Option<String>,
    pub cpu3: Option<String>,
    pub cpu4: Option<String>,
    pub threadsmin: Option<String>,
    pub nbscrapeurs: Option<String>,
    pub apiaccess: Option<String>,
    pub closefornomember: Option<String>,
    pub closeforlevel1member: Option<String>,
}

/// Parsed infra info for display.
#[derive(Debug)]
pub struct InfraInfo {
    pub cpu_loads: Vec<String>,
    pub active_threads: String,
    pub active_scrapers: String,
    pub api_open: bool,
}

impl From<SsInfra> for InfraInfo {
    fn from(i: SsInfra) -> Self {
        let mut cpus = Vec::new();
        if let Some(c) = i.cpu1 {
            cpus.push(format!("CPU1: {}%", c));
        }
        if let Some(c) = i.cpu2 {
            cpus.push(format!("CPU2: {}%", c));
        }
        if let Some(c) = i.cpu3 {
            cpus.push(format!("CPU3: {}%", c));
        }
        if let Some(c) = i.cpu4 {
            cpus.push(format!("CPU4: {}%", c));
        }
        Self {
            cpu_loads: cpus,
            active_threads: i.threadsmin.unwrap_or_default(),
            active_scrapers: i.nbscrapeurs.unwrap_or_default(),
            api_open: i.apiaccess.as_deref() != Some("0"),
        }
    }
}

/// Systems list response from systemesListe.php
#[derive(Debug, Deserialize)]
pub struct SystemsListResponse {
    pub response: SystemsListInner,
}

#[derive(Debug, Deserialize)]
pub struct SystemsListInner {
    pub systemes: Vec<SystemEntry>,
}

#[derive(Debug, Deserialize)]
pub struct SystemEntry {
    pub id: serde_json::Value,
    pub noms: Option<serde_json::Value>,
    pub extensions: Option<String>,
    #[serde(rename = "type")]
    pub system_type: Option<String>,
    pub compagnie: Option<String>,
}

impl SystemEntry {
    pub fn to_system(&self) -> System {
        let id = match &self.id {
            serde_json::Value::Number(n) => n.as_u64().unwrap_or(0),
            serde_json::Value::String(s) => s.parse().unwrap_or(0),
            _ => 0,
        };

        let name = self
            .noms
            .as_ref()
            .and_then(|n| {
                n.get("nom_eu")
                    .or_else(|| n.get("nom_us"))
                    .or_else(|| n.get("nom_wor"))
                    .or_else(|| n.get("nom_jp"))
            })
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();

        let extensions = self
            .extensions
            .as_deref()
            .unwrap_or("")
            .split([' ', ','])
            .filter(|s| !s.is_empty())
            .map(|s| s.trim_start_matches('.').to_lowercase())
            .collect();

        System {
            id,
            name,
            extensions,
            system_type: self.system_type.clone().unwrap_or_default(),
            company: self.compagnie.clone(),
        }
    }
}

// --- Client methods ---

impl ScreenScraperClient {
    pub async fn get_user_info(&self) -> Result<UserInfo, ApiError> {
        let resp: UserInfoResponse = self.get_json("ssuserInfos.php", &[]).await?;
        Ok(resp.response.ssuser.into())
    }

    pub async fn get_infra_info(&self) -> Result<InfraInfo, ApiError> {
        let resp: InfraInfoResponse = self.get_json("ssinfraInfos.php", &[]).await?;
        Ok(resp.response.ssinfra.into())
    }

    pub async fn get_systems_list(&self) -> Result<Vec<System>, ApiError> {
        let resp: SystemsListResponse = self.get_json("systemesListe.php", &[]).await?;
        Ok(resp.response.systemes.iter().map(SystemEntry::to_system).collect())
    }
}

// --- Systems cache ---

fn systems_cache_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join("systems_cache.json")
}

/// Load systems from local cache if fresh (< 7 days).
pub fn load_systems_cache(cache_dir: &Path) -> Result<Option<Vec<System>>> {
    let path = systems_cache_path(cache_dir);
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&path).context("Failed to read systems cache")?;
    let cache: SystemsCache =
        serde_json::from_str(&content).context("Failed to parse systems cache")?;

    // Check if cache is still fresh (< 7 days)
    if let Ok(fetched) = chrono::DateTime::parse_from_rfc3339(&cache.fetched_at) {
        let age = chrono::Utc::now() - fetched.to_utc();
        if age.num_days() < 7 {
            return Ok(Some(cache.systems));
        }
    }

    Ok(None)
}

/// Save systems list to local cache.
pub fn save_systems_cache(cache_dir: &Path, systems: &[System]) -> Result<()> {
    std::fs::create_dir_all(cache_dir).context("Failed to create cache directory")?;

    let cache =
        SystemsCache { fetched_at: chrono::Utc::now().to_rfc3339(), systems: systems.to_vec() };

    let json = serde_json::to_string_pretty(&cache).context("Failed to serialize systems cache")?;

    std::fs::write(systems_cache_path(cache_dir), json).context("Failed to write systems cache")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_system_entry_to_system_numeric_id() {
        let entry = SystemEntry {
            id: json!(1),
            noms: Some(json!({"nom_eu": "Super Nintendo"})),
            extensions: Some(".sfc .smc .zip".to_string()),
            system_type: Some("Console".to_string()),
            compagnie: Some("Nintendo".to_string()),
        };
        let sys = entry.to_system();
        assert_eq!(sys.id, 1);
        assert_eq!(sys.name, "Super Nintendo");
        assert_eq!(sys.extensions, vec!["sfc", "smc", "zip"]);
        assert_eq!(sys.system_type, "Console");
        assert_eq!(sys.company, Some("Nintendo".to_string()));
    }

    #[test]
    fn test_system_entry_to_system_string_id() {
        let entry = SystemEntry {
            id: json!("42"),
            noms: Some(json!({"nom_us": "Genesis"})),
            extensions: Some(".md .bin".to_string()),
            system_type: None,
            compagnie: None,
        };
        let sys = entry.to_system();
        assert_eq!(sys.id, 42);
        assert_eq!(sys.name, "Genesis");
        assert_eq!(sys.extensions, vec!["md", "bin"]);
    }

    #[test]
    fn test_system_entry_name_fallback_order() {
        // Falls back: eu -> us -> wor -> jp
        let entry = SystemEntry {
            id: json!(1),
            noms: Some(json!({"nom_jp": "ファミコン", "nom_wor": "NES World"})),
            extensions: Some(".nes".to_string()),
            system_type: None,
            compagnie: None,
        };
        let sys = entry.to_system();
        assert_eq!(sys.name, "NES World");

        let entry2 = SystemEntry {
            id: json!(1),
            noms: Some(json!({"nom_jp": "ファミコン"})),
            extensions: None,
            system_type: None,
            compagnie: None,
        };
        assert_eq!(entry2.to_system().name, "ファミコン");
    }

    #[test]
    fn test_system_entry_missing_noms() {
        let entry = SystemEntry {
            id: json!(1),
            noms: None,
            extensions: None,
            system_type: None,
            compagnie: None,
        };
        let sys = entry.to_system();
        assert_eq!(sys.name, "Unknown");
        assert!(sys.extensions.is_empty());
    }

    #[test]
    fn test_system_entry_strips_dot_prefix() {
        let entry = SystemEntry {
            id: json!(1),
            noms: Some(json!({"nom_eu": "Test"})),
            extensions: Some(".sfc .smc".to_string()),
            system_type: None,
            compagnie: None,
        };
        assert_eq!(entry.to_system().extensions, vec!["sfc", "smc"]);
    }

    #[test]
    fn test_user_info_from_ss_user_defaults() {
        let ss_user = SsUser {
            id: None,
            num_id: None,
            niveau: None,
            contribution: None,
            uploadsysteme: None,
            uploadinfos: None,
            romassocaliases: None,
            uploadmedia: None,
            propositionok: None,
            propositionko: None,
            quotareq100: None,
            maxthreads: None,
            maxdownloadspeed: None,
            requeststoday: None,
            requestskotoday: None,
            maxrequestspermin: None,
            maxrequestsperday: None,
            maxrequestskoperday: None,
            visites: None,
            datedernierevisite: None,
            favregion: None,
        };
        let info: UserInfo = ss_user.into();
        assert!(info.username.is_empty());
        assert_eq!(info.max_threads, 1);
        assert_eq!(info.max_requests_per_min, 0);
        assert_eq!(info.requests_today, 0);
    }

    #[test]
    fn test_user_info_from_ss_user_with_values() {
        let ss_user = SsUser {
            id: Some("testuser".to_string()),
            num_id: Some("123".to_string()),
            niveau: Some("5".to_string()),
            contribution: Some("100".to_string()),
            uploadsysteme: None,
            uploadinfos: None,
            romassocaliases: None,
            uploadmedia: None,
            propositionok: None,
            propositionko: None,
            quotareq100: None,
            maxthreads: Some("4".to_string()),
            maxdownloadspeed: Some("1000".to_string()),
            requeststoday: Some("50".to_string()),
            requestskotoday: Some("2".to_string()),
            maxrequestspermin: Some("30".to_string()),
            maxrequestsperday: Some("5000".to_string()),
            maxrequestskoperday: None,
            visites: None,
            datedernierevisite: None,
            favregion: Some("us".to_string()),
        };
        let info: UserInfo = ss_user.into();
        assert_eq!(info.username, "testuser");
        assert_eq!(info.user_id, "123");
        assert_eq!(info.max_threads, 4);
        assert_eq!(info.max_download_speed, 1000);
        assert_eq!(info.requests_today, 50);
        assert_eq!(info.requests_ko_today, 2);
        assert_eq!(info.max_requests_per_min, 30);
        assert_eq!(info.favorite_region, "us");
    }

    #[test]
    fn test_infra_info_from_ss_infra() {
        let infra = SsInfra {
            cpu1: Some("25".to_string()),
            cpu2: Some("50".to_string()),
            cpu3: None,
            cpu4: None,
            threadsmin: Some("10".to_string()),
            nbscrapeurs: Some("5".to_string()),
            apiaccess: Some("1".to_string()),
            closefornomember: None,
            closeforlevel1member: None,
        };
        let info: InfraInfo = infra.into();
        assert_eq!(info.cpu_loads, vec!["CPU1: 25%", "CPU2: 50%"]);
        assert_eq!(info.active_threads, "10");
        assert_eq!(info.active_scrapers, "5");
        assert!(info.api_open);
    }

    #[test]
    fn test_infra_info_api_closed() {
        let infra = SsInfra {
            cpu1: None,
            cpu2: None,
            cpu3: None,
            cpu4: None,
            threadsmin: None,
            nbscrapeurs: None,
            apiaccess: Some("0".to_string()),
            closefornomember: None,
            closeforlevel1member: None,
        };
        let info: InfraInfo = infra.into();
        assert!(!info.api_open);
        assert!(info.cpu_loads.is_empty());
    }

    #[test]
    fn test_infra_info_all_cpus() {
        let infra = SsInfra {
            cpu1: Some("10".to_string()),
            cpu2: Some("20".to_string()),
            cpu3: Some("30".to_string()),
            cpu4: Some("40".to_string()),
            threadsmin: None,
            nbscrapeurs: None,
            apiaccess: None,
            closefornomember: None,
            closeforlevel1member: None,
        };
        let info: InfraInfo = infra.into();
        assert_eq!(info.cpu_loads.len(), 4);
        assert!(info.api_open); // None != Some("0")
    }

    #[test]
    fn test_system_entry_to_system_invalid_id() {
        let entry = SystemEntry {
            id: json!(true), // boolean - not number or string
            noms: Some(json!({"nom_eu": "Test"})),
            extensions: None,
            system_type: None,
            compagnie: None,
        };
        let sys = entry.to_system();
        assert_eq!(sys.id, 0); // Falls back to 0
    }

    #[test]
    fn test_system_entry_to_system_unparseable_string_id() {
        let entry = SystemEntry {
            id: json!("not_a_number"),
            noms: Some(json!({"nom_eu": "Test"})),
            extensions: None,
            system_type: None,
            compagnie: None,
        };
        let sys = entry.to_system();
        assert_eq!(sys.id, 0);
    }

    #[test]
    fn test_system_entry_to_system_empty_extensions() {
        let entry = SystemEntry {
            id: json!(1),
            noms: Some(json!({"nom_eu": "Test"})),
            extensions: Some(String::new()),
            system_type: None,
            compagnie: None,
        };
        let sys = entry.to_system();
        assert!(sys.extensions.is_empty());
    }

    #[test]
    fn test_system_entry_to_system_extensions_case() {
        let entry = SystemEntry {
            id: json!(1),
            noms: Some(json!({"nom_eu": "Test"})),
            extensions: Some(".SFC .SMC .ZIP".to_string()),
            system_type: None,
            compagnie: None,
        };
        let sys = entry.to_system();
        assert_eq!(sys.extensions, vec!["sfc", "smc", "zip"]);
    }

    #[test]
    fn test_system_entry_name_fallback_us() {
        let entry = SystemEntry {
            id: json!(1),
            noms: Some(json!({"nom_us": "Genesis", "nom_wor": "Mega Drive"})),
            extensions: None,
            system_type: None,
            compagnie: None,
        };
        // eu not present, falls to us
        assert_eq!(entry.to_system().name, "Genesis");
    }

    #[test]
    fn test_user_info_unparseable_numeric_fields() {
        let ss_user = SsUser {
            id: Some("user".to_string()),
            num_id: Some("abc".to_string()), // not a number
            niveau: None,
            contribution: None,
            uploadsysteme: None,
            uploadinfos: None,
            romassocaliases: None,
            uploadmedia: None,
            propositionok: None,
            propositionko: None,
            quotareq100: None,
            maxthreads: Some("abc".to_string()), // not a number
            maxdownloadspeed: None,
            requeststoday: None,
            requestskotoday: None,
            maxrequestspermin: None,
            maxrequestsperday: None,
            maxrequestskoperday: None,
            visites: None,
            datedernierevisite: None,
            favregion: None,
        };
        let info: UserInfo = ss_user.into();
        assert_eq!(info.max_threads, 1); // Falls back to 1
        assert_eq!(info.user_id, "abc"); // String field, not parsed
    }

    #[test]
    fn test_infra_info_none_apiaccess_is_open() {
        let infra = SsInfra {
            cpu1: None,
            cpu2: None,
            cpu3: None,
            cpu4: None,
            threadsmin: None,
            nbscrapeurs: None,
            apiaccess: None, // None != Some("0"), so API is open
            closefornomember: None,
            closeforlevel1member: None,
        };
        let info: InfraInfo = infra.into();
        assert!(info.api_open);
    }

    #[test]
    fn test_save_and_load_systems_cache() {
        use crate::models::system::System;
        let tmp = tempfile::tempdir().unwrap();
        let cache_dir = tmp.path();
        let systems = vec![System {
            id: 1,
            name: "Test System".to_string(),
            extensions: vec!["sfc".to_string()],
            system_type: "Console".to_string(),
            company: Some("TestCo".to_string()),
        }];
        // Save should succeed
        save_systems_cache(cache_dir, &systems).unwrap();

        // Load should return the data (fresh cache)
        let loaded = load_systems_cache(cache_dir).unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "Test System");
    }

    // --- Wiremock integration tests ---

    use crate::api::client::ScreenScraperClient;
    use crate::api::error::ApiError;
    use wiremock::matchers::{method, path};
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;

    fn mock_config() -> crate::config::Config {
        let mut config = crate::config::Config::default();
        config.credentials.devid = "testdev".to_string();
        config.credentials.devpassword = "testpass".to_string();
        config.credentials.softname = "scrauper".to_string();
        config.credentials.ssid = "user".to_string();
        config.credentials.sspassword = "userpass".to_string();
        config
    }

    #[tokio::test]
    async fn test_get_user_info_mock() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/ssuserInfos.php"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "response": {
                    "ssuser": {
                        "id": "testuser",
                        "numid": "42",
                        "niveau": "5",
                        "contribution": "100",
                        "maxthreads": "4",
                        "maxrequestspermin": "30",
                        "maxrequestsperday": "5000",
                        "requeststoday": "50",
                        "requestskotoday": "2",
                        "maxdownloadspeed": "1000",
                        "favregion": "us"
                    }
                }
            })))
            .mount(&server)
            .await;

        let config = mock_config();
        let client = ScreenScraperClient::with_base_url(&config, &server.uri()).unwrap();
        let info = client.get_user_info().await.unwrap();

        assert_eq!(info.username, "testuser");
        assert_eq!(info.user_id, "42");
        assert_eq!(info.level, "5");
        assert_eq!(info.contribution, "100");
        assert_eq!(info.max_threads, 4);
        assert_eq!(info.max_requests_per_min, 30);
        assert_eq!(info.max_requests_per_day, 5000);
        assert_eq!(info.requests_today, 50);
        assert_eq!(info.requests_ko_today, 2);
        assert_eq!(info.max_download_speed, 1000);
        assert_eq!(info.favorite_region, "us");
    }

    #[tokio::test]
    async fn test_get_infra_info_mock() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/ssinfraInfos.php"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "response": {
                    "ssinfra": {
                        "cpu1": "25",
                        "cpu2": "50",
                        "threadsmin": "10",
                        "nbscrapeurs": "5",
                        "apiaccess": "1"
                    }
                }
            })))
            .mount(&server)
            .await;

        let config = mock_config();
        let client = ScreenScraperClient::with_base_url(&config, &server.uri()).unwrap();
        let info = client.get_infra_info().await.unwrap();

        assert_eq!(info.cpu_loads.len(), 2);
        assert_eq!(info.cpu_loads[0], "CPU1: 25%");
        assert_eq!(info.cpu_loads[1], "CPU2: 50%");
        assert_eq!(info.active_threads, "10");
        assert_eq!(info.active_scrapers, "5");
        assert!(info.api_open);
    }

    #[tokio::test]
    async fn test_get_systems_list_mock() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/systemesListe.php"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "response": {
                    "systemes": [
                        {
                            "id": 1,
                            "noms": {"nom_eu": "Super Nintendo"},
                            "extensions": ".sfc .smc",
                            "type": "Console",
                            "compagnie": "Nintendo"
                        },
                        {
                            "id": 2,
                            "noms": {"nom_eu": "Mega Drive"},
                            "extensions": ".md .bin",
                            "type": "Console",
                            "compagnie": "Sega"
                        }
                    ]
                }
            })))
            .mount(&server)
            .await;

        let config = mock_config();
        let client = ScreenScraperClient::with_base_url(&config, &server.uri()).unwrap();
        let systems = client.get_systems_list().await.unwrap();

        assert_eq!(systems.len(), 2);
        assert_eq!(systems[0].id, 1);
        assert_eq!(systems[0].name, "Super Nintendo");
        assert_eq!(systems[0].extensions, vec!["sfc", "smc"]);
        assert_eq!(systems[0].system_type, "Console");
        assert_eq!(systems[0].company, Some("Nintendo".to_string()));
        assert_eq!(systems[1].id, 2);
        assert_eq!(systems[1].name, "Mega Drive");
        assert_eq!(systems[1].extensions, vec!["md", "bin"]);
        assert_eq!(systems[1].company, Some("Sega".to_string()));
    }

    #[tokio::test]
    async fn test_get_user_info_error() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/ssuserInfos.php"))
            .respond_with(ResponseTemplate::new(403).set_body_string("Forbidden"))
            .mount(&server)
            .await;

        let config = mock_config();
        let client = ScreenScraperClient::with_base_url(&config, &server.uri()).unwrap();
        let result = client.get_user_info().await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ApiError::InvalidCredentials));
    }

    #[tokio::test]
    async fn test_get_systems_list_empty() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/systemesListe.php"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "response": {
                    "systemes": []
                }
            })))
            .mount(&server)
            .await;

        let config = mock_config();
        let client = ScreenScraperClient::with_base_url(&config, &server.uri()).unwrap();
        let systems = client.get_systems_list().await.unwrap();

        assert!(systems.is_empty());
    }
}

use std::time::Duration;

use reqwest::Client;

use crate::config::Config;

use super::error::ApiError;

const BASE_URL: &str = "https://api.screenscraper.fr/api2";

/// HTTP client for the ScreenScraper API with authentication.
#[derive(Debug, Clone)]
pub struct ScreenScraperClient {
    http: Client,
    base_url: String,
    devid: String,
    devpassword: String,
    softname: String,
    ssid: String,
    sspassword: String,
}

impl ScreenScraperClient {
    pub fn new(config: &Config) -> Result<Self, ApiError> {
        let http = Client::builder()
            .timeout(Duration::from_secs(config.network.request_timeout_secs))
            .user_agent(&config.credentials.softname)
            .build()?;

        Ok(Self {
            http,
            base_url: BASE_URL.to_string(),
            devid: config.credentials.devid.clone(),
            devpassword: config.credentials.devpassword.clone(),
            softname: config.credentials.softname.clone(),
            ssid: config.credentials.ssid.clone(),
            sspassword: config.credentials.sspassword.clone(),
        })
    }

    /// Build the base query parameters for every API request.
    fn base_params(&self) -> Vec<(&str, String)> {
        let mut params: Vec<(&str, String)> = vec![
            ("devid", self.devid.clone()),
            ("devpassword", self.devpassword.clone()),
            ("softname", self.softname.clone()),
            ("output", "json".to_string()),
        ];
        if !self.ssid.is_empty() {
            params.push(("ssid", self.ssid.clone()));
        }
        if !self.sspassword.is_empty() {
            params.push(("sspassword", self.sspassword.clone()));
        }
        params
    }

    /// Perform a GET request to a ScreenScraper endpoint and parse JSON response.
    pub async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        endpoint: &str,
        extra_params: &[(&str, String)],
    ) -> Result<T, ApiError> {
        let url = format!("{}/{}", self.base_url, endpoint);
        let mut params = self.base_params();
        params.extend_from_slice(extra_params);

        tracing::debug!("API request: {} with {} params", endpoint, params.len());

        let response = self.http.get(&url).query(&params).send().await?;

        let status = response.status().as_u16();
        if status != 200 {
            let body = response.text().await.unwrap_or_default();
            return Err(ApiError::from_status(status, &body));
        }

        let body = response.text().await?;
        serde_json::from_str::<T>(&body)
            .map_err(|e| ApiError::Deserialize(format!("{}: {}", e, &body[..body.len().min(200)])))
    }

    /// Perform a raw GET request returning the response for streaming (media downloads).
    pub async fn get_raw(
        &self,
        endpoint: &str,
        extra_params: &[(&str, String)],
        timeout: Duration,
    ) -> Result<reqwest::Response, ApiError> {
        let url = format!("{}/{}", self.base_url, endpoint);
        let mut params = self.base_params();
        params.extend_from_slice(extra_params);

        let response = self.http.get(&url).query(&params).timeout(timeout).send().await?;

        let status = response.status().as_u16();
        if status != 200 {
            let body = response.text().await.unwrap_or_default();
            return Err(ApiError::from_status(status, &body));
        }

        Ok(response)
    }
}

#[cfg(test)]
impl ScreenScraperClient {
    /// Create a client pointing at a custom base URL (for mock server tests).
    pub fn with_base_url(config: &Config, base_url: &str) -> Result<Self, ApiError> {
        let mut client = Self::new(config)?;
        client.base_url = base_url.to_string();
        Ok(client)
    }

    fn test_base_params(&self) -> Vec<(&str, String)> {
        self.base_params()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn test_config() -> Config {
        let mut config = Config::default();
        config.credentials.devid = "testdev".to_string();
        config.credentials.devpassword = "testpass".to_string();
        config.credentials.softname = "scrauper".to_string();
        config.credentials.ssid = "user".to_string();
        config.credentials.sspassword = "userpass".to_string();
        config
    }

    #[test]
    fn test_new_client() {
        let config = test_config();
        let client = ScreenScraperClient::new(&config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_new_client_empty_creds() {
        let config = Config::default();
        // new() should NOT fail for empty creds - that's validated elsewhere
        // But Config::default() has empty devid/devpassword which is fine for client construction
        let result = ScreenScraperClient::new(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_base_params_includes_auth() {
        let config = test_config();
        let client = ScreenScraperClient::new(&config).unwrap();
        let params = client.test_base_params();

        assert!(params.iter().any(|(k, v)| *k == "devid" && v == "testdev"));
        assert!(params.iter().any(|(k, v)| *k == "devpassword" && v == "testpass"));
        assert!(params.iter().any(|(k, v)| *k == "softname" && v == "scrauper"));
        assert!(params.iter().any(|(k, v)| *k == "output" && v == "json"));
        assert!(params.iter().any(|(k, v)| *k == "ssid" && v == "user"));
        assert!(params.iter().any(|(k, v)| *k == "sspassword" && v == "userpass"));
    }

    #[test]
    fn test_base_params_omits_empty_ssid() {
        let mut config = test_config();
        config.credentials.ssid = String::new();
        config.credentials.sspassword = String::new();
        let client = ScreenScraperClient::new(&config).unwrap();
        let params = client.test_base_params();

        assert!(!params.iter().any(|(k, _)| *k == "ssid"));
        assert!(!params.iter().any(|(k, _)| *k == "sspassword"));
    }

    #[test]
    fn test_base_params_count() {
        let config = test_config();
        let client = ScreenScraperClient::new(&config).unwrap();
        let params = client.test_base_params();
        // devid + devpassword + softname + output + ssid + sspassword = 6
        assert_eq!(params.len(), 6);
    }

    #[test]
    fn test_base_params_count_no_user() {
        let mut config = test_config();
        config.credentials.ssid = String::new();
        config.credentials.sspassword = String::new();
        let client = ScreenScraperClient::new(&config).unwrap();
        let params = client.test_base_params();
        // devid + devpassword + softname + output = 4
        assert_eq!(params.len(), 4);
    }

    // --- wiremock integration tests ---

    use std::time::Duration;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;

    #[tokio::test]
    async fn test_get_json_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/test_endpoint.php"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"result": "ok"})),
            )
            .mount(&mock_server)
            .await;

        let config = test_config();
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();
        let result: serde_json::Value = client.get_json("test_endpoint.php", &[]).await.unwrap();

        assert_eq!(result, serde_json::json!({"result": "ok"}));
    }

    #[tokio::test]
    async fn test_get_json_404() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/test_endpoint.php"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let config = test_config();
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();
        let result: Result<serde_json::Value, _> = client.get_json("test_endpoint.php", &[]).await;

        assert!(matches!(result, Err(ApiError::GameNotFound)));
    }

    #[tokio::test]
    async fn test_get_json_429() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/test_endpoint.php"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&mock_server)
            .await;

        let config = test_config();
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();
        let result: Result<serde_json::Value, _> = client.get_json("test_endpoint.php", &[]).await;

        assert!(matches!(result, Err(ApiError::RateLimited)));
    }

    #[tokio::test]
    async fn test_get_json_403() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/test_endpoint.php"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&mock_server)
            .await;

        let config = test_config();
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();
        let result: Result<serde_json::Value, _> = client.get_json("test_endpoint.php", &[]).await;

        assert!(matches!(result, Err(ApiError::InvalidCredentials)));
    }

    #[tokio::test]
    async fn test_get_json_invalid_json() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/test_endpoint.php"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&mock_server)
            .await;

        let config = test_config();
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();
        let result: Result<serde_json::Value, _> = client.get_json("test_endpoint.php", &[]).await;

        assert!(matches!(result, Err(ApiError::Deserialize(_))));
    }

    #[tokio::test]
    async fn test_get_raw_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/media_endpoint.php"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"image data".to_vec()))
            .mount(&mock_server)
            .await;

        let config = test_config();
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();
        let response =
            client.get_raw("media_endpoint.php", &[], Duration::from_secs(10)).await.unwrap();
        let bytes = response.bytes().await.unwrap();

        assert_eq!(bytes.as_ref(), b"image data");
    }

    #[tokio::test]
    async fn test_get_raw_401() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/media_endpoint.php"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&mock_server)
            .await;

        let config = test_config();
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();
        let result = client.get_raw("media_endpoint.php", &[], Duration::from_secs(10)).await;

        assert!(matches!(result, Err(ApiError::ApiClosedOverloaded)));
    }

    #[tokio::test]
    async fn test_get_json_sends_auth_params() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/test_endpoint.php"))
            .and(query_param("devid", "testdev"))
            .and(query_param("devpassword", "testpass"))
            .and(query_param("softname", "scrauper"))
            .and(query_param("output", "json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
            .mount(&mock_server)
            .await;

        let config = test_config();
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();
        let result: serde_json::Value = client.get_json("test_endpoint.php", &[]).await.unwrap();

        assert_eq!(result, serde_json::json!({"ok": true}));
    }

    #[tokio::test]
    async fn test_get_json_sends_user_params() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/test_endpoint.php"))
            .and(query_param("ssid", "user"))
            .and(query_param("sspassword", "userpass"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
            .mount(&mock_server)
            .await;

        let config = test_config();
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();
        let result: serde_json::Value = client.get_json("test_endpoint.php", &[]).await.unwrap();

        assert_eq!(result, serde_json::json!({"ok": true}));
    }

    #[tokio::test]
    async fn test_get_raw_custom_timeout() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/media_endpoint.php"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"data".to_vec()))
            .mount(&mock_server)
            .await;

        let config = test_config();
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();
        let response =
            client.get_raw("media_endpoint.php", &[], Duration::from_mins(1)).await.unwrap();
        let bytes = response.bytes().await.unwrap();

        assert_eq!(bytes.as_ref(), b"data");
    }
}

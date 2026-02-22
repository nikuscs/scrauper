use serde::{Deserialize, Serialize};

/// A gaming system as returned by the ScreenScraper API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct System {
    pub id: u64,
    pub name: String,
    pub extensions: Vec<String>,
    pub system_type: String,
    pub company: Option<String>,
}

/// Cached systems list stored locally.
#[derive(Debug, Serialize, Deserialize)]
pub struct SystemsCache {
    pub fetched_at: String,
    pub systems: Vec<System>,
}

impl System {
    /// Check if this system supports a given file extension.
    #[allow(dead_code)]
    pub fn supports_extension(&self, ext: &str) -> bool {
        let ext_lower = ext.to_lowercase();
        self.extensions.iter().any(|e| e.to_lowercase() == ext_lower)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_system(exts: &[&str]) -> System {
        System {
            id: 1,
            name: "Test System".to_string(),
            extensions: exts.iter().copied().map(String::from).collect(),
            system_type: "Console".to_string(),
            company: Some("TestCo".to_string()),
        }
    }

    #[test]
    fn test_supports_extension_exact_match() {
        let sys = make_system(&["sfc", "smc", "zip"]);
        assert!(sys.supports_extension("sfc"));
        assert!(sys.supports_extension("zip"));
    }

    #[test]
    fn test_supports_extension_case_insensitive() {
        let sys = make_system(&["sfc", "smc"]);
        assert!(sys.supports_extension("SFC"));
        assert!(sys.supports_extension("Smc"));
    }

    #[test]
    fn test_supports_extension_not_found() {
        let sys = make_system(&["sfc", "smc"]);
        assert!(!sys.supports_extension("nes"));
        assert!(!sys.supports_extension(""));
    }

    #[test]
    fn test_supports_extension_empty_list() {
        let sys = make_system(&[]);
        assert!(!sys.supports_extension("sfc"));
    }

    #[test]
    fn test_system_serde_roundtrip() {
        let sys = make_system(&["sfc", "smc"]);
        let json = serde_json::to_string(&sys).unwrap();
        let back: System = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, 1);
        assert_eq!(back.name, "Test System");
        assert_eq!(back.extensions, vec!["sfc", "smc"]);
    }

    #[test]
    fn test_systems_cache_serde() {
        let cache = SystemsCache {
            fetched_at: "2024-01-01T00:00:00Z".to_string(),
            systems: vec![make_system(&["sfc"])],
        };
        let json = serde_json::to_string(&cache).unwrap();
        let back: SystemsCache = serde_json::from_str(&json).unwrap();
        assert_eq!(back.systems.len(), 1);
        assert_eq!(back.fetched_at, "2024-01-01T00:00:00Z");
    }
}

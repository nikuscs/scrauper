use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A ROM file discovered on disk.
#[derive(Debug, Clone)]
pub struct RomFile {
    pub path: PathBuf,
    pub filename: String,
    #[allow(dead_code)]
    pub file_size: u64,
    pub rom_type: RomType,
    /// For M3U games, the path of the first disc file (used for hashing).
    pub hash_target: Option<PathBuf>,
}

/// ROM type as expected by the ScreenScraper API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RomType {
    Rom,
    Iso,
    Dossier,
}

impl RomType {
    pub fn api_value(&self) -> &'static str {
        match self {
            RomType::Rom => "rom",
            RomType::Iso => "iso",
            RomType::Dossier => "dossier",
        }
    }
}

/// Computed hashes of a ROM file.
#[derive(Debug, Clone)]
pub struct RomHashes {
    pub crc32: String,
    pub md5: String,
    pub sha1: String,
    pub file_size: u64,
}

/// Scraped game metadata from the ScreenScraper API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapedGame {
    pub game_id: u64,
    pub rom_id: Option<u64>,
    pub name: String,
    pub description: Option<String>,
    pub rating: Option<f32>,
    pub release_date: Option<String>,
    pub developer: Option<String>,
    pub publisher: Option<String>,
    pub genre: Option<String>,
    pub players: Option<String>,
    pub system_id: u64,
    pub available_media: Vec<AvailableMedia>,
}

/// A media file available for download from the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailableMedia {
    pub media_type: String,
    pub url: String,
    pub region: Option<String>,
    pub format: Option<String>,
    pub crc: Option<String>,
}

/// Confidence level of a game match.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MatchConfidence {
    HashMatch,
    ExactNameMatch,
    FuzzyMatch(f32),
    #[allow(dead_code)]
    UserSelected,
}

impl MatchConfidence {
    #[allow(dead_code)]
    pub fn score(&self) -> f32 {
        match self {
            MatchConfidence::HashMatch | MatchConfidence::UserSelected => 1.0,
            MatchConfidence::ExactNameMatch => 0.95,
            MatchConfidence::FuzzyMatch(s) => *s,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rom_type_api_value() {
        assert_eq!(RomType::Rom.api_value(), "rom");
        assert_eq!(RomType::Iso.api_value(), "iso");
        assert_eq!(RomType::Dossier.api_value(), "dossier");
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_match_confidence_scores() {
        assert_eq!(MatchConfidence::HashMatch.score(), 1.0);
        assert_eq!(MatchConfidence::ExactNameMatch.score(), 0.95);
        assert_eq!(MatchConfidence::FuzzyMatch(0.7).score(), 0.7);
        assert_eq!(MatchConfidence::UserSelected.score(), 1.0);
    }

    #[test]
    fn test_match_confidence_ordering() {
        assert!(MatchConfidence::HashMatch.score() > MatchConfidence::ExactNameMatch.score());
        assert!(MatchConfidence::ExactNameMatch.score() > MatchConfidence::FuzzyMatch(0.7).score());
    }

    #[test]
    fn test_rom_type_serde() {
        let rt = RomType::Iso;
        let json = serde_json::to_string(&rt).unwrap();
        assert_eq!(json, "\"iso\"");
        let back: RomType = serde_json::from_str(&json).unwrap();
        assert_eq!(back, RomType::Iso);
    }

    #[test]
    fn test_rom_type_serde_all() {
        for (rt, expected) in &[
            (RomType::Rom, "\"rom\""),
            (RomType::Iso, "\"iso\""),
            (RomType::Dossier, "\"dossier\""),
        ] {
            let json = serde_json::to_string(rt).unwrap();
            assert_eq!(&json, expected);
            let back: RomType = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, rt);
        }
    }

    #[test]
    fn test_scraped_game_serde_roundtrip() {
        let game = ScrapedGame {
            game_id: 42,
            rom_id: Some(99),
            name: "Sonic".to_string(),
            description: Some("A hedgehog".to_string()),
            rating: Some(0.8),
            release_date: Some("19910623T000000".to_string()),
            developer: Some("Sonic Team".to_string()),
            publisher: Some("Sega".to_string()),
            genre: Some("Platform".to_string()),
            players: Some("1".to_string()),
            system_id: 1,
            available_media: vec![],
        };
        let json = serde_json::to_string(&game).unwrap();
        let back: ScrapedGame = serde_json::from_str(&json).unwrap();
        assert_eq!(back.game_id, 42);
        assert_eq!(back.rom_id, Some(99));
        assert_eq!(back.name, "Sonic");
        assert_eq!(back.description.as_deref(), Some("A hedgehog"));
    }

    #[test]
    fn test_scraped_game_minimal() {
        let game = ScrapedGame {
            game_id: 1,
            rom_id: None,
            name: "Minimal".to_string(),
            description: None,
            rating: None,
            release_date: None,
            developer: None,
            publisher: None,
            genre: None,
            players: None,
            system_id: 0,
            available_media: vec![],
        };
        let json = serde_json::to_string(&game).unwrap();
        let back: ScrapedGame = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "Minimal");
        assert!(back.description.is_none());
    }

    #[test]
    fn test_available_media_serde() {
        let media = AvailableMedia {
            media_type: "screenshot".to_string(),
            url: "https://example.com/ss.png".to_string(),
            region: Some("us".to_string()),
            format: Some("png".to_string()),
            crc: Some("ABCD1234".to_string()),
        };
        let json = serde_json::to_string(&media).unwrap();
        let back: AvailableMedia = serde_json::from_str(&json).unwrap();
        assert_eq!(back.media_type, "screenshot");
        assert_eq!(back.region.as_deref(), Some("us"));
        assert_eq!(back.crc.as_deref(), Some("ABCD1234"));
    }

    #[test]
    fn test_match_confidence_equality() {
        assert_eq!(MatchConfidence::HashMatch, MatchConfidence::HashMatch);
        assert_eq!(MatchConfidence::FuzzyMatch(0.5), MatchConfidence::FuzzyMatch(0.5));
        assert_ne!(MatchConfidence::HashMatch, MatchConfidence::ExactNameMatch);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_match_confidence_fuzzy_zero() {
        assert_eq!(MatchConfidence::FuzzyMatch(0.0).score(), 0.0);
    }
}

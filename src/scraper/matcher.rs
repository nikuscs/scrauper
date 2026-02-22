use std::path::Path;

use anyhow::Result;

use crate::api::client::ScreenScraperClient;
use crate::api::error::ApiError;
use crate::config::Config;
use crate::models::game::{MatchConfidence, RomFile, RomHashes, ScrapedGame};
use crate::models::region::{resolve_language_text, resolve_region_text, Region};

/// Look up a game via the ScreenScraper API.
/// Strategy: try hash-based lookup first, fall back to filename search.
pub async fn lookup_game(
    client: &ScreenScraperClient,
    rom: &RomFile,
    hashes: Option<&RomHashes>,
    system_id: u64,
    config: &Config,
    interactive: bool,
) -> Result<Option<(ScrapedGame, MatchConfidence)>, ApiError> {
    // Strategy 1: Hash lookup via jeuInfos
    if config.scraping.search.strategy.contains(&"hash".to_string()) {
        if let Some(hashes) = hashes {
            match hash_lookup(client, rom, hashes, system_id, config).await {
                Ok(game) => return Ok(Some((game, MatchConfidence::HashMatch))),
                Err(ApiError::GameNotFound) => {
                    tracing::debug!("Hash lookup failed for {}, trying filename", rom.filename);
                }
                Err(e) => return Err(e),
            }
        }
    }

    // Strategy 2: Filename search via jeuRecherche
    if config.scraping.search.strategy.contains(&"filename".to_string()) {
        match filename_lookup(client, rom, system_id, config, interactive).await {
            Ok(Some((game, confidence))) => return Ok(Some((game, confidence))),
            Ok(None) => {}
            Err(e) => return Err(e),
        }
    }

    Ok(None)
}

/// Hash-based game lookup via jeuInfos.php.
async fn hash_lookup(
    client: &ScreenScraperClient,
    rom: &RomFile,
    hashes: &RomHashes,
    system_id: u64,
    config: &Config,
) -> Result<ScrapedGame, ApiError> {
    let params = [
        ("crc", hashes.crc32.clone()),
        ("md5", hashes.md5.clone()),
        ("sha1", hashes.sha1.clone()),
        ("systemeid", system_id.to_string()),
        ("romtype", rom.rom_type.api_value().to_string()),
        ("romnom", rom.filename.clone()),
        ("romtaille", hashes.file_size.to_string()),
    ];

    let resp: serde_json::Value = client
        .get_json("jeuInfos.php", &params.iter().map(|(k, v)| (*k, v.clone())).collect::<Vec<_>>())
        .await?;

    parse_game_response(&resp, config)
}

/// Filename-based game search via jeuRecherche.php.
async fn filename_lookup(
    client: &ScreenScraperClient,
    rom: &RomFile,
    system_id: u64,
    config: &Config,
    interactive: bool,
) -> Result<Option<(ScrapedGame, MatchConfidence)>, ApiError> {
    let cleaned = clean_rom_filename(&rom.filename);
    if cleaned.is_empty() {
        return Ok(None);
    }

    let params: Vec<(&str, String)> =
        vec![("recherche", cleaned.clone()), ("systemeid", system_id.to_string())];

    let resp: serde_json::Value = match client.get_json("jeuRecherche.php", &params).await {
        Ok(r) => r,
        Err(ApiError::GameNotFound) => return Ok(None),
        Err(e) => return Err(e),
    };

    // Parse search results
    let games =
        resp.pointer("/response/jeux").and_then(|j| j.as_array()).cloned().unwrap_or_default();

    if games.is_empty() {
        return Ok(None);
    }

    // Parse all candidates and compute fuzzy scores
    let candidates: Vec<(ScrapedGame, f32)> = games
        .iter()
        .map(|g| {
            let game = parse_game_from_value(g, config);
            let score = fuzzy_score(&cleaned, &game.name);
            (game, score)
        })
        .collect();

    // Find best match
    let (best_idx, (best_game, best_score)) = candidates
        .iter()
        .enumerate()
        .max_by(|(_, (_, a)), (_, (_, b))| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map_or_else(
            || (0, (candidates[0].0.clone(), candidates[0].1)),
            |(i, (g, s))| (i, (g.clone(), *s)),
        );

    let threshold = config.scraping.min_auto_accept_confidence;

    // High confidence: auto-accept
    if best_score >= threshold || candidates.len() == 1 {
        let confidence = if best_score >= 0.95 {
            MatchConfidence::ExactNameMatch
        } else {
            MatchConfidence::FuzzyMatch(best_score)
        };
        return Ok(Some((best_game, confidence)));
    }

    // Low confidence with interactive mode: prompt user
    if interactive {
        return Ok(interactive_select(&rom.filename, &candidates, best_idx)
            .map(|(game, _)| (game, MatchConfidence::UserSelected)));
    }

    // Low confidence without interactive: warn and use best match anyway
    tracing::warn!(
        "Low confidence match for '{}': '{}' (score: {:.2}). Use --interactive to choose manually.",
        rom.filename,
        best_game.name,
        best_score
    );
    Ok(Some((best_game, MatchConfidence::FuzzyMatch(best_score))))
}

/// Compute fuzzy similarity between a cleaned ROM name and a game name.
pub fn fuzzy_score(rom_name: &str, game_name: &str) -> f32 {
    let a = normalize_for_comparison(rom_name);
    let b = normalize_for_comparison(game_name);

    if a == b {
        return 1.0;
    }

    strsim::normalized_damerau_levenshtein(&a, &b) as f32
}

/// Normalize a name for fuzzy comparison: lowercase, remove punctuation, collapse whitespace.
fn normalize_for_comparison(name: &str) -> String {
    let lower = name.to_lowercase();
    let cleaned: String =
        lower.chars().map(|c| if c.is_alphanumeric() || c == ' ' { c } else { ' ' }).collect();
    // Collapse multiple spaces
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Interactive selection using dialoguer.
#[allow(clippy::print_stderr)]
fn interactive_select(
    rom_filename: &str,
    candidates: &[(ScrapedGame, f32)],
    default_idx: usize,
) -> Option<(ScrapedGame, f32)> {
    let mut items: Vec<String> = candidates
        .iter()
        .map(|(game, score)| format!("{} (score: {:.2})", game.name, score))
        .collect();

    items.push("[ Skip this ROM ]".to_string());

    eprintln!();
    eprintln!("Multiple matches for: {rom_filename}");

    let selection = dialoguer::Select::new()
        .with_prompt("Select the correct match")
        .items(&items)
        .default(default_idx)
        .interact_opt();

    match selection {
        Ok(Some(idx)) if idx < candidates.len() => Some(candidates[idx].clone()),
        _ => None,
    }
}

/// Parse a full game response from jeuInfos.php.
fn parse_game_response(resp: &serde_json::Value, config: &Config) -> Result<ScrapedGame, ApiError> {
    let jeu = resp
        .pointer("/response/jeu")
        .ok_or_else(|| ApiError::Deserialize("Missing 'jeu' in response".to_string()))?;

    Ok(parse_game_from_value(jeu, config))
}

/// Parse a game object from a JSON value.
fn parse_game_from_value(jeu: &serde_json::Value, config: &Config) -> ScrapedGame {
    let game_id = jeu
        .get("id")
        .and_then(|v| match v {
            serde_json::Value::Number(n) => n.as_u64(),
            serde_json::Value::String(s) => s.parse().ok(),
            _ => None,
        })
        .unwrap_or(0);

    let rom_id = jeu.get("romid").and_then(|v| match v {
        serde_json::Value::Number(n) => n.as_u64(),
        serde_json::Value::String(s) => s.parse().ok(),
        _ => None,
    });

    let system_id = jeu
        .pointer("/systeme/id")
        .and_then(|v| match v {
            serde_json::Value::Number(n) => n.as_u64(),
            serde_json::Value::String(s) => s.parse().ok(),
            _ => None,
        })
        .unwrap_or(0);

    let regions: Vec<Region> = config.locale.region_priority.clone();

    // Name: noms.nom_{region}
    let name = jeu
        .get("noms")
        .and_then(|noms| resolve_region_text(noms, "nom", &regions))
        .unwrap_or_else(|| "Unknown".to_string());

    // Description: synopsis.synopsis_{language}
    let description = jeu
        .get("synopsis")
        .and_then(|syn| resolve_language_text(syn, "synopsis", config.locale.language));

    // Rating: note / 20.0
    let rating = jeu
        .get("note")
        .and_then(|v| match v {
            serde_json::Value::Number(n) => n.as_f64(),
            serde_json::Value::String(s) => s.parse::<f64>().ok(),
            serde_json::Value::Object(obj) => {
                obj.get("text").and_then(|t| t.as_str()).and_then(|s| s.parse::<f64>().ok())
            }
            _ => None,
        })
        .map(|n| (n / 20.0) as f32);

    // Release date: dates.date_{region} -> yyyyMMddT000000
    let release_date = jeu
        .get("dates")
        .and_then(|dates| resolve_region_text(dates, "date", &regions))
        .map(|d| format_release_date(&d));

    // Publisher/Developer
    let publisher = extract_text(jeu, "editeur");
    let developer = extract_text(jeu, "developpeur");

    // Genre: genres[].genre_{language}, joined
    let genre = jeu
        .get("genres")
        .and_then(|g| g.as_array())
        .map(|genres| {
            genres
                .iter()
                .filter_map(|g| resolve_language_text(g, "genre", config.locale.language))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|g| !g.is_empty());

    // Players
    let players = extract_text(jeu, "joueurs");

    ScrapedGame {
        game_id,
        rom_id,
        name,
        description,
        rating,
        release_date,
        developer,
        publisher,
        genre,
        players,
        system_id,
        available_media: Vec::new(),
    }
}

/// Extract a simple text field from the API response.
/// Handles both string values and objects with a "text" field.
fn extract_text(jeu: &serde_json::Value, field: &str) -> Option<String> {
    match jeu.get(field) {
        Some(serde_json::Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(serde_json::Value::Object(obj)) => obj
            .get("text")
            .and_then(|t| t.as_str())
            .filter(|s| !s.is_empty())
            .map(std::string::ToString::to_string),
        _ => None,
    }
}

/// Format a date string from the API (yyyy-mm-dd or similar) to ES-DE format (yyyyMMddT000000).
fn format_release_date(date: &str) -> String {
    // Remove any non-alphanumeric characters and pad
    let digits: String = date.chars().filter(char::is_ascii_digit).collect();
    if digits.len() >= 8 {
        format!("{}T000000", &digits[..8])
    } else if digits.len() >= 4 {
        format!("{}0101T000000", &digits[..4])
    } else {
        format!("{}T000000", digits)
    }
}

/// Clean a ROM filename for search:
/// Remove extension, region tags (USA), revision tags, hash tags, etc.
pub fn clean_rom_filename(filename: &str) -> String {
    let name = Path::new(filename).file_stem().and_then(|s| s.to_str()).unwrap_or(filename);

    // Remove common tags in parentheses and brackets
    let mut cleaned = name.to_string();
    // Remove (...) groups
    while let Some(start) = cleaned.find('(') {
        if let Some(end) = cleaned[start..].find(')') {
            cleaned = format!("{}{}", &cleaned[..start], &cleaned[start + end + 1..]);
        } else {
            break;
        }
    }
    // Remove [...] groups
    while let Some(start) = cleaned.find('[') {
        if let Some(end) = cleaned[start..].find(']') {
            cleaned = format!("{}{}", &cleaned[..start], &cleaned[start + end + 1..]);
        } else {
            break;
        }
    }

    cleaned.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_rom_filename_removes_extension_and_tags() {
        assert_eq!(clean_rom_filename("Sonic The Hedgehog (USA).zip"), "Sonic The Hedgehog");
        assert_eq!(clean_rom_filename("Mario (Europe) [!].sfc"), "Mario");
    }

    #[test]
    fn test_clean_rom_filename_multiple_tags() {
        assert_eq!(clean_rom_filename("Game (USA) (En,Fr) [!] [T+Eng1.0].bin"), "Game");
    }

    #[test]
    fn test_clean_rom_filename_no_tags() {
        assert_eq!(clean_rom_filename("SimpleGame.rom"), "SimpleGame");
    }

    #[test]
    fn test_clean_rom_filename_no_extension() {
        assert_eq!(clean_rom_filename("GameName"), "GameName");
    }

    #[test]
    fn test_format_release_date_full() {
        assert_eq!(format_release_date("2024-03-15"), "20240315T000000");
    }

    #[test]
    fn test_format_release_date_year_only() {
        assert_eq!(format_release_date("1994"), "19940101T000000");
    }

    #[test]
    fn test_format_release_date_no_separators() {
        assert_eq!(format_release_date("19940615"), "19940615T000000");
    }

    #[test]
    fn test_format_release_date_partial() {
        // 6 digits: only first 4 used as year, padded with 0101
        assert_eq!(format_release_date("199406"), "19940101T000000");
    }

    #[test]
    fn test_extract_text_string() {
        let jeu = serde_json::json!({"editeur": "Sega"});
        assert_eq!(extract_text(&jeu, "editeur"), Some("Sega".to_string()));
    }

    #[test]
    fn test_extract_text_object_with_text() {
        let jeu = serde_json::json!({"editeur": {"text": "Nintendo", "id": "123"}});
        assert_eq!(extract_text(&jeu, "editeur"), Some("Nintendo".to_string()));
    }

    #[test]
    fn test_extract_text_empty_string() {
        let jeu = serde_json::json!({"editeur": ""});
        assert_eq!(extract_text(&jeu, "editeur"), None);
    }

    #[test]
    fn test_extract_text_missing_field() {
        let jeu = serde_json::json!({"other": "value"});
        assert_eq!(extract_text(&jeu, "editeur"), None);
    }

    #[test]
    fn test_extract_text_object_empty_text() {
        let jeu = serde_json::json!({"editeur": {"text": "", "id": "123"}});
        assert_eq!(extract_text(&jeu, "editeur"), None);
    }

    #[test]
    fn test_parse_game_from_value_basic() {
        let config = Config::default();
        let jeu = serde_json::json!({
            "id": "1234",
            "romid": "5678",
            "systeme": {"id": "1"},
            "noms": {"nom_wor": "Sonic The Hedgehog"},
            "synopsis": {"synopsis_en": "A fast blue hedgehog"},
            "note": "16",
            "dates": {"date_wor": "1991-06-23"},
            "editeur": "Sega",
            "developpeur": "Sonic Team",
            "genres": [
                {"genre_en": "Platform"}
            ],
            "joueurs": "1"
        });

        let game = parse_game_from_value(&jeu, &config);
        assert_eq!(game.game_id, 1234);
        assert_eq!(game.rom_id, Some(5678));
        assert_eq!(game.name, "Sonic The Hedgehog");
        assert_eq!(game.description.as_deref(), Some("A fast blue hedgehog"));
        assert!((game.rating.unwrap() - 0.8).abs() < 0.01); // 16/20 = 0.8
        assert_eq!(game.release_date.as_deref(), Some("19910623T000000"));
        assert_eq!(game.publisher.as_deref(), Some("Sega"));
        assert_eq!(game.developer.as_deref(), Some("Sonic Team"));
        assert_eq!(game.genre.as_deref(), Some("Platform"));
        assert_eq!(game.players.as_deref(), Some("1"));
    }

    #[test]
    fn test_parse_game_from_value_numeric_id() {
        let config = Config::default();
        let jeu = serde_json::json!({
            "id": 42,
            "systeme": {"id": 1},
            "noms": {"nom_ss": "Test Game"}
        });

        let game = parse_game_from_value(&jeu, &config);
        assert_eq!(game.game_id, 42);
        assert_eq!(game.name, "Test Game");
    }

    #[test]
    fn test_parse_game_from_value_missing_optional_fields() {
        let config = Config::default();
        let jeu = serde_json::json!({
            "id": "1",
            "systeme": {"id": "1"},
            "noms": {"nom_wor": "Minimal Game"}
        });

        let game = parse_game_from_value(&jeu, &config);
        assert_eq!(game.name, "Minimal Game");
        assert!(game.description.is_none());
        assert!(game.rating.is_none());
        assert!(game.release_date.is_none());
        assert!(game.publisher.is_none());
        assert!(game.developer.is_none());
        assert!(game.genre.is_none());
        assert!(game.players.is_none());
    }

    // --- Fuzzy matching tests ---

    #[test]
    fn test_fuzzy_score_exact_match() {
        let score = fuzzy_score("Sonic The Hedgehog", "Sonic The Hedgehog");
        assert!((score - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_fuzzy_score_case_insensitive() {
        let score = fuzzy_score("sonic the hedgehog", "Sonic The Hedgehog");
        assert!((score - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_fuzzy_score_punctuation_ignored() {
        let score = fuzzy_score("Sonic The Hedgehog", "Sonic: The Hedgehog");
        assert!((score - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_fuzzy_score_similar_names() {
        let score = fuzzy_score("Sonic The Hedgehog 2", "Sonic The Hedgehog 3");
        assert!(score > 0.8, "Expected high similarity, got {}", score);
    }

    #[test]
    fn test_fuzzy_score_different_names() {
        let score = fuzzy_score("Sonic", "Mario");
        assert!(score < 0.5, "Expected low similarity, got {}", score);
    }

    #[test]
    fn test_normalize_for_comparison() {
        assert_eq!(normalize_for_comparison("Sonic: The Hedgehog"), "sonic the hedgehog");
        assert_eq!(normalize_for_comparison("Tom & Jerry"), "tom jerry");
        assert_eq!(normalize_for_comparison("  Multiple   Spaces  "), "multiple spaces");
    }

    #[test]
    fn test_normalize_for_comparison_special_chars() {
        assert_eq!(normalize_for_comparison("Crash Bandicoot - Warped!"), "crash bandicoot warped");
    }

    #[test]
    fn test_normalize_for_comparison_numbers() {
        assert_eq!(normalize_for_comparison("Super Mario 64"), "super mario 64");
    }

    #[test]
    fn test_normalize_for_comparison_empty() {
        assert_eq!(normalize_for_comparison(""), "");
    }

    #[test]
    fn test_fuzzy_score_empty_strings() {
        let score = fuzzy_score("", "");
        assert!((score - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_fuzzy_score_one_empty() {
        let score = fuzzy_score("Sonic", "");
        assert!(score < 0.5);
    }

    #[test]
    fn test_fuzzy_score_substring() {
        let score = fuzzy_score("Sonic", "Sonic The Hedgehog");
        assert!(score > 0.2, "Expected reasonable similarity, got {}", score);
    }

    #[test]
    fn test_clean_rom_filename_nested_parens() {
        assert_eq!(clean_rom_filename("Game (USA) (En,Fr).zip"), "Game");
    }

    #[test]
    fn test_clean_rom_filename_brackets_only() {
        assert_eq!(clean_rom_filename("Game [!].sfc"), "Game");
    }

    #[test]
    fn test_clean_rom_filename_unclosed_paren() {
        assert_eq!(clean_rom_filename("Game (USA.zip"), "Game (USA");
    }

    #[test]
    fn test_clean_rom_filename_spaces_trimmed() {
        assert_eq!(clean_rom_filename("Game   (USA).zip"), "Game");
    }

    #[test]
    fn test_clean_rom_filename_m3u() {
        assert_eq!(clean_rom_filename("Final Fantasy VII.m3u"), "Final Fantasy VII");
    }

    #[test]
    fn test_format_release_date_empty() {
        assert_eq!(format_release_date(""), "T000000");
    }

    #[test]
    fn test_format_release_date_short() {
        assert_eq!(format_release_date("19"), "19T000000");
    }

    #[test]
    fn test_extract_text_number() {
        let jeu = serde_json::json!({"editeur": 42});
        assert_eq!(extract_text(&jeu, "editeur"), None);
    }

    #[test]
    fn test_extract_text_null() {
        let jeu = serde_json::json!({"editeur": null});
        assert_eq!(extract_text(&jeu, "editeur"), None);
    }

    #[test]
    fn test_parse_game_response_missing_jeu() {
        let config = Config::default();
        let resp = serde_json::json!({"response": {}});
        let result = parse_game_response(&resp, &config);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_game_from_value_rating_as_object() {
        let config = Config::default();
        let jeu = serde_json::json!({
            "id": "1",
            "systeme": {"id": "1"},
            "noms": {"nom_wor": "Test"},
            "note": {"text": "15"}
        });
        let game = parse_game_from_value(&jeu, &config);
        assert!((game.rating.unwrap() - 0.75).abs() < 0.01); // 15/20
    }

    #[test]
    fn test_parse_game_from_value_multiple_genres() {
        let config = Config::default();
        let jeu = serde_json::json!({
            "id": "1",
            "systeme": {"id": "1"},
            "noms": {"nom_wor": "Test"},
            "genres": [
                {"genre_en": "Action"},
                {"genre_en": "Platform"}
            ]
        });
        let game = parse_game_from_value(&jeu, &config);
        assert_eq!(game.genre.as_deref(), Some("Action, Platform"));
    }

    #[test]
    fn test_parse_game_from_value_empty_genres() {
        let config = Config::default();
        let jeu = serde_json::json!({
            "id": "1",
            "systeme": {"id": "1"},
            "noms": {"nom_wor": "Test"},
            "genres": []
        });
        let game = parse_game_from_value(&jeu, &config);
        assert!(game.genre.is_none());
    }

    #[test]
    fn test_parse_game_from_value_no_id() {
        let config = Config::default();
        let jeu = serde_json::json!({
            "systeme": {},
            "noms": {"nom_wor": "Test"}
        });
        let game = parse_game_from_value(&jeu, &config);
        assert_eq!(game.game_id, 0);
        assert_eq!(game.system_id, 0);
    }

    #[test]
    fn test_parse_game_from_value_romid_as_string() {
        let config = Config::default();
        let jeu = serde_json::json!({
            "id": "1",
            "romid": "9999",
            "systeme": {"id": "1"},
            "noms": {"nom_wor": "Test"}
        });
        let game = parse_game_from_value(&jeu, &config);
        assert_eq!(game.rom_id, Some(9999));
    }

    #[test]
    fn test_parse_game_from_value_romid_as_number() {
        let config = Config::default();
        let jeu = serde_json::json!({
            "id": "1",
            "romid": 7777,
            "systeme": {"id": "1"},
            "noms": {"nom_wor": "Test"}
        });
        let game = parse_game_from_value(&jeu, &config);
        assert_eq!(game.rom_id, Some(7777));
    }

    #[test]
    fn test_parse_game_from_value_romid_missing() {
        let config = Config::default();
        let jeu = serde_json::json!({
            "id": "1",
            "systeme": {"id": "1"},
            "noms": {"nom_wor": "Test"}
        });
        let game = parse_game_from_value(&jeu, &config);
        assert!(game.rom_id.is_none());
    }

    #[test]
    fn test_parse_game_from_value_note_as_string() {
        let config = Config::default();
        let jeu = serde_json::json!({
            "id": "1",
            "systeme": {"id": "1"},
            "noms": {"nom_wor": "Test"},
            "note": "10"
        });
        let game = parse_game_from_value(&jeu, &config);
        assert!((game.rating.unwrap() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_parse_game_from_value_rating_as_float() {
        let config = Config::default();
        let jeu = serde_json::json!({
            "id": "1",
            "systeme": {"id": "1"},
            "noms": {"nom_wor": "Test"},
            "note": 20
        });
        let game = parse_game_from_value(&jeu, &config);
        assert!((game.rating.unwrap() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_game_from_value_rating_as_null() {
        let config = Config::default();
        let jeu = serde_json::json!({
            "id": "1",
            "systeme": {"id": "1"},
            "noms": {"nom_wor": "Test"},
            "note": null
        });
        let game = parse_game_from_value(&jeu, &config);
        assert!(game.rating.is_none());
    }

    #[test]
    fn test_parse_game_from_value_dates_from_region() {
        let mut config = Config::default();
        config.locale.region_priority =
            vec![crate::models::region::Region::Us, crate::models::region::Region::Jp];
        let jeu = serde_json::json!({
            "id": "1",
            "systeme": {"id": "1"},
            "noms": {"nom_us": "Game"},
            "dates": {"date_us": "1995-12-15", "date_jp": "1995-01-01"}
        });
        let game = parse_game_from_value(&jeu, &config);
        assert_eq!(game.release_date.as_deref(), Some("19951215T000000"));
    }

    #[test]
    fn test_parse_game_from_value_synopsis_language() {
        let mut config = Config::default();
        config.locale.language = crate::models::region::Language::Fr;
        let jeu = serde_json::json!({
            "id": "1",
            "systeme": {"id": "1"},
            "noms": {"nom_wor": "Game"},
            "synopsis": {"synopsis_fr": "Description en français", "synopsis_en": "English desc"}
        });
        let game = parse_game_from_value(&jeu, &config);
        assert_eq!(game.description.as_deref(), Some("Description en français"));
    }

    #[test]
    fn test_parse_game_from_value_joueurs_as_object() {
        let config = Config::default();
        let jeu = serde_json::json!({
            "id": "1",
            "systeme": {"id": "1"},
            "noms": {"nom_wor": "Test"},
            "joueurs": {"text": "1-4"}
        });
        let game = parse_game_from_value(&jeu, &config);
        assert_eq!(game.players.as_deref(), Some("1-4"));
    }

    #[test]
    fn test_parse_game_response_valid() {
        let config = Config::default();
        let resp = serde_json::json!({
            "response": {
                "jeu": {
                    "id": "42",
                    "systeme": {"id": "1"},
                    "noms": {"nom_wor": "Test Game"}
                }
            }
        });
        let game = parse_game_response(&resp, &config).unwrap();
        assert_eq!(game.game_id, 42);
        assert_eq!(game.name, "Test Game");
    }

    #[test]
    fn test_parse_game_from_value_system_id_number() {
        let config = Config::default();
        let jeu = serde_json::json!({
            "id": "1",
            "systeme": {"id": 75},
            "noms": {"nom_wor": "Test"}
        });
        let game = parse_game_from_value(&jeu, &config);
        assert_eq!(game.system_id, 75);
    }

    #[test]
    fn test_parse_game_from_value_system_id_invalid() {
        let config = Config::default();
        let jeu = serde_json::json!({
            "id": "1",
            "systeme": {"id": true},
            "noms": {"nom_wor": "Test"}
        });
        let game = parse_game_from_value(&jeu, &config);
        assert_eq!(game.system_id, 0);
    }

    #[test]
    fn test_extract_text_object_missing_text_field() {
        let jeu = serde_json::json!({"editeur": {"id": "123"}});
        assert_eq!(extract_text(&jeu, "editeur"), None);
    }

    #[test]
    fn test_format_release_date_with_dashes_and_spaces() {
        assert_eq!(format_release_date("1994 - 06 - 23"), "19940623T000000");
    }

    #[test]
    fn test_format_release_date_5_digits() {
        // Only 5 digits - first 4 used as year
        assert_eq!(format_release_date("19945"), "19940101T000000");
    }

    // --- Wiremock integration tests ---

    use crate::api::client::ScreenScraperClient;
    use crate::models::game::{MatchConfidence, RomFile, RomHashes, RomType};
    use std::path::PathBuf;
    use wiremock::matchers::{method, path};
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

    fn test_rom() -> RomFile {
        RomFile {
            path: PathBuf::from("Sonic The Hedgehog (USA).zip"),
            filename: "Sonic The Hedgehog (USA).zip".to_string(),
            file_size: 1000,
            rom_type: RomType::Rom,
            hash_target: None,
        }
    }

    fn test_hashes() -> RomHashes {
        RomHashes {
            crc32: "AABBCCDD".to_string(),
            md5: "aabbccdd11223344".to_string(),
            sha1: "aabbccdd1122334455667788".to_string(),
            file_size: 1000,
        }
    }

    #[tokio::test]
    async fn test_lookup_game_hash_match() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/jeuInfos.php"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "response": {
                    "jeu": {
                        "id": "1234",
                        "systeme": {"id": "1"},
                        "noms": {"nom_wor": "Sonic The Hedgehog"},
                        "editeur": "Sega"
                    }
                }
            })))
            .mount(&mock_server)
            .await;

        let config = mock_config();
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();
        let rom = test_rom();
        let hashes = test_hashes();

        let result = lookup_game(&client, &rom, Some(&hashes), 1, &config, false).await;

        let (game, confidence) = result.unwrap().expect("Expected Some game");
        assert_eq!(confidence, MatchConfidence::HashMatch);
        assert_eq!(game.name, "Sonic The Hedgehog");
        assert_eq!(game.game_id, 1234);
    }

    #[tokio::test]
    async fn test_lookup_game_hash_not_found_falls_to_filename() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/jeuInfos.php"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/jeuRecherche.php"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "response": {
                    "jeux": [{
                        "id": "5678",
                        "systeme": {"id": "1"},
                        "noms": {"nom_wor": "Sonic The Hedgehog"}
                    }]
                }
            })))
            .mount(&mock_server)
            .await;

        let config = mock_config();
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();
        let rom = test_rom();
        let hashes = test_hashes();

        let result = lookup_game(&client, &rom, Some(&hashes), 1, &config, false).await;

        let (game, confidence) = result.unwrap().expect("Expected Some game");
        assert!(
            matches!(confidence, MatchConfidence::FuzzyMatch(_) | MatchConfidence::ExactNameMatch),
            "Expected FuzzyMatch or ExactNameMatch, got {:?}",
            confidence
        );
        assert_eq!(game.name, "Sonic The Hedgehog");
    }

    #[tokio::test]
    async fn test_lookup_game_both_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/jeuInfos.php"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/jeuRecherche.php"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let config = mock_config();
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();
        let rom = test_rom();
        let hashes = test_hashes();

        let result = lookup_game(&client, &rom, Some(&hashes), 1, &config, false).await;

        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_lookup_game_filename_only_strategy() {
        let mock_server = MockServer::start().await;

        // This mock should never be called
        Mock::given(method("GET"))
            .and(path("/jeuInfos.php"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"response": {}})),
            )
            .expect(0)
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/jeuRecherche.php"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "response": {
                    "jeux": [{
                        "id": "9999",
                        "systeme": {"id": "1"},
                        "noms": {"nom_wor": "Sonic The Hedgehog"}
                    }]
                }
            })))
            .mount(&mock_server)
            .await;

        let mut config = mock_config();
        config.scraping.search.strategy = vec!["filename".to_string()];
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();
        let rom = test_rom();
        let hashes = test_hashes();

        let result = lookup_game(&client, &rom, Some(&hashes), 1, &config, false).await;

        let (game, _confidence) = result.unwrap().expect("Expected Some game");
        assert_eq!(game.name, "Sonic The Hedgehog");
    }

    #[tokio::test]
    async fn test_lookup_game_hash_only_strategy() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/jeuInfos.php"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        // filename endpoint should never be called
        Mock::given(method("GET"))
            .and(path("/jeuRecherche.php"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "response": {
                    "jeux": [{
                        "id": "1",
                        "systeme": {"id": "1"},
                        "noms": {"nom_wor": "Should Not Appear"}
                    }]
                }
            })))
            .expect(0)
            .mount(&mock_server)
            .await;

        let mut config = mock_config();
        config.scraping.search.strategy = vec!["hash".to_string()];
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();
        let rom = test_rom();
        let hashes = test_hashes();

        let result = lookup_game(&client, &rom, Some(&hashes), 1, &config, false).await;

        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_lookup_game_multiple_filename_results() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/jeuInfos.php"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/jeuRecherche.php"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "response": {
                    "jeux": [
                        {
                            "id": "1",
                            "systeme": {"id": "1"},
                            "noms": {"nom_wor": "Sonic The Hedgehog"}
                        },
                        {
                            "id": "2",
                            "systeme": {"id": "1"},
                            "noms": {"nom_wor": "Sonic The Hedgehog 2"}
                        }
                    ]
                }
            })))
            .mount(&mock_server)
            .await;

        let config = mock_config();
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();
        let rom = RomFile {
            path: PathBuf::from("Sonic The Hedgehog.zip"),
            filename: "Sonic The Hedgehog.zip".to_string(),
            file_size: 1000,
            rom_type: RomType::Rom,
            hash_target: None,
        };
        let hashes = test_hashes();

        let result = lookup_game(&client, &rom, Some(&hashes), 1, &config, false).await;

        let (game, _confidence) = result.unwrap().expect("Expected Some game");
        assert_eq!(game.name, "Sonic The Hedgehog");
    }

    #[tokio::test]
    async fn test_lookup_game_api_error_propagates() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/jeuInfos.php"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&mock_server)
            .await;

        let config = mock_config();
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();
        let rom = test_rom();
        let hashes = test_hashes();

        let result = lookup_game(&client, &rom, Some(&hashes), 1, &config, false).await;

        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), crate::api::error::ApiError::InvalidCredentials),
            "Expected InvalidCredentials error"
        );
    }

    #[tokio::test]
    async fn test_lookup_game_no_hashes_skips_hash_lookup() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/jeuInfos.php"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/jeuRecherche.php"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "response": {
                    "jeux": [{
                        "id": "42",
                        "systeme": {"id": "1"},
                        "noms": {"nom_wor": "Sonic The Hedgehog"}
                    }]
                }
            })))
            .mount(&mock_server)
            .await;

        let config = mock_config();
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();
        let rom = test_rom();

        let result = lookup_game(&client, &rom, None, 1, &config, false).await;

        let (game, _) = result.unwrap().expect("Expected Some game");
        assert_eq!(game.name, "Sonic The Hedgehog");
    }

    #[tokio::test]
    async fn test_lookup_game_empty_filename_returns_none() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/jeuInfos.php"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/jeuRecherche.php"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&mock_server)
            .await;

        let config = mock_config();
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();
        let rom = RomFile {
            path: PathBuf::from("(USA) [!].zip"),
            filename: "(USA) [!].zip".to_string(),
            file_size: 100,
            rom_type: RomType::Rom,
            hash_target: None,
        };
        let hashes = test_hashes();

        let result = lookup_game(&client, &rom, Some(&hashes), 1, &config, false).await;
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_lookup_game_filename_empty_results() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/jeuInfos.php"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/jeuRecherche.php"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "response": { "jeux": [] }
            })))
            .mount(&mock_server)
            .await;

        let config = mock_config();
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();
        let rom = test_rom();
        let hashes = test_hashes();

        let result = lookup_game(&client, &rom, Some(&hashes), 1, &config, false).await;
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_lookup_game_low_confidence_non_interactive() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/jeuInfos.php"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/jeuRecherche.php"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "response": {
                    "jeux": [
                        {
                            "id": "1",
                            "systeme": {"id": "1"},
                            "noms": {"nom_wor": "Completely Different Game Title XYZZY"}
                        },
                        {
                            "id": "2",
                            "systeme": {"id": "1"},
                            "noms": {"nom_wor": "Another Unrelated Game ABCDE"}
                        }
                    ]
                }
            })))
            .mount(&mock_server)
            .await;

        let config = mock_config();
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();
        let rom = test_rom();
        let hashes = test_hashes();

        let result = lookup_game(&client, &rom, Some(&hashes), 1, &config, false).await;
        let (game, confidence) =
            result.unwrap().expect("Expected Some game even with low confidence");
        assert!(
            matches!(confidence, MatchConfidence::FuzzyMatch(score) if score < 0.8),
            "Expected low FuzzyMatch, got {:?}",
            confidence
        );
        assert!(!game.name.is_empty());
    }

    #[tokio::test]
    async fn test_lookup_game_filename_error_propagates() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/jeuInfos.php"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/jeuRecherche.php"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&mock_server)
            .await;

        let config = mock_config();
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();
        let rom = test_rom();
        let hashes = test_hashes();

        let result = lookup_game(&client, &rom, Some(&hashes), 1, &config, false).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_lookup_game_single_result_auto_accepts() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/jeuInfos.php"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/jeuRecherche.php"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "response": {
                    "jeux": [{
                        "id": "1",
                        "systeme": {"id": "1"},
                        "noms": {"nom_wor": "Totally Different Name"}
                    }]
                }
            })))
            .mount(&mock_server)
            .await;

        let config = mock_config();
        let client = ScreenScraperClient::with_base_url(&config, &mock_server.uri()).unwrap();
        let rom = test_rom();
        let hashes = test_hashes();

        let result = lookup_game(&client, &rom, Some(&hashes), 1, &config, false).await;
        let (game, _) = result.unwrap().expect("Single result should be accepted");
        assert_eq!(game.name, "Totally Different Name");
    }
}

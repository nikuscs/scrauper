use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Region {
    Wor,
    Us,
    Eu,
    Jp,
    Fr,
    De,
    Es,
    Pt,
    Br,
    It,
    Nl,
    Se,
    Au,
    Ca,
    Uk,
    Kr,
    Cn,
    Tw,
    Asi,
}

impl Region {
    pub fn api_suffix(&self) -> &'static str {
        match self {
            Region::Wor => "wor",
            Region::Us => "us",
            Region::Eu => "eu",
            Region::Jp => "jp",
            Region::Fr => "fr",
            Region::De => "de",
            Region::Es => "es",
            Region::Pt => "pt",
            Region::Br => "br",
            Region::It => "it",
            Region::Nl => "nl",
            Region::Se => "se",
            Region::Au => "au",
            Region::Ca => "ca",
            Region::Uk => "uk",
            Region::Kr => "kr",
            Region::Cn => "cn",
            Region::Tw => "tw",
            Region::Asi => "asi",
        }
    }
}

impl fmt::Display for Region {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.api_suffix())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    En,
    Fr,
    De,
    Es,
    Pt,
    It,
    Nl,
    Sv,
    Ja,
    Ko,
    Zh,
    Da,
    Fi,
    No,
    Pl,
    Ru,
    Tr,
    Hu,
    Cs,
    Ro,
}

impl Language {
    pub fn api_suffix(&self) -> &'static str {
        match self {
            Language::En => "en",
            Language::Fr => "fr",
            Language::De => "de",
            Language::Es => "es",
            Language::Pt => "pt",
            Language::It => "it",
            Language::Nl => "nl",
            Language::Sv => "sv",
            Language::Ja => "ja",
            Language::Ko => "ko",
            Language::Zh => "zh",
            Language::Da => "da",
            Language::Fi => "fi",
            Language::No => "no",
            Language::Pl => "pl",
            Language::Ru => "ru",
            Language::Tr => "tr",
            Language::Hu => "hu",
            Language::Cs => "cs",
            Language::Ro => "ro",
        }
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.api_suffix())
    }
}

/// Resolve a value from a map of region-keyed entries using a priority fallback chain.
#[allow(dead_code)]
pub fn resolve_by_region<T: Clone>(available: &[(Region, T)], priority: &[Region]) -> Option<T> {
    for region in priority {
        if let Some((_, val)) = available.iter().find(|(r, _)| r == region) {
            return Some(val.clone());
        }
    }
    // Fall back to first available
    available.first().map(|(_, val)| val.clone())
}

/// Resolve a text value from a JSON value with region-keyed fields.
/// Handles two formats:
/// - Object: `{"nom_wor": "...", "nom_us": "..."}`
/// - Array: `[{"region": "wor", "text": "..."}, {"region": "us", "text": "..."}]`
pub fn resolve_region_text(
    obj: &serde_json::Value,
    prefix: &str,
    priority: &[Region],
) -> Option<String> {
    // Format 1: Array of {region, text} objects
    if let Some(arr) = obj.as_array() {
        for region in priority {
            let suffix = region.api_suffix();
            if let Some(entry) =
                arr.iter().find(|e| e.get("region").and_then(|r| r.as_str()) == Some(suffix))
            {
                if let Some(text) = entry.get("text").and_then(|t| t.as_str()) {
                    if !text.is_empty() {
                        return Some(text.to_string());
                    }
                }
            }
        }
        // Fall back to "ss" region
        if let Some(entry) =
            arr.iter().find(|e| e.get("region").and_then(|r| r.as_str()) == Some("ss"))
        {
            if let Some(text) = entry.get("text").and_then(|t| t.as_str()) {
                if !text.is_empty() {
                    return Some(text.to_string());
                }
            }
        }
        // Fall back to first entry with text
        return arr.iter().find_map(|e| {
            e.get("text")
                .and_then(|t| t.as_str())
                .filter(|s| !s.is_empty())
                .map(std::string::ToString::to_string)
        });
    }

    // Format 2: Object with "prefix_region" keys
    for region in priority {
        let key = format!("{}_{}", prefix, region.api_suffix());
        if let Some(serde_json::Value::String(val)) = obj.get(&key) {
            if !val.is_empty() {
                return Some(val.clone());
            }
        }
    }
    let ss_key = format!("{}_ss", prefix);
    if let Some(serde_json::Value::String(val)) = obj.get(&ss_key) {
        if !val.is_empty() {
            return Some(val.clone());
        }
    }
    None
}

/// Resolve a text value from a JSON value with language-keyed fields.
/// Handles two formats:
/// - Object: `{"synopsis_en": "..."}`
/// - Array: `[{"langue": "en", "text": "..."}]`
pub fn resolve_language_text(
    obj: &serde_json::Value,
    prefix: &str,
    language: Language,
) -> Option<String> {
    // Format 1: Array of {langue, text} objects
    if let Some(arr) = obj.as_array() {
        let suffix = language.api_suffix();
        if let Some(entry) =
            arr.iter().find(|e| e.get("langue").and_then(|l| l.as_str()) == Some(suffix))
        {
            if let Some(text) = entry.get("text").and_then(|t| t.as_str()) {
                if !text.is_empty() {
                    return Some(text.to_string());
                }
            }
        }
        // Fall back to English
        if language != Language::En {
            if let Some(entry) =
                arr.iter().find(|e| e.get("langue").and_then(|l| l.as_str()) == Some("en"))
            {
                if let Some(text) = entry.get("text").and_then(|t| t.as_str()) {
                    if !text.is_empty() {
                        return Some(text.to_string());
                    }
                }
            }
        }
        // Fall back to first entry
        return arr.iter().find_map(|e| {
            e.get("text")
                .and_then(|t| t.as_str())
                .filter(|s| !s.is_empty())
                .map(std::string::ToString::to_string)
        });
    }

    // Format 2: Object with "prefix_language" keys
    let key = format!("{}_{}", prefix, language.api_suffix());
    if let Some(serde_json::Value::String(val)) = obj.get(&key) {
        if !val.is_empty() {
            return Some(val.clone());
        }
    }
    if language != Language::En {
        let en_key = format!("{}_en", prefix);
        if let Some(serde_json::Value::String(val)) = obj.get(&en_key) {
            if !val.is_empty() {
                return Some(val.clone());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_region_api_suffix() {
        assert_eq!(Region::Wor.api_suffix(), "wor");
        assert_eq!(Region::Us.api_suffix(), "us");
        assert_eq!(Region::Jp.api_suffix(), "jp");
    }

    #[test]
    fn test_language_api_suffix() {
        assert_eq!(Language::En.api_suffix(), "en");
        assert_eq!(Language::Fr.api_suffix(), "fr");
        assert_eq!(Language::Ja.api_suffix(), "ja");
    }

    #[test]
    fn test_region_display() {
        assert_eq!(format!("{}", Region::Us), "us");
        assert_eq!(format!("{}", Region::Eu), "eu");
    }

    #[test]
    fn test_resolve_by_region_exact_match() {
        let available =
            vec![(Region::Us, "US Name".to_string()), (Region::Jp, "JP Name".to_string())];
        let priority = vec![Region::Us, Region::Eu, Region::Jp];
        assert_eq!(resolve_by_region(&available, &priority), Some("US Name".to_string()));
    }

    #[test]
    fn test_resolve_by_region_fallback_order() {
        let available =
            vec![(Region::Jp, "JP Name".to_string()), (Region::Fr, "FR Name".to_string())];
        let priority = vec![Region::Us, Region::Eu, Region::Jp];
        assert_eq!(resolve_by_region(&available, &priority), Some("JP Name".to_string()));
    }

    #[test]
    fn test_resolve_by_region_no_priority_match_returns_first() {
        let available =
            vec![(Region::Fr, "FR Name".to_string()), (Region::De, "DE Name".to_string())];
        let priority = vec![Region::Us, Region::Eu];
        assert_eq!(resolve_by_region(&available, &priority), Some("FR Name".to_string()));
    }

    #[test]
    fn test_resolve_by_region_empty_available() {
        let available: Vec<(Region, String)> = vec![];
        let priority = vec![Region::Us];
        assert_eq!(resolve_by_region(&available, &priority), None);
    }

    #[test]
    fn test_resolve_region_text_priority() {
        let obj = json!({
            "nom_us": "Sonic",
            "nom_jp": "ソニック",
            "nom_wor": "Sonic World"
        });
        let priority = vec![Region::Wor, Region::Us, Region::Jp];
        assert_eq!(resolve_region_text(&obj, "nom", &priority), Some("Sonic World".to_string()));
    }

    #[test]
    fn test_resolve_region_text_skips_empty() {
        let obj = json!({
            "nom_us": "",
            "nom_jp": "ソニック"
        });
        let priority = vec![Region::Us, Region::Jp];
        assert_eq!(resolve_region_text(&obj, "nom", &priority), Some("ソニック".to_string()));
    }

    #[test]
    fn test_resolve_region_text_ss_fallback() {
        let obj = json!({
            "nom_ss": "Internal Name"
        });
        let priority = vec![Region::Us, Region::Eu];
        assert_eq!(resolve_region_text(&obj, "nom", &priority), Some("Internal Name".to_string()));
    }

    #[test]
    fn test_resolve_region_text_none_when_missing() {
        let obj = json!({"unrelated": "value"});
        let priority = vec![Region::Us];
        assert_eq!(resolve_region_text(&obj, "nom", &priority), None);
    }

    #[test]
    fn test_resolve_language_text_exact() {
        let obj = json!({
            "synopsis_fr": "Description en français",
            "synopsis_en": "English description"
        });
        assert_eq!(
            resolve_language_text(&obj, "synopsis", Language::Fr),
            Some("Description en français".to_string())
        );
    }

    #[test]
    fn test_resolve_language_text_english_fallback() {
        let obj = json!({
            "synopsis_en": "English description"
        });
        assert_eq!(
            resolve_language_text(&obj, "synopsis", Language::De),
            Some("English description".to_string())
        );
    }

    #[test]
    fn test_resolve_language_text_no_fallback_for_english() {
        let obj = json!({
            "synopsis_fr": "Only French"
        });
        assert_eq!(resolve_language_text(&obj, "synopsis", Language::En), None);
    }

    #[test]
    fn test_resolve_language_text_skips_empty() {
        let obj = json!({
            "synopsis_de": "",
            "synopsis_en": "English"
        });
        assert_eq!(
            resolve_language_text(&obj, "synopsis", Language::De),
            Some("English".to_string())
        );
    }

    #[test]
    fn test_region_serde_roundtrip() {
        let region = Region::Us;
        let json = serde_json::to_string(&region).unwrap();
        assert_eq!(json, "\"us\"");
        let back: Region = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Region::Us);
    }

    #[test]
    fn test_language_serde_roundtrip() {
        let lang = Language::Ja;
        let json = serde_json::to_string(&lang).unwrap();
        assert_eq!(json, "\"ja\"");
        let back: Language = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Language::Ja);
    }

    #[test]
    fn test_language_display() {
        assert_eq!(format!("{}", Language::En), "en");
        assert_eq!(format!("{}", Language::Fr), "fr");
        assert_eq!(format!("{}", Language::De), "de");
    }

    #[test]
    fn test_all_region_suffixes() {
        let expected = vec![
            (Region::Wor, "wor"),
            (Region::Us, "us"),
            (Region::Eu, "eu"),
            (Region::Jp, "jp"),
            (Region::Fr, "fr"),
            (Region::De, "de"),
            (Region::Es, "es"),
            (Region::Pt, "pt"),
            (Region::Br, "br"),
            (Region::It, "it"),
            (Region::Nl, "nl"),
            (Region::Se, "se"),
            (Region::Au, "au"),
            (Region::Ca, "ca"),
            (Region::Uk, "uk"),
            (Region::Kr, "kr"),
            (Region::Cn, "cn"),
            (Region::Tw, "tw"),
            (Region::Asi, "asi"),
        ];
        for (region, suffix) in expected {
            assert_eq!(region.api_suffix(), suffix);
        }
    }

    #[test]
    fn test_all_language_suffixes() {
        let expected = vec![
            (Language::En, "en"),
            (Language::Fr, "fr"),
            (Language::De, "de"),
            (Language::Es, "es"),
            (Language::Pt, "pt"),
            (Language::It, "it"),
            (Language::Nl, "nl"),
            (Language::Sv, "sv"),
            (Language::Ja, "ja"),
            (Language::Ko, "ko"),
            (Language::Zh, "zh"),
            (Language::Da, "da"),
            (Language::Fi, "fi"),
            (Language::No, "no"),
            (Language::Pl, "pl"),
            (Language::Ru, "ru"),
            (Language::Tr, "tr"),
            (Language::Hu, "hu"),
            (Language::Cs, "cs"),
            (Language::Ro, "ro"),
        ];
        for (lang, suffix) in expected {
            assert_eq!(lang.api_suffix(), suffix);
        }
    }

    #[test]
    fn test_region_serde_all_variants() {
        let regions = vec![
            Region::Wor,
            Region::Us,
            Region::Eu,
            Region::Jp,
            Region::Fr,
            Region::De,
            Region::Br,
            Region::Kr,
            Region::Asi,
        ];
        for region in regions {
            let json = serde_json::to_string(&region).unwrap();
            let back: Region = serde_json::from_str(&json).unwrap();
            assert_eq!(back, region);
        }
    }

    #[test]
    fn test_language_serde_all_variants() {
        let languages = vec![
            Language::En,
            Language::Fr,
            Language::De,
            Language::Es,
            Language::Ja,
            Language::Ko,
            Language::Zh,
            Language::Ru,
            Language::Pl,
            Language::Ro,
        ];
        for lang in languages {
            let json = serde_json::to_string(&lang).unwrap();
            let back: Language = serde_json::from_str(&json).unwrap();
            assert_eq!(back, lang);
        }
    }

    #[test]
    fn test_resolve_language_text_empty_value() {
        let obj = json!({
            "synopsis_en": ""
        });
        // Empty string should be skipped
        assert_eq!(resolve_language_text(&obj, "synopsis", Language::En), None);
    }

    #[test]
    fn test_resolve_region_text_empty_priority() {
        let obj = json!({
            "nom_us": "Sonic"
        });
        let priority: Vec<Region> = vec![];
        // With empty priority, falls back to _ss key (which doesn't exist)
        assert_eq!(resolve_region_text(&obj, "nom", &priority), None);
    }

    #[test]
    fn test_resolve_by_region_single_entry() {
        let available = vec![(Region::Jp, "JP".to_string())];
        let priority = vec![Region::Jp];
        assert_eq!(resolve_by_region(&available, &priority), Some("JP".to_string()));
    }

    #[test]
    fn test_resolve_region_text_array_ss_fallback() {
        let arr = json!([
            {"region": "ss", "text": "Internal Name"},
            {"region": "jp", "text": "JP Name"}
        ]);
        let priority = vec![Region::Us, Region::Eu];
        assert_eq!(resolve_region_text(&arr, "nom", &priority), Some("Internal Name".to_string()));
    }

    #[test]
    fn test_resolve_region_text_array_first_non_empty_fallback() {
        let arr = json!([
            {"region": "xx", "text": ""},
            {"region": "yy", "text": "Fallback Name"}
        ]);
        let priority = vec![Region::Us];
        assert_eq!(resolve_region_text(&arr, "nom", &priority), Some("Fallback Name".to_string()));
    }

    #[test]
    fn test_resolve_language_text_array_english_fallback() {
        let arr = json!([
            {"langue": "en", "text": "English text"},
            {"langue": "fr", "text": "French text"}
        ]);
        assert_eq!(
            resolve_language_text(&arr, "synopsis", Language::De),
            Some("English text".to_string())
        );
    }

    #[test]
    fn test_resolve_language_text_array_first_non_empty_fallback() {
        let arr = json!([
            {"langue": "fr", "text": ""},
            {"langue": "jp", "text": "Japanese text"}
        ]);
        assert_eq!(
            resolve_language_text(&arr, "synopsis", Language::En),
            Some("Japanese text".to_string())
        );
    }
}

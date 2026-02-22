use serde::{Deserialize, Serialize};

/// A single game entry in gamelist.xml.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GameListEntry {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rating: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub releasedate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub developer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub players: Option<String>,
    // User-managed fields (never overwritten by scraper)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playcount: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lastplayed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub favorite: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kidgame: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub altemulator: Option<String>,
}

/// The full gamelist.xml structure.
#[derive(Debug, Clone, Default)]
pub struct GameList {
    pub games: Vec<GameListEntry>,
}

/// User-managed fields that should never be overwritten during scraping.
const PRESERVED_FIELDS: &[&str] =
    &["playcount", "lastplayed", "favorite", "kidgame", "hidden", "altemulator"];

impl GameList {
    /// Find a game entry by its ROM path.
    pub fn find_by_path(&self, path: &str) -> Option<&GameListEntry> {
        self.games.iter().find(|g| g.path == path)
    }

    /// Find a mutable game entry by its ROM path.
    pub fn find_by_path_mut(&mut self, path: &str) -> Option<&mut GameListEntry> {
        self.games.iter_mut().find(|g| g.path == path)
    }

    /// Merge scraped data into an existing entry, preserving user fields.
    /// If force is true, overwrite existing scraped fields.
    pub fn merge_or_insert(&mut self, path: &str, scraped: &GameListEntry, force: bool) {
        if let Some(existing) = self.find_by_path_mut(path) {
            merge_entry(existing, scraped, force);
        } else {
            let mut entry = scraped.clone();
            entry.path = path.to_string();
            self.games.push(entry);
        }
    }
}

fn merge_entry(existing: &mut GameListEntry, scraped: &GameListEntry, force: bool) {
    macro_rules! merge_field {
        ($field:ident) => {
            if scraped.$field.is_some() && (force || existing.$field.is_none()) {
                // Don't overwrite preserved fields
                let field_name = stringify!($field);
                if !PRESERVED_FIELDS.contains(&field_name) {
                    existing.$field = scraped.$field.clone();
                }
            }
        };
    }

    merge_field!(name);
    merge_field!(desc);
    merge_field!(rating);
    merge_field!(releasedate);
    merge_field!(developer);
    merge_field!(publisher);
    merge_field!(genre);
    merge_field!(players);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(path: &str, name: &str) -> GameListEntry {
        GameListEntry { path: path.to_string(), name: Some(name.to_string()), ..Default::default() }
    }

    #[test]
    fn test_find_by_path() {
        let gl = GameList {
            games: vec![make_entry("./sonic.zip", "Sonic"), make_entry("./mario.zip", "Mario")],
        };
        assert_eq!(gl.find_by_path("./sonic.zip").unwrap().name.as_deref(), Some("Sonic"));
        assert!(gl.find_by_path("./missing.zip").is_none());
    }

    #[test]
    fn test_merge_or_insert_new_game() {
        let mut gl = GameList::default();
        let scraped = GameListEntry {
            path: String::new(),
            name: Some("Sonic".to_string()),
            desc: Some("Fast hedgehog".to_string()),
            ..Default::default()
        };
        gl.merge_or_insert("./sonic.zip", &scraped, false);

        assert_eq!(gl.games.len(), 1);
        assert_eq!(gl.games[0].path, "./sonic.zip");
        assert_eq!(gl.games[0].name.as_deref(), Some("Sonic"));
        assert_eq!(gl.games[0].desc.as_deref(), Some("Fast hedgehog"));
    }

    #[test]
    fn test_merge_preserves_existing_fields_no_force() {
        let mut gl = GameList {
            games: vec![GameListEntry {
                path: "./sonic.zip".to_string(),
                name: Some("Sonic Old".to_string()),
                desc: None,
                ..Default::default()
            }],
        };

        let scraped = GameListEntry {
            path: String::new(),
            name: Some("Sonic New".to_string()),
            desc: Some("New description".to_string()),
            ..Default::default()
        };

        gl.merge_or_insert("./sonic.zip", &scraped, false);

        // Name should NOT be overwritten (already had a value, force=false)
        assert_eq!(gl.games[0].name.as_deref(), Some("Sonic Old"));
        // Desc should be filled (was None)
        assert_eq!(gl.games[0].desc.as_deref(), Some("New description"));
    }

    #[test]
    fn test_merge_overwrites_with_force() {
        let mut gl = GameList {
            games: vec![GameListEntry {
                path: "./sonic.zip".to_string(),
                name: Some("Sonic Old".to_string()),
                ..Default::default()
            }],
        };

        let scraped = GameListEntry {
            path: String::new(),
            name: Some("Sonic New".to_string()),
            ..Default::default()
        };

        gl.merge_or_insert("./sonic.zip", &scraped, true);

        assert_eq!(gl.games[0].name.as_deref(), Some("Sonic New"));
    }

    #[test]
    fn test_merge_never_overwrites_user_fields() {
        let mut gl = GameList {
            games: vec![GameListEntry {
                path: "./sonic.zip".to_string(),
                playcount: Some("5".to_string()),
                favorite: Some("true".to_string()),
                lastplayed: Some("20240101T000000".to_string()),
                kidgame: Some("false".to_string()),
                hidden: Some("false".to_string()),
                altemulator: Some("retroarch".to_string()),
                ..Default::default()
            }],
        };

        let scraped = GameListEntry {
            path: String::new(),
            playcount: Some("0".to_string()),
            favorite: Some("false".to_string()),
            lastplayed: Some("99999999T000000".to_string()),
            kidgame: Some("true".to_string()),
            hidden: Some("true".to_string()),
            altemulator: Some("other".to_string()),
            name: Some("Sonic".to_string()),
            ..Default::default()
        };

        gl.merge_or_insert("./sonic.zip", &scraped, true);

        // User fields preserved even with force=true
        assert_eq!(gl.games[0].playcount.as_deref(), Some("5"));
        assert_eq!(gl.games[0].favorite.as_deref(), Some("true"));
        assert_eq!(gl.games[0].lastplayed.as_deref(), Some("20240101T000000"));
        assert_eq!(gl.games[0].kidgame.as_deref(), Some("false"));
        assert_eq!(gl.games[0].hidden.as_deref(), Some("false"));
        assert_eq!(gl.games[0].altemulator.as_deref(), Some("retroarch"));
    }

    #[test]
    fn test_default_entry_all_none() {
        let entry = GameListEntry::default();
        assert!(entry.path.is_empty());
        assert!(entry.name.is_none());
        assert!(entry.desc.is_none());
        assert!(entry.playcount.is_none());
    }

    #[test]
    fn test_find_by_path_mut() {
        let mut gl = GameList {
            games: vec![make_entry("./sonic.zip", "Sonic"), make_entry("./mario.zip", "Mario")],
        };
        let entry = gl.find_by_path_mut("./sonic.zip").unwrap();
        entry.name = Some("Sonic Updated".to_string());
        assert_eq!(gl.games[0].name.as_deref(), Some("Sonic Updated"));
    }

    #[test]
    fn test_find_by_path_mut_not_found() {
        let mut gl = GameList { games: vec![make_entry("./sonic.zip", "Sonic")] };
        assert!(gl.find_by_path_mut("./missing.zip").is_none());
    }

    #[test]
    fn test_merge_fills_all_fields() {
        let mut gl = GameList::default();
        let scraped = GameListEntry {
            path: String::new(),
            name: Some("Sonic".to_string()),
            desc: Some("Fast hedgehog".to_string()),
            rating: Some("0.8".to_string()),
            releasedate: Some("19910623T000000".to_string()),
            developer: Some("Sonic Team".to_string()),
            publisher: Some("Sega".to_string()),
            genre: Some("Platform".to_string()),
            players: Some("1".to_string()),
            ..Default::default()
        };
        gl.merge_or_insert("./sonic.zip", &scraped, false);

        let g = &gl.games[0];
        assert_eq!(g.path, "./sonic.zip");
        assert_eq!(g.name.as_deref(), Some("Sonic"));
        assert_eq!(g.desc.as_deref(), Some("Fast hedgehog"));
        assert_eq!(g.rating.as_deref(), Some("0.8"));
        assert_eq!(g.releasedate.as_deref(), Some("19910623T000000"));
        assert_eq!(g.developer.as_deref(), Some("Sonic Team"));
        assert_eq!(g.publisher.as_deref(), Some("Sega"));
        assert_eq!(g.genre.as_deref(), Some("Platform"));
        assert_eq!(g.players.as_deref(), Some("1"));
    }

    #[test]
    fn test_merge_empty_scraped_does_not_clear() {
        let mut gl = GameList {
            games: vec![GameListEntry {
                path: "./sonic.zip".to_string(),
                name: Some("Sonic".to_string()),
                desc: Some("Original".to_string()),
                ..Default::default()
            }],
        };

        let scraped = GameListEntry::default();
        gl.merge_or_insert("./sonic.zip", &scraped, false);

        // Nothing changes since scraped has all None
        assert_eq!(gl.games[0].name.as_deref(), Some("Sonic"));
        assert_eq!(gl.games[0].desc.as_deref(), Some("Original"));
    }

    #[test]
    fn test_merge_force_overwrites_all_scraped_fields() {
        let mut gl = GameList {
            games: vec![GameListEntry {
                path: "./sonic.zip".to_string(),
                name: Some("Old Name".to_string()),
                desc: Some("Old Desc".to_string()),
                rating: Some("0.5".to_string()),
                developer: Some("Old Dev".to_string()),
                ..Default::default()
            }],
        };

        let scraped = GameListEntry {
            path: String::new(),
            name: Some("New Name".to_string()),
            desc: Some("New Desc".to_string()),
            rating: Some("0.9".to_string()),
            developer: Some("New Dev".to_string()),
            ..Default::default()
        };

        gl.merge_or_insert("./sonic.zip", &scraped, true);

        assert_eq!(gl.games[0].name.as_deref(), Some("New Name"));
        assert_eq!(gl.games[0].desc.as_deref(), Some("New Desc"));
        assert_eq!(gl.games[0].rating.as_deref(), Some("0.9"));
        assert_eq!(gl.games[0].developer.as_deref(), Some("New Dev"));
    }

    #[test]
    fn test_gamelist_entry_serde_roundtrip() {
        let entry = GameListEntry {
            path: "./sonic.zip".to_string(),
            name: Some("Sonic".to_string()),
            desc: Some("A game".to_string()),
            rating: Some("0.8".to_string()),
            ..Default::default()
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: GameListEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.path, "./sonic.zip");
        assert_eq!(back.name.as_deref(), Some("Sonic"));
        // None fields should not be in JSON
        assert!(!json.contains("playcount"));
    }

    #[test]
    fn test_gamelist_default() {
        let gl = GameList::default();
        assert!(gl.games.is_empty());
    }
}

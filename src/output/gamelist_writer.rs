use std::io::BufRead;
use std::path::Path;

use anyhow::{Context, Result};

use crate::models::game::ScrapedGame;
use crate::models::gamelist::{GameList, GameListEntry};

/// Read an existing gamelist.xml file.
pub fn read_gamelist(path: &Path) -> Result<GameList> {
    if !path.exists() {
        return Ok(GameList::default());
    }

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read gamelist: {}", path.display()))?;

    parse_gamelist_xml(&content)
}

/// Write gamelist.xml atomically (.tmp + rename).
pub fn write_gamelist(gamelist: &GameList, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    let xml = serialize_gamelist(gamelist);

    let tmp_path = path.with_extension("xml.tmp");
    std::fs::write(&tmp_path, &xml)
        .with_context(|| format!("Failed to write temp gamelist: {}", tmp_path.display()))?;

    std::fs::rename(&tmp_path, path)
        .with_context(|| format!("Failed to rename gamelist: {}", tmp_path.display()))?;

    Ok(())
}

/// Convert a ScrapedGame into a GameListEntry for merging.
pub fn scraped_to_entry(game: &ScrapedGame, rom_path: &str) -> GameListEntry {
    GameListEntry {
        path: rom_path.to_string(),
        name: Some(game.name.clone()),
        desc: game.description.clone(),
        rating: game.rating.map(|r| format!("{:.6}", r)),
        releasedate: game.release_date.clone(),
        developer: game.developer.clone(),
        publisher: game.publisher.clone(),
        genre: game.genre.clone(),
        players: game.players.clone(),
        // User fields — never set by scraper
        playcount: None,
        lastplayed: None,
        favorite: None,
        kidgame: None,
        hidden: None,
        altemulator: None,
    }
}

/// Parse gamelist.xml content. Uses a simple line-based parser to handle
/// ES-DE's XML format without requiring strict XML compliance.
fn parse_gamelist_xml(content: &str) -> Result<GameList> {
    let mut games = Vec::new();
    let mut current: Option<GameListEntry> = None;
    let mut current_tag: Option<String> = None;
    let mut text_buf = String::new();

    for line in content.as_bytes().lines() {
        let line = line?;
        let line = line.trim();

        if line == "<game>" {
            current = Some(GameListEntry::default());
            continue;
        }

        if line == "</game>" {
            if let Some(entry) = current.take() {
                if !entry.path.is_empty() {
                    games.push(entry);
                }
            }
            continue;
        }

        if let Some(ref mut entry) = current {
            // Parse <tag>value</tag> on a single line
            if let Some((tag, value)) = parse_xml_tag(line) {
                set_entry_field(entry, &tag, &value);
                continue;
            }

            // Handle multi-line content (rare but possible for <desc>)
            if let Some(tag) = extract_opening_tag(line) {
                current_tag = Some(tag.clone());
                // Check if there's content after the opening tag on this line
                let after_tag = &line[tag.len() + 2..]; // skip <tag>
                text_buf.clear();
                if !after_tag.is_empty() {
                    text_buf.push_str(after_tag);
                }
                continue;
            }

            if let Some(ref tag) = current_tag {
                let closing = format!("</{}>", tag);
                if line.ends_with(&closing) {
                    let before_close = &line[..line.len() - closing.len()];
                    if !text_buf.is_empty() {
                        text_buf.push('\n');
                    }
                    text_buf.push_str(before_close);
                    set_entry_field(entry, tag, &text_buf);
                    current_tag = None;
                    text_buf.clear();
                } else {
                    if !text_buf.is_empty() {
                        text_buf.push('\n');
                    }
                    text_buf.push_str(line);
                }
            }
        }
    }

    Ok(GameList { games })
}

/// Parse a single-line XML tag like `<name>Sonic</name>`.
fn parse_xml_tag(line: &str) -> Option<(String, String)> {
    if !line.starts_with('<') {
        return None;
    }
    let tag_end = line.find('>')?;
    let tag = &line[1..tag_end];
    if tag.starts_with('/') || tag.ends_with('/') {
        return None;
    }
    let closing = format!("</{}>", tag);
    if !line.ends_with(&closing) {
        return None;
    }
    let value_start = tag_end + 1;
    let value_end = line.len() - closing.len();
    let value = &line[value_start..value_end];
    Some((tag.to_string(), value.to_string()))
}

/// Extract the tag name from an opening tag like `<desc>`.
fn extract_opening_tag(line: &str) -> Option<String> {
    if !line.starts_with('<') || line.starts_with("</") {
        return None;
    }
    let tag_end = line.find('>')?;
    let tag = &line[1..tag_end];
    if tag.starts_with('/') || tag.ends_with('/') || tag.contains(' ') {
        return None;
    }
    Some(tag.to_string())
}

fn set_entry_field(entry: &mut GameListEntry, tag: &str, value: &str) {
    let value = xml_unescape(value);
    match tag {
        "path" => entry.path = value,
        "name" => entry.name = Some(value),
        "desc" => entry.desc = Some(value),
        "rating" => entry.rating = Some(value),
        "releasedate" => entry.releasedate = Some(value),
        "developer" => entry.developer = Some(value),
        "publisher" => entry.publisher = Some(value),
        "genre" => entry.genre = Some(value),
        "players" => entry.players = Some(value),
        "playcount" => entry.playcount = Some(value),
        "lastplayed" => entry.lastplayed = Some(value),
        "favorite" => entry.favorite = Some(value),
        "kidgame" => entry.kidgame = Some(value),
        "hidden" => entry.hidden = Some(value),
        "altemulator" => entry.altemulator = Some(value),
        _ => {} // Ignore unknown tags
    }
}

fn serialize_gamelist(gamelist: &GameList) -> String {
    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\"?>\n");
    xml.push_str("<gameList>\n");

    for game in &gamelist.games {
        xml.push_str("\t<game>\n");
        write_xml_field(&mut xml, "path", &game.path);
        write_xml_opt_field(&mut xml, "name", game.name.as_ref());
        write_xml_opt_field(&mut xml, "desc", game.desc.as_ref());
        write_xml_opt_field(&mut xml, "rating", game.rating.as_ref());
        write_xml_opt_field(&mut xml, "releasedate", game.releasedate.as_ref());
        write_xml_opt_field(&mut xml, "developer", game.developer.as_ref());
        write_xml_opt_field(&mut xml, "publisher", game.publisher.as_ref());
        write_xml_opt_field(&mut xml, "genre", game.genre.as_ref());
        write_xml_opt_field(&mut xml, "players", game.players.as_ref());
        write_xml_opt_field(&mut xml, "playcount", game.playcount.as_ref());
        write_xml_opt_field(&mut xml, "lastplayed", game.lastplayed.as_ref());
        write_xml_opt_field(&mut xml, "favorite", game.favorite.as_ref());
        write_xml_opt_field(&mut xml, "kidgame", game.kidgame.as_ref());
        write_xml_opt_field(&mut xml, "hidden", game.hidden.as_ref());
        write_xml_opt_field(&mut xml, "altemulator", game.altemulator.as_ref());
        xml.push_str("\t</game>\n");
    }

    xml.push_str("</gameList>\n");
    xml
}

fn write_xml_field(xml: &mut String, tag: &str, value: &str) {
    xml.push_str(&format!("\t\t<{}>{}</{}>\n", tag, xml_escape(value), tag));
}

fn write_xml_opt_field(xml: &mut String, tag: &str, value: Option<&String>) {
    if let Some(v) = value {
        write_xml_field(xml, tag, v);
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn xml_unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xml_escape() {
        assert_eq!(xml_escape("Tom & Jerry"), "Tom &amp; Jerry");
        assert_eq!(xml_escape("<test>"), "&lt;test&gt;");
        assert_eq!(xml_escape("He said \"hi\""), "He said &quot;hi&quot;");
        assert_eq!(xml_escape("it's"), "it&apos;s");
    }

    #[test]
    fn test_xml_unescape() {
        assert_eq!(xml_unescape("Tom &amp; Jerry"), "Tom & Jerry");
        assert_eq!(xml_unescape("&lt;test&gt;"), "<test>");
    }

    #[test]
    fn test_xml_escape_unescape_roundtrip() {
        let original = "Tom & Jerry's <\"great\"> adventure";
        assert_eq!(xml_unescape(&xml_escape(original)), original);
    }

    #[test]
    fn test_parse_xml_tag_single_line() {
        assert_eq!(
            parse_xml_tag("<name>Sonic</name>"),
            Some(("name".to_string(), "Sonic".to_string()))
        );
    }

    #[test]
    fn test_parse_xml_tag_empty_value() {
        assert_eq!(parse_xml_tag("<name></name>"), Some(("name".to_string(), String::new())));
    }

    #[test]
    fn test_parse_xml_tag_closing_tag() {
        assert_eq!(parse_xml_tag("</game>"), None);
    }

    #[test]
    fn test_parse_xml_tag_not_xml() {
        assert_eq!(parse_xml_tag("just text"), None);
    }

    #[test]
    fn test_extract_opening_tag() {
        assert_eq!(extract_opening_tag("<desc>"), Some("desc".to_string()));
        assert_eq!(extract_opening_tag("</desc>"), None);
        assert_eq!(extract_opening_tag("<br/>"), None);
        assert_eq!(extract_opening_tag("text"), None);
    }

    #[test]
    fn test_parse_gamelist_xml_simple() {
        let xml = r#"<?xml version="1.0"?>
<gameList>
	<game>
		<path>./Sonic.zip</path>
		<name>Sonic The Hedgehog</name>
		<rating>0.8</rating>
	</game>
</gameList>"#;

        let gl = parse_gamelist_xml(xml).unwrap();
        assert_eq!(gl.games.len(), 1);
        assert_eq!(gl.games[0].path, "./Sonic.zip");
        assert_eq!(gl.games[0].name.as_deref(), Some("Sonic The Hedgehog"));
        assert_eq!(gl.games[0].rating.as_deref(), Some("0.8"));
    }

    #[test]
    fn test_parse_gamelist_xml_multiple_games() {
        let xml = r#"<?xml version="1.0"?>
<gameList>
	<game>
		<path>./Sonic.zip</path>
		<name>Sonic</name>
	</game>
	<game>
		<path>./Mario.zip</path>
		<name>Mario</name>
	</game>
</gameList>"#;

        let gl = parse_gamelist_xml(xml).unwrap();
        assert_eq!(gl.games.len(), 2);
        assert_eq!(gl.games[0].name.as_deref(), Some("Sonic"));
        assert_eq!(gl.games[1].name.as_deref(), Some("Mario"));
    }

    #[test]
    fn test_parse_gamelist_xml_preserves_user_fields() {
        let xml = r#"<?xml version="1.0"?>
<gameList>
	<game>
		<path>./Sonic.zip</path>
		<name>Sonic</name>
		<playcount>42</playcount>
		<lastplayed>20240101T000000</lastplayed>
		<favorite>true</favorite>
	</game>
</gameList>"#;

        let gl = parse_gamelist_xml(xml).unwrap();
        assert_eq!(gl.games[0].playcount.as_deref(), Some("42"));
        assert_eq!(gl.games[0].lastplayed.as_deref(), Some("20240101T000000"));
        assert_eq!(gl.games[0].favorite.as_deref(), Some("true"));
    }

    #[test]
    fn test_parse_gamelist_xml_multiline_desc() {
        let xml = r#"<?xml version="1.0"?>
<gameList>
	<game>
		<path>./Sonic.zip</path>
		<desc>Line one
Line two
Line three</desc>
	</game>
</gameList>"#;

        let gl = parse_gamelist_xml(xml).unwrap();
        assert!(gl.games[0].desc.as_ref().unwrap().contains("Line one"));
        assert!(gl.games[0].desc.as_ref().unwrap().contains("Line two"));
    }

    #[test]
    fn test_serialize_gamelist_roundtrip() {
        let gl = GameList {
            games: vec![GameListEntry {
                path: "./Sonic.zip".to_string(),
                name: Some("Sonic".to_string()),
                desc: Some("A hedgehog game".to_string()),
                rating: Some("0.800000".to_string()),
                releasedate: Some("19910623T000000".to_string()),
                developer: Some("Sonic Team".to_string()),
                publisher: Some("Sega".to_string()),
                genre: Some("Platform".to_string()),
                players: Some("1".to_string()),
                playcount: Some("5".to_string()),
                favorite: Some("true".to_string()),
                ..Default::default()
            }],
        };

        let xml = serialize_gamelist(&gl);
        let parsed = parse_gamelist_xml(&xml).unwrap();

        assert_eq!(parsed.games.len(), 1);
        let g = &parsed.games[0];
        assert_eq!(g.path, "./Sonic.zip");
        assert_eq!(g.name.as_deref(), Some("Sonic"));
        assert_eq!(g.desc.as_deref(), Some("A hedgehog game"));
        assert_eq!(g.playcount.as_deref(), Some("5"));
        assert_eq!(g.favorite.as_deref(), Some("true"));
    }

    #[test]
    fn test_serialize_gamelist_escapes_special_chars() {
        let gl = GameList {
            games: vec![GameListEntry {
                path: "./Tom & Jerry.zip".to_string(),
                name: Some("Tom & Jerry's <Game>".to_string()),
                ..Default::default()
            }],
        };

        let xml = serialize_gamelist(&gl);
        assert!(xml.contains("&amp;"));
        assert!(xml.contains("&apos;"));

        let parsed = parse_gamelist_xml(&xml).unwrap();
        assert_eq!(parsed.games[0].path, "./Tom & Jerry.zip");
        assert_eq!(parsed.games[0].name.as_deref(), Some("Tom & Jerry's <Game>"));
    }

    #[test]
    fn test_scraped_to_entry() {
        let game = ScrapedGame {
            game_id: 1,
            rom_id: Some(2),
            name: "Sonic".to_string(),
            description: Some("Fast hedgehog".to_string()),
            rating: Some(0.8),
            release_date: Some("19910623T000000".to_string()),
            developer: Some("Sonic Team".to_string()),
            publisher: Some("Sega".to_string()),
            genre: Some("Platform".to_string()),
            players: Some("1".to_string()),
            system_id: 1,
            available_media: vec![],
        };

        let entry = scraped_to_entry(&game, "./sonic.zip");
        assert_eq!(entry.path, "./sonic.zip");
        assert_eq!(entry.name.as_deref(), Some("Sonic"));
        assert_eq!(entry.rating.as_deref(), Some("0.800000"));
        assert!(entry.playcount.is_none());
        assert!(entry.favorite.is_none());
    }

    #[test]
    fn test_write_and_read_gamelist_roundtrip() {
        let dir = std::env::temp_dir().join("scrauper_test_gamelist");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("gamelist.xml");

        let gl = GameList {
            games: vec![GameListEntry {
                path: "./test.zip".to_string(),
                name: Some("Test Game".to_string()),
                ..Default::default()
            }],
        };

        write_gamelist(&gl, &path).unwrap();
        let read_back = read_gamelist(&path).unwrap();

        assert_eq!(read_back.games.len(), 1);
        assert_eq!(read_back.games[0].name.as_deref(), Some("Test Game"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_read_gamelist_missing_file() {
        let path = Path::new("/tmp/nonexistent_gamelist_scrauper.xml");
        let gl = read_gamelist(path).unwrap();
        assert!(gl.games.is_empty());
    }

    #[test]
    fn test_serialize_empty_gamelist() {
        let gl = GameList::default();
        let xml = serialize_gamelist(&gl);
        assert!(xml.contains("<gameList>"));
        assert!(xml.contains("</gameList>"));
        assert!(!xml.contains("<game>"));
    }

    #[test]
    fn test_parse_gamelist_xml_hidden_field() {
        let xml = r#"<?xml version="1.0"?>
<gameList>
	<game>
		<path>./Disc1.chd</path>
		<name>Disc 1</name>
		<hidden>true</hidden>
	</game>
</gameList>"#;

        let gl = parse_gamelist_xml(xml).unwrap();
        assert_eq!(gl.games[0].hidden.as_deref(), Some("true"));
    }

    #[test]
    fn test_parse_gamelist_xml_altemulator() {
        let xml = r#"<?xml version="1.0"?>
<gameList>
	<game>
		<path>./game.zip</path>
		<altemulator>retroarch</altemulator>
	</game>
</gameList>"#;

        let gl = parse_gamelist_xml(xml).unwrap();
        assert_eq!(gl.games[0].altemulator.as_deref(), Some("retroarch"));
    }

    #[test]
    fn test_parse_gamelist_xml_unknown_tags_ignored() {
        let xml = r#"<?xml version="1.0"?>
<gameList>
	<game>
		<path>./sonic.zip</path>
		<name>Sonic</name>
		<unknowntag>value</unknowntag>
	</game>
</gameList>"#;

        let gl = parse_gamelist_xml(xml).unwrap();
        assert_eq!(gl.games.len(), 1);
        assert_eq!(gl.games[0].name.as_deref(), Some("Sonic"));
    }

    #[test]
    fn test_parse_gamelist_xml_empty() {
        let xml = r#"<?xml version="1.0"?>
<gameList>
</gameList>"#;

        let gl = parse_gamelist_xml(xml).unwrap();
        assert!(gl.games.is_empty());
    }

    #[test]
    fn test_parse_gamelist_xml_game_without_path() {
        let xml = r#"<?xml version="1.0"?>
<gameList>
	<game>
		<name>No Path</name>
	</game>
</gameList>"#;

        let gl = parse_gamelist_xml(xml).unwrap();
        // Game without path should be skipped
        assert!(gl.games.is_empty());
    }

    #[test]
    fn test_serialize_gamelist_with_hidden() {
        let gl = GameList {
            games: vec![GameListEntry {
                path: "./disc.chd".to_string(),
                name: Some("Disc 1".to_string()),
                hidden: Some("true".to_string()),
                ..Default::default()
            }],
        };
        let xml = serialize_gamelist(&gl);
        assert!(xml.contains("<hidden>true</hidden>"));

        let parsed = parse_gamelist_xml(&xml).unwrap();
        assert_eq!(parsed.games[0].hidden.as_deref(), Some("true"));
    }

    #[test]
    fn test_scraped_to_entry_minimal() {
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
            system_id: 1,
            available_media: vec![],
        };

        let entry = scraped_to_entry(&game, "./minimal.zip");
        assert_eq!(entry.path, "./minimal.zip");
        assert_eq!(entry.name.as_deref(), Some("Minimal"));
        assert!(entry.desc.is_none());
        assert!(entry.rating.is_none());
        assert!(entry.releasedate.is_none());
    }

    #[test]
    fn test_xml_escape_no_special_chars() {
        assert_eq!(xml_escape("simple text"), "simple text");
    }

    #[test]
    fn test_xml_unescape_no_entities() {
        assert_eq!(xml_unescape("simple text"), "simple text");
    }

    #[test]
    fn test_xml_escape_all_entities() {
        let input = "A & B < C > D \" E ' F";
        let escaped = xml_escape(input);
        assert_eq!(escaped, "A &amp; B &lt; C &gt; D &quot; E &apos; F");
        assert_eq!(xml_unescape(&escaped), input);
    }

    #[test]
    fn test_parse_xml_tag_self_closing() {
        assert_eq!(parse_xml_tag("<br/>"), None);
    }

    #[test]
    fn test_extract_opening_tag_with_attributes() {
        // Tags with spaces (attributes) should return None
        assert_eq!(extract_opening_tag("<game source=\"scraper\">"), None);
    }

    #[test]
    fn test_write_gamelist_creates_parent_dirs() {
        let dir = std::env::temp_dir().join("scrauper_test_nested_gl");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("sub").join("gamelist.xml");

        let gl = GameList {
            games: vec![GameListEntry {
                path: "./test.zip".to_string(),
                name: Some("Test".to_string()),
                ..Default::default()
            }],
        };

        write_gamelist(&gl, &path).unwrap();
        assert!(path.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}

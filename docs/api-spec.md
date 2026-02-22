# ScreenScraper API v2 - Specification

Base URL: `https://api.screenscraper.fr/api2/`

## Authentication

Every request requires developer credentials. User credentials are optional for most list endpoints but required for game lookups.

| Parameter      | Required | Description                          |
|----------------|----------|--------------------------------------|
| `devid`        | Yes      | Developer identifier                 |
| `devpassword`  | Yes      | Developer password                   |
| `softname`     | Yes      | Name of the calling software         |
| `ssid`         | No*      | ScreenScraper user identifier        |
| `sspassword`   | No*      | ScreenScraper user password          |
| `output`       | No       | Response format: `xml` (default), `json` |

*Required for game info/search endpoints.

## Rate Limiting & Threads

- **Threads**: Number of simultaneous API requests allowed per user. Default is low; higher-tier contributors get more.
- **Quotas**: Per-minute and per-day request limits based on user contribution level.
- User info response includes `maxthreads`, `maxrequestspermin`, `maxrequestsperday` — the scraper **must** respect these.
- Exceeding limits returns HTTP 429 or 430.

## HTTP Error Codes

| Code | Meaning                                                  |
|------|----------------------------------------------------------|
| 400  | Malformed URL, missing required fields, bad CRC/MD5/SHA1 |
| 401  | API closed for non-members (server CPU > 60%)            |
| 403  | Invalid developer credentials                            |
| 404  | Game/ROM not found                                       |
| 423  | API completely closed (server issues)                    |
| 426  | Software blacklisted (outdated/non-compliant)            |
| 429  | Thread limit or per-minute limit exceeded                |
| 430  | Daily scrape quota exceeded                              |
| 431  | Too many unrecognized ROMs scraped today                 |

---

## Core Endpoints for Game Scraping

### 1. `jeuRecherche.php` — Search Games by Name

Search for a game by name. Returns up to 30 results ranked by probability.

**Parameters:**

| Parameter    | Required | Description                              |
|--------------|----------|------------------------------------------|
| `recherche`  | Yes      | Game name to search for                  |
| `systemeid`  | No       | System ID to narrow results              |

**Response:** Array of game objects (same structure as `jeuInfos` but without ROM details).

**Example:**
```
/api2/jeuRecherche.php?devid=xxx&devpassword=yyy&softname=zzz&output=json&ssid=USER&sspassword=PASS&systemeid=1&recherche=sonic
```

---

### 2. `jeuInfos.php` — Game Details (Primary Endpoint)

The main endpoint for scraping. Identifies a game by ROM hash, filename, or game ID.

**Parameters:**

| Parameter    | Required | Description                                    |
|--------------|----------|------------------------------------------------|
| `crc`        | Yes*     | CRC32 hash of the ROM file                     |
| `md5`        | Yes*     | MD5 hash of the ROM file                       |
| `sha1`       | Yes*     | SHA1 hash of the ROM file                      |
| `systemeid`  | Yes      | Numeric system ID                              |
| `romtype`    | Yes      | ROM type: `rom`, `iso`, or `dossier` (folder)  |
| `romnom`     | Yes      | ROM filename (with extension)                  |
| `romtaille`  | Yes*     | ROM file size in bytes                          |
| `serialnum`  | No       | Force search by serial number                  |
| `gameid`     | No       | Force search by game ID (skips ROM matching)   |

*Send at least one hash (ideally all 3) plus file size for best matching.

**Response — Game Object:**

```
jeu {
  id              — Numeric game ID
  romid           — Numeric ROM ID
  notgame         — true/false (demo/app vs actual game)

  noms {
    nom_ss        — Internal ScreenScraper name
    nom_{region}  — Region-specific title (wor, us, eu, jp, etc.)
  }

  systeme {
    id, nom, parentid
  }

  editeur         — Publisher name
  developpeur     — Developer name
  joueurs         — Number of players
  note            — Rating out of 20

  synopsis {
    synopsis_{lang} — Description per language (en, fr, de, es, pt, etc.)
  }

  dates {
    date_{region}  — Release date per region (yyyy-mm-dd)
  }

  genres {
    genre_id, nomcourt, principale, parentid
    genre_{lang}   — Genre name per language
  }

  familles        — Game family/franchise info

  medias {
    — Box art —
    media_boitiers_2d    — 2D box front per region
    media_boitiers_3d    — 3D box render per region
    media_boitiers_texture — Box texture per region

    — Screenshots —
    media_screenshot     — In-game screenshot

    — Logos —
    media_wheel_{region} — Color logo/wheel per region

    — Other —
    media_fanart         — Fan art background
    media_video          — Video capture
    media_marquee        — Marquee image
    media_manuel_{region} — PDF manual per region
    media_support_2d_{region} — Cart/disc front image
    media_flyer_{region} — Promotional flyer

    — Bezels —
    media_bezel4-3_{region}
    media_bezel16-9_{region}
  }

  roms [] — List of known ROMs for this game
    rom {
      id, romfilename, romsize
      romcrc, rommd5, romsha1
      romregions, romlangues, romserial
      beta, demo, hack, alt, best
      retroachievement — RetroAchievement compatible (0/1)
    }
}
```

**Example:**
```
/api2/jeuInfos.php?devid=xxx&devpassword=yyy&softname=zzz&output=json
  &ssid=USER&sspassword=PASS
  &crc=50ABC90A&systemeid=1&romtype=rom
  &romnom=Sonic%20The%20Hedgehog%202%20(World).zip&romtaille=749652
```

---

### 3. `mediaJeu.php` — Download Game Media

Download a specific media image for a game.

**Parameters:**

| Parameter      | Required | Description                                  |
|----------------|----------|----------------------------------------------|
| `systemeid`    | Yes      | System ID                                    |
| `jeuid`        | Yes      | Game ID (from `jeuInfos`)                    |
| `media`        | Yes      | Media identifier (e.g. `box-2D(us)`, `wheel-hd(wor)`) |
| `crc`/`md5`/`sha1` | No  | Hash of local file (returns `CRCOK` etc. if unchanged) |
| `maxwidth`     | No       | Max pixel width of returned image            |
| `maxheight`    | No       | Max pixel height of returned image           |
| `outputformat` | No       | `png` or `jpg`                               |

**Returns:** Raw image binary, or `CRCOK`/`MD5OK`/`SHA1OK` if unchanged, or `NOMEDIA`.

---

### 4. `mediaVideoJeu.php` — Download Game Video

Same parameters as `mediaJeu.php` but returns MP4 video.

---

### 5. `systemesListe.php` — List All Systems

Returns all supported systems with IDs, names, extensions, and media.

**Key fields per system:**
- `id` — System ID to use in other requests
- `extensions` — ROM file extensions
- `type` — Arcade, Console, Portable, Computer, etc.
- `compagnie` — Manufacturer

---

## Media Types Relevant for Box Art + Descriptions

These are the media identifiers to use with `mediaJeu.php`:

| Media ID          | Description              | Format |
|-------------------|--------------------------|--------|
| `box-2D`          | Box front                | PNG    |
| `box-2D-side`     | Box spine                | PNG    |
| `box-2D-back`     | Box back                 | PNG    |
| `box-texture`     | Box texture (unwrapped)  | PNG    |
| `ss`              | In-game screenshot       | JPG    |
| `sstitle`         | Title screen screenshot  | JPG    |
| `wheel`           | Color logo               | PNG    |
| `wheel-hd`        | HD color logo            | PNG    |
| `fanart`          | Fan art                  | JPG    |
| `marquee`         | Marquee                  | PNG    |
| `support-texture` | Cart/disc texture        | PNG    |
| `video`           | Gameplay video           | MP4    |
| `manuel`          | PDF manual               | PDF    |

Region suffixes: append `(wor)`, `(us)`, `(eu)`, `(jp)`, `(fr)`, etc.
Example: `box-2D(us)`, `wheel-hd(wor)`, `ss(wor)`

---

## Infrastructure / Monitoring

### `ssinfraInfos.php` — Server Status
Returns CPU load, active threads, daily access count. Use this to check if the API is overloaded before starting a scrape session.

### `ssuserInfos.php` — User Info & Quotas
Returns the user's thread limit, daily quota, and current usage. **Must be checked before scraping to respect limits.**

Key response fields:
- `maxthreads` — Max concurrent requests allowed
- `maxrequestspermin` — Max requests/minute
- `maxrequestsperday` — Max requests/day
- `requeststoday` — Requests used today
- `maxdownloadspeed` — Max download speed (KB/s)

---

## Design Notes for Rust Implementation

### Multi-threading Strategy
1. **On startup**: Call `ssuserInfos.php` to get `maxthreads` and quota info
2. **Thread pool**: Size the thread pool to `maxthreads` (not more)
3. **Rate limiter**: Enforce `maxrequestspermin` with a token-bucket or sliding-window limiter
4. **Daily counter**: Track total requests and stop at `maxrequestsperday`
5. **Backoff on 429/430**: Exponential backoff when rate-limited

### Scrape Workflow per Game
1. Hash the ROM file (CRC32 + MD5 + SHA1)
2. Call `jeuInfos.php` with hashes, system ID, filename, and file size
3. Extract game info: name, description, publisher, developer, genre, release date, rating
4. Extract media URLs from the response
5. Download desired media via `mediaJeu.php` (box art, screenshots, etc.)
6. Use CRC/MD5/SHA1 of local files to skip re-downloads (`CRCOK` optimization)

### Key Data to Extract (Game Boxes + Descriptions)
- **Box art**: `box-2D`, `box-texture` (for 3D rendering), `box-2D-back`
- **Description**: `synopsis_{lang}` from `jeuInfos` response
- **Title**: `noms.nom_{region}` — prefer `wor` > `us` > `eu` > `jp`
- **Metadata**: publisher, developer, genre, players, rating, release date

### CRC Optimization
For media downloads, send the hash of the already-downloaded local file. If the server returns `CRCOK`/`MD5OK`/`SHA1OK`, skip the download. This dramatically reduces bandwidth on re-scrapes.

# 🕹️ scrauper

![CI](https://github.com/nikuscs/scrauper/actions/workflows/ci.yml/badge.svg)
![Release](https://img.shields.io/github/v/release/nikuscs/scrauper)
![License](https://img.shields.io/badge/license-MIT-blue.svg)

**Fast multi-threaded [ScreenScraper.fr](https://www.screenscraper.fr/) scraper for [ES-DE](https://es-de.org/) with multi-account rotation, proxy support, and miximage generation.**

> **Disclaimer:** This project is for **educational purposes and AI automation research only**.
> The authors are not responsible for any misuse or for any damages resulting from the use of this tool.
> Users are solely responsible for ensuring compliance with applicable laws and the terms of service
> of any websites accessed. This software is provided "as-is" without warranty of any kind.
>
> If you are a rights holder and wish to have this project removed, please [contact me](https://github.com/nikuscs).

> **Note:** This project was partially developed with AI assistance and may contain bugs or unexpected behavior. Use at your own risk.

## Why?

Scraping metadata for retro game collections is painful. ScreenScraper.fr is the best source, but the official tools are slow, single-threaded, and hit daily API quotas fast. If you have thousands of ROMs across multiple systems, you're looking at days of scraping.

- **Multi-account rotation** — bypass daily quotas by rotating across multiple ScreenScraper accounts
- **Pixel-accurate miximage generation** — pure Rust compositing, no ImageMagick dependency
- **Multi-threaded with per-account rate limiting** — fast scraping that respects API limits
- **Proxy rotation** — distribute requests across multiple proxies
- **Intelligent response caching** — skip already-scraped games, resume interrupted runs
- **Cross-platform** — Linux, macOS, Windows, Android/ARM
- **Hash-based ROM identification** — CRC32 + MD5 + SHA1 for accurate matching
- **Interactive mode** — manual selection for ambiguous matches

## Install

```bash
# From source (requires Rust)
cargo install --git https://github.com/nikuscs/scrauper

# Or clone and build
git clone https://github.com/nikuscs/scrauper
cd scrauper
cargo build --release
```

Pre-built binaries available in [Releases](https://github.com/nikuscs/scrauper/releases).

## Usage

```bash
# Generate a config file
scrauper init

# Edit scrauper.toml with your ScreenScraper credentials and paths
$EDITOR scrauper.toml

# Start scraping
scrauper scrape
```

### Commands

```
scrauper <COMMAND>

Commands:
  scrape              Scrape game metadata and media from ScreenScraper
  info                Show user account info and quotas
  systems             List all supported systems
  cleanup             Remove orphaned media files
  generate-miximages  Generate miximages from existing media
  cache-stats         Show cache statistics
  init                Create a new config file from the example template
```

### Scrape options

```bash
scrauper scrape                     # scrape all detected systems
scrauper scrape --system snes       # scrape a specific system
scrauper scrape --force             # re-scrape even for complete games
scrauper scrape --interactive       # prompt for ambiguous matches
```

## Configuration

Run `scrauper init` to generate a `scrauper.toml` from the example template. Key sections:

### Credentials and accounts

```toml
[credentials]
# Single account
accounts = "user1:pass1"
# Multi-account rotation (recommended for higher quotas)
accounts = "user1:pass1;user2:pass2;user3:pass3"
```

### Paths

```toml
[paths]
rom_directory = "~/ROMs"
media_directory = "~/.emulationstation/downloaded_media"
gamelist_directory = "~/.emulationstation/gamelists"
```

### Proxy rotation

```toml
[network]
proxies = "user:pass@host:port;host2:port2"
```

### Miximage generation

```toml
[miximage]
enabled = true
width = 1280
height = 960
format = "png"
```

See [`scrauper.toml.example`](scrauper.toml.example) for the full configuration reference.

## How It Works

1. **ROM discovery** — scans ROM directories, identifies systems by folder structure
2. **Hash identification** — computes CRC32, MD5, and SHA1 for accurate game matching
3. **API lookup** — queries ScreenScraper.fr with hash or filename fallback
4. **Metadata + media download** — fetches game info, screenshots, box art, videos
5. **gamelist.xml generation** — writes ES-DE compatible gamelist files
6. **Miximage compositing** — generates composite images (screenshot + box + marquee)

## License

MIT

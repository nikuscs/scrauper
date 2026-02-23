# Scrauper

Multi-threaded [ScreenScraper.fr](https://www.screenscraper.fr/) scraper for [ES-DE](https://es-de.org/) with multi-account rotation, proxy support, and miximage generation.

[![CI](https://github.com/nikuscs/scrauper/actions/workflows/ci.yml/badge.svg)](https://github.com/nikuscs/scrauper/actions/workflows/ci.yml)
[![Release](https://github.com/nikuscs/scrauper/actions/workflows/release.yml/badge.svg)](https://github.com/nikuscs/scrauper/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

## Why Scrauper?

- **Multi-account rotation** — bypass daily quotas by rotating across multiple ScreenScraper accounts
- **Pixel-accurate miximage generation** — pure Rust compositing, no ImageMagick dependency
- **Multi-threaded with per-account rate limiting** — fast scraping that respects API limits
- **Proxy rotation** — distribute requests across multiple proxies
- **Intelligent response caching** — skip already-scraped games, resume interrupted runs
- **Cross-platform** — Linux, macOS, Windows, Android/ARM
- **Hash-based ROM identification** — CRC32 + MD5 + SHA1 for accurate matching
- **Interactive mode** — manual selection for ambiguous matches

## Installation

### Pre-built binaries

Download the latest release for your platform from the [Releases](https://github.com/nikuscs/scrauper/releases) page.

### From source

```bash
cargo install --git https://github.com/nikuscs/scrauper
```

## Quick Start

```bash
# Generate a config file
scrauper init

# Edit scrauper.toml with your ScreenScraper credentials and paths
$EDITOR scrauper.toml

# Start scraping
scrauper scrape
```

## Usage

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

[MIT](LICENSE)

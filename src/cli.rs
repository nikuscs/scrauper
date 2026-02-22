use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "scrauper", version, about = "Multi-threaded ScreenScraper API scraper for ES-DE")]
pub struct Cli {
    /// Path to config file
    #[arg(long, short, default_value = "scrauper.toml", env = "SCRAUPER_CONFIG")]
    pub config: PathBuf,

    /// Enable verbose logging
    #[arg(long, short)]
    pub verbose: bool,

    /// Disable colored output
    #[arg(long)]
    pub no_color: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Scrape game metadata and media from ScreenScraper
    Scrape {
        /// Only scrape a specific system (e.g., "snes", "megadrive")
        #[arg(long, short)]
        system: Option<String>,

        /// Force re-scrape even for complete games
        #[arg(long, short)]
        force: bool,

        /// Prompt for ambiguous matches
        #[arg(long, short)]
        interactive: bool,
    },

    /// Show user account info and quotas
    Info,

    /// List all supported systems
    Systems,

    /// Remove orphaned media files
    Cleanup {
        /// Show what would be deleted without deleting
        #[arg(long)]
        dry_run: bool,

        /// Skip confirmation prompt
        #[arg(long, short)]
        yes: bool,
    },

    /// Generate miximages from existing media
    GenerateMiximages {
        /// Only generate for a specific system
        #[arg(long, short)]
        system: Option<String>,
    },

    /// Show cache statistics
    CacheStats,

    /// Create a new scrauper.toml config file from the example template
    Init,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_scrape_defaults() {
        let cli = Cli::parse_from(["scrauper", "scrape"]);
        assert!(matches!(cli.command, Command::Scrape { .. }));
        if let Command::Scrape { system, force, interactive } = cli.command {
            assert!(system.is_none());
            assert!(!force);
            assert!(!interactive);
        }
    }

    #[test]
    fn test_parse_scrape_with_system() {
        let cli = Cli::parse_from(["scrauper", "scrape", "--system", "snes"]);
        if let Command::Scrape { system, .. } = cli.command {
            assert_eq!(system.as_deref(), Some("snes"));
        } else {
            panic!("expected Scrape command");
        }
    }

    #[test]
    fn test_parse_scrape_force() {
        let cli = Cli::parse_from(["scrauper", "scrape", "--force"]);
        if let Command::Scrape { force, .. } = cli.command {
            assert!(force);
        }
    }

    #[test]
    fn test_parse_scrape_interactive() {
        let cli = Cli::parse_from(["scrauper", "scrape", "--interactive"]);
        if let Command::Scrape { interactive, .. } = cli.command {
            assert!(interactive);
        }
    }

    #[test]
    fn test_parse_info() {
        let cli = Cli::parse_from(["scrauper", "info"]);
        assert!(matches!(cli.command, Command::Info));
    }

    #[test]
    fn test_parse_systems() {
        let cli = Cli::parse_from(["scrauper", "systems"]);
        assert!(matches!(cli.command, Command::Systems));
    }

    #[test]
    fn test_parse_cleanup_dry_run() {
        let cli = Cli::parse_from(["scrauper", "cleanup", "--dry-run"]);
        if let Command::Cleanup { dry_run, yes } = cli.command {
            assert!(dry_run);
            assert!(!yes);
        }
    }

    #[test]
    fn test_parse_init() {
        let cli = Cli::parse_from(["scrauper", "init"]);
        assert!(matches!(cli.command, Command::Init));
    }

    #[test]
    fn test_parse_cache_stats() {
        let cli = Cli::parse_from(["scrauper", "cache-stats"]);
        assert!(matches!(cli.command, Command::CacheStats));
    }

    #[test]
    fn test_parse_generate_miximages() {
        let cli = Cli::parse_from(["scrauper", "generate-miximages", "--system", "megadrive"]);
        if let Command::GenerateMiximages { system } = cli.command {
            assert_eq!(system.as_deref(), Some("megadrive"));
        }
    }

    #[test]
    fn test_verbose_flag() {
        let cli = Cli::parse_from(["scrauper", "--verbose", "info"]);
        assert!(cli.verbose);
    }

    #[test]
    fn test_custom_config_path() {
        let cli = Cli::parse_from(["scrauper", "--config", "/path/to/config.toml", "info"]);
        assert_eq!(cli.config, PathBuf::from("/path/to/config.toml"));
    }

    #[test]
    fn test_default_config_path() {
        let cli = Cli::parse_from(["scrauper", "info"]);
        assert_eq!(cli.config, PathBuf::from("scrauper.toml"));
    }
}

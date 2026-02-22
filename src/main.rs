mod api;
mod cache;
mod cli;
mod config;
mod media;
mod models;
mod output;
mod progress;
mod scraper;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::api::client::ScreenScraperClient;
use crate::api::endpoints;
use crate::cache::store::CacheStore;
use crate::cli::{Cli, Command};
use crate::config::Config;
use crate::output::cleanup::{self, CleanupOptions};
use crate::scraper::pipeline::{self, ScrapeOptions};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Init tracing
    let filter = if cli.verbose {
        EnvFilter::new("scrauper=debug")
    } else {
        EnvFilter::new("scrauper=info")
    };
    tracing_subscriber::fmt().with_env_filter(filter).with_target(false).init();

    match &cli.command {
        Command::Scrape { system, force, interactive } => {
            let config = load_config(&cli)?;
            let options = ScrapeOptions {
                system_filter: system.clone(),
                force: *force,
                interactive: *interactive,
            };
            pipeline::run_scrape(&config, &options).await?;
        }
        Command::Info => {
            let config = load_config(&cli)?;
            cmd_info(&config).await?;
        }
        Command::Systems => {
            let config = load_config(&cli)?;
            cmd_systems(&config).await?;
        }
        Command::Cleanup { dry_run, yes } => {
            let config = load_config(&cli)?;
            let options = CleanupOptions { dry_run: *dry_run, auto_confirm: *yes };
            cleanup::run_cleanup(&config, &options)?;
        }
        Command::GenerateMiximages { system } => {
            let config = load_config(&cli)?;
            pipeline::run_generate_miximages(&config, system.as_deref()).await?;
        }
        Command::CacheStats => {
            let config = load_config(&cli)?;
            cmd_cache_stats(&config)?;
        }
        Command::Init => {
            cmd_init()?;
        }
    }

    Ok(())
}

fn load_config(cli: &Cli) -> Result<Config> {
    if !cli.config.exists() {
        anyhow::bail!(
            "Config file not found: {}\n\
             Run `scrauper init` to create one from the example template.",
            cli.config.display()
        );
    }
    Config::load(&cli.config)
}

/// Create a new scrauper.toml from the embedded example template.
#[allow(clippy::print_stderr)]
fn cmd_init() -> Result<()> {
    let dest = std::path::Path::new("scrauper.toml");

    if dest.exists() {
        anyhow::bail!("scrauper.toml already exists. Remove it first if you want to reinitialize.");
    }

    let template = include_str!("../scrauper.toml.example");
    std::fs::write(dest, template)?;

    eprintln!("Created scrauper.toml from template.");
    eprintln!();
    eprintln!("Next steps:");
    eprintln!("  1. Edit scrauper.toml and fill in your ScreenScraper credentials");
    eprintln!("  2. Set your ROM and media directory paths");
    eprintln!("  3. Run `scrauper scrape` to start scraping");

    Ok(())
}

#[allow(clippy::print_stderr)]
async fn cmd_info(config: &Config) -> Result<()> {
    let client = ScreenScraperClient::new(config)?;

    tracing::info!("Fetching server status...");

    match client.get_infra_info().await {
        Ok(infra) => {
            eprintln!("=== Server Status ===");
            eprintln!("  API status:         {}", if infra.api_open { "Open" } else { "Closed" });
            for cpu in &infra.cpu_loads {
                eprintln!("  {cpu}");
            }
            eprintln!("  Active threads:     {}", infra.active_threads);
            eprintln!("  Active scrapers:    {}", infra.active_scrapers);
        }
        Err(e) => {
            eprintln!("=== Server Status ===");
            eprintln!("  Could not fetch: {e}");
        }
    }

    if config.has_user_credentials() {
        tracing::info!("Fetching account info...");
        match client.get_user_info().await {
            Ok(user) => {
                eprintln!();
                eprintln!("=== User Info ===");
                eprintln!("  Username:           {}", user.username);
                eprintln!("  User ID:            {}", user.user_id);
                eprintln!("  Level:              {}", user.level);
                eprintln!("  Contribution:       {}", user.contribution);
                eprintln!("  Favorite region:    {}", user.favorite_region);
                eprintln!();
                eprintln!("=== Quotas ===");
                eprintln!("  Max threads:        {}", user.max_threads);
                eprintln!("  Max req/min:        {}", user.max_requests_per_min);
                eprintln!(
                    "  Requests today:     {} / {}",
                    user.requests_today, user.max_requests_per_day
                );
                eprintln!("  Failed today:       {}", user.requests_ko_today);
                eprintln!("  Max download speed: {} KB/s", user.max_download_speed);
            }
            Err(e) => {
                eprintln!();
                eprintln!("  Could not fetch user info: {e}");
            }
        }
    } else {
        eprintln!();
        eprintln!("  (Set both ssid and sspassword in config for user info and better quotas)");
    }

    Ok(())
}

#[allow(clippy::print_stderr)]
async fn cmd_systems(config: &Config) -> Result<()> {
    // Try cache first
    let cache_dir = config.cache_directory();
    if let Some(systems) = endpoints::load_systems_cache(&cache_dir)? {
        tracing::info!("Loaded {} systems from cache", systems.len());
        print_systems(&systems);
        return Ok(());
    }

    let client = ScreenScraperClient::new(config)?;
    tracing::info!("Fetching systems list...");

    let systems = client.get_systems_list().await?;
    endpoints::save_systems_cache(&cache_dir, &systems)?;
    tracing::info!("Fetched {} systems (cached for next time)", systems.len());
    print_systems(&systems);

    Ok(())
}

#[allow(clippy::print_stderr)]
fn cmd_cache_stats(config: &Config) -> Result<()> {
    let cache = CacheStore::new(&config.cache_directory());
    let stats = cache.all_stats()?;

    eprintln!("Cache directory: {}", cache.base_dir().display());
    eprintln!();

    if stats.is_empty() {
        eprintln!("Cache is empty.");
        return Ok(());
    }

    eprintln!("{:<25} {:>8} {:>12}", "System", "Games", "Size");
    eprintln!("{}", "-".repeat(47));

    let mut total_entries = 0;
    let mut total_bytes = 0u64;

    for s in &stats {
        total_entries += s.entry_count;
        total_bytes += s.total_bytes;
        eprintln!("{:<25} {:>8} {:>12}", s.system, s.entry_count, format_bytes(s.total_bytes));
    }

    eprintln!("{}", "-".repeat(47));
    eprintln!("{:<25} {:>8} {:>12}", "Total", total_entries, format_bytes(total_bytes));

    Ok(())
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

#[allow(clippy::print_stderr)]
fn print_systems(systems: &[models::system::System]) {
    eprintln!("{:<6} {:<35} {:<12} Extensions", "ID", "Name", "Type");
    eprintln!("{}", "-".repeat(90));

    let mut sorted: Vec<_> = systems.iter().collect();
    sorted.sort_by_key(|s| s.id);

    for sys in sorted {
        let exts = if sys.extensions.len() > 5 {
            let preview: Vec<_> = sys.extensions.iter().take(5).cloned().collect();
            format!("{} (+{})", preview.join(", "), sys.extensions.len() - 5)
        } else {
            sys.extensions.join(", ")
        };

        eprintln!("{:<6} {:<35} {:<12} {exts}", sys.id, sys.name, sys.system_type);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1023), "1023 B");
    }

    #[test]
    fn test_format_bytes_kilobytes() {
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
    }

    #[test]
    fn test_format_bytes_megabytes() {
        assert_eq!(format_bytes(1_048_576), "1.0 MB");
        assert_eq!(format_bytes(10_485_760), "10.0 MB");
    }

    #[test]
    fn test_format_bytes_gigabytes() {
        assert_eq!(format_bytes(1_073_741_824), "1.0 GB");
        assert_eq!(format_bytes(2_147_483_648), "2.0 GB");
    }
}

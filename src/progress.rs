use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

/// Progress tracking for the scrape pipeline.
pub struct ScrapeProgress {
    multi: MultiProgress,
    overall_bar: Option<ProgressBar>,
    system_bar: Option<ProgressBar>,
    stats: Arc<ScrapeStats>,
    per_system: Vec<SystemSummary>,
    current_system: Option<String>,
}

/// Per-system summary for the final table.
struct SystemSummary {
    name: String,
    scraped: u32,
    skipped: u32,
    not_found: u32,
    errors: u32,
}

/// Accumulated scrape statistics.
pub struct ScrapeStats {
    pub scraped: AtomicU32,
    pub skipped: AtomicU32,
    pub not_found: AtomicU32,
    pub errors: AtomicU32,
}

impl ScrapeStats {
    pub fn new() -> Self {
        Self {
            scraped: AtomicU32::new(0),
            skipped: AtomicU32::new(0),
            not_found: AtomicU32::new(0),
            errors: AtomicU32::new(0),
        }
    }

    pub fn inc_scraped(&self) {
        self.scraped.fetch_add(1, Ordering::Relaxed);
    }

    #[allow(dead_code)]
    pub fn inc_skipped(&self) {
        self.skipped.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_not_found(&self) {
        self.not_found.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_errors(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> (u32, u32, u32, u32) {
        (
            self.scraped.load(Ordering::Relaxed),
            self.skipped.load(Ordering::Relaxed),
            self.not_found.load(Ordering::Relaxed),
            self.errors.load(Ordering::Relaxed),
        )
    }
}

impl ScrapeProgress {
    pub fn new() -> Self {
        Self {
            multi: MultiProgress::new(),
            overall_bar: None,
            system_bar: None,
            stats: Arc::new(ScrapeStats::new()),
            per_system: Vec::new(),
            current_system: None,
        }
    }

    pub fn stats(&self) -> Arc<ScrapeStats> {
        self.stats.clone()
    }

    /// Set the total number of ROMs across all systems for the overall progress bar.
    pub fn set_total(&mut self, total: u64) {
        let bar = self.multi.add(ProgressBar::new(total));
        bar.set_style(
            ProgressStyle::default_bar()
                .template("{prefix} [{bar:40.green/dim}] {pos}/{len} ({eta})")
                .unwrap_or_else(|_| ProgressStyle::default_bar())
                .progress_chars("=> "),
        );
        bar.set_prefix("Overall");
        self.overall_bar = Some(bar);
    }

    /// Start progress for a new system.
    pub fn start_system(&mut self, system_name: &str, total: u64) {
        // Snapshot stats at system start to track per-system deltas
        self.current_system = Some(system_name.to_string());

        let bar = self.multi.add(ProgressBar::new(total));
        bar.set_style(
            ProgressStyle::default_bar()
                .template("{prefix} [{bar:30.cyan/blue}] {pos}/{len} {msg}")
                .unwrap_or_else(|_| ProgressStyle::default_bar())
                .progress_chars("=> "),
        );
        bar.set_prefix(format!("  {system_name}"));
        self.system_bar = Some(bar);
    }

    /// Increment the system progress bar (and overall).
    pub fn inc_system(&self) {
        if let Some(bar) = &self.system_bar {
            bar.inc(1);
        }
        if let Some(bar) = &self.overall_bar {
            bar.inc(1);
        }
    }

    /// Finish the current system progress bar.
    pub fn finish_system(&mut self) {
        if let Some(bar) = &self.system_bar {
            bar.finish_and_clear();
        }
        self.system_bar = None;
    }

    /// Record per-system stats for the summary table.
    pub fn record_system(
        &mut self,
        system_name: &str,
        scraped: u32,
        skipped: u32,
        not_found: u32,
        errors: u32,
    ) {
        self.per_system.push(SystemSummary {
            name: system_name.to_string(),
            scraped,
            skipped,
            not_found,
            errors,
        });
    }

    /// Print final summary table.
    #[allow(clippy::print_stderr)]
    pub fn print_summary(&self) {
        if let Some(bar) = &self.overall_bar {
            bar.finish_and_clear();
        }

        let (total_scraped, total_skipped, total_not_found, total_errors) = self.stats.snapshot();

        eprintln!();
        eprintln!("=== Scrape Summary ===");

        if !self.per_system.is_empty() {
            eprintln!(
                "{:<25} {:>8} {:>8} {:>10} {:>8}",
                "System", "Scraped", "Skipped", "Not Found", "Errors"
            );
            eprintln!("{}", "-".repeat(61));

            for s in &self.per_system {
                eprintln!(
                    "{:<25} {:>8} {:>8} {:>10} {:>8}",
                    s.name, s.scraped, s.skipped, s.not_found, s.errors
                );
            }

            eprintln!("{}", "-".repeat(61));
        }

        eprintln!(
            "{:<25} {:>8} {:>8} {:>10} {:>8}",
            "Total", total_scraped, total_skipped, total_not_found, total_errors
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scrape_stats_new() {
        let stats = ScrapeStats::new();
        assert_eq!(stats.snapshot(), (0, 0, 0, 0));
    }

    #[test]
    fn test_scrape_stats_increments() {
        let stats = ScrapeStats::new();
        stats.inc_scraped();
        stats.inc_scraped();
        stats.inc_skipped();
        stats.inc_not_found();
        stats.inc_errors();
        stats.inc_errors();
        stats.inc_errors();
        assert_eq!(stats.snapshot(), (2, 1, 1, 3));
    }

    #[test]
    fn test_scrape_progress_new() {
        let progress = ScrapeProgress::new();
        let stats = progress.stats();
        assert_eq!(stats.snapshot(), (0, 0, 0, 0));
    }

    #[test]
    fn test_scrape_progress_set_total() {
        let mut progress = ScrapeProgress::new();
        progress.set_total(100);
        assert!(progress.overall_bar.is_some());
    }

    #[test]
    fn test_scrape_progress_system_lifecycle() {
        let mut progress = ScrapeProgress::new();
        progress.set_total(10);
        progress.start_system("snes", 5);
        assert!(progress.system_bar.is_some());
        progress.inc_system();
        progress.finish_system();
        assert!(progress.system_bar.is_none());
    }

    #[test]
    fn test_scrape_progress_record_system() {
        let mut progress = ScrapeProgress::new();
        progress.record_system("snes", 10, 2, 1, 0);
        progress.record_system("megadrive", 5, 0, 0, 1);
        assert_eq!(progress.per_system.len(), 2);
    }

    #[test]
    fn test_print_summary_with_systems() {
        let mut progress = ScrapeProgress::new();
        progress.stats().inc_scraped();
        progress.stats().inc_scraped();
        progress.stats().inc_not_found();
        progress.record_system("snes", 2, 1, 1, 0);
        // Just verify it doesn't panic
        progress.print_summary();
    }

    #[test]
    fn test_print_summary_empty() {
        let progress = ScrapeProgress::new();
        progress.print_summary();
    }

    #[test]
    fn test_print_summary_with_overall_bar() {
        let mut progress = ScrapeProgress::new();
        progress.set_total(10);
        progress.stats().inc_scraped();
        progress.record_system("snes", 1, 0, 0, 0);
        progress.print_summary();
    }

    #[test]
    fn test_inc_system_without_bars() {
        let progress = ScrapeProgress::new();
        // Should not panic even without bars
        progress.inc_system();
    }

    #[test]
    fn test_finish_system_without_bar() {
        let mut progress = ScrapeProgress::new();
        // Should not panic even without a system bar
        progress.finish_system();
        assert!(progress.system_bar.is_none());
    }

    #[test]
    fn test_current_system_tracking() {
        let mut progress = ScrapeProgress::new();
        assert!(progress.current_system.is_none());
        progress.start_system("snes", 5);
        assert_eq!(progress.current_system.as_deref(), Some("snes"));
    }
}

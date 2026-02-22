# Phase 5: Cross-Platform Builds + Release Polish

## Goal
Ensure scrauper builds and runs on all target platforms (Linux x86_64, ARM64, Android ARM, macOS, Windows). Add CI/CD, documentation, and final polish.

## Dependencies
- Phase 4 complete (all features working)

## Steps

### 5.1 — Cross-compilation setup
- [ ] Add `.cargo/config.toml` with target-specific linker settings for cross-compilation
- [ ] Verify `cargo build --release --target aarch64-linux-android` succeeds
- [ ] Verify `cargo build --release --target armv7-linux-androideabi` succeeds
- [ ] Verify `cargo build --release --target aarch64-unknown-linux-gnu` succeeds
- [ ] Verify `cargo build --release --target x86_64-unknown-linux-gnu` succeeds
- [ ] Test binary on Android (Termux) with real ROM collection
- [ ] Test binary on Raspberry Pi with real ROM collection
- [ ] Test binary on Steam Deck (SteamOS) with real ROM collection

### 5.2 — CI/CD (GitHub Actions)
- [ ] Workflow: build + test on push/PR
- [ ] Matrix: Linux x86_64, macOS ARM64, Windows
- [ ] Cross-compile job: Linux ARM64, Android ARM64
- [ ] Release workflow: build all targets, create GitHub release with binaries
- [ ] Binary naming: `scrauper-{version}-{target}.tar.gz` (linux/mac) or `.zip` (windows)

### 5.3 — Binary size optimization
- [ ] `[profile.release]` in Cargo.toml: `lto = true`, `codegen-units = 1`, `strip = true`
- [ ] Measure binary size per target, aim for < 10MB
- [ ] Consider `opt-level = "z"` for ARM targets if binary too large

### 5.4 — Error messages and UX
- [ ] Clear error on missing config file: "No scrauper.toml found. Run `scrauper init` or create one from scrauper.toml.example"
- [ ] `scrauper init` command: copy example config to `./scrauper.toml`, prompt for credentials
- [ ] Colored terminal output (with `--no-color` flag for piping)
- [ ] Graceful Ctrl+C handling: finish current games, write partial gamelist, print summary
- [ ] `SCRAUPER_CONFIG` env var to override config path

### 5.5 — Documentation
- [ ] README.md: installation, quick start, configuration reference, usage examples
- [ ] CHANGELOG.md
- [ ] Man page or `--help` with full option documentation
- [ ] Example configs for common setups: RetroPie, Batocera, Steam Deck, Android

### 5.6 — Testing
- [ ] Integration tests with mock HTTP server (wiremock-rs)
- [ ] Unit tests for all modules with >80% coverage
- [ ] Test gamelist.xml round-trip: read → modify → write → read = identical
- [ ] Test miximage output determinism: same inputs = same output
- [ ] Benchmark: time per game at different thread counts

## Verification
- [ ] Binary runs on all target platforms
- [ ] CI/CD green on all targets
- [ ] `scrauper init` creates working config from example
- [ ] Ctrl+C during scrape writes partial progress (no data loss)
- [ ] README is clear enough for a new user to get started
- [ ] Binary size < 10MB on all targets

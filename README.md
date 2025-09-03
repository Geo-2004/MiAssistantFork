# MAF (MiAssistantFork) – Xiaomi Recovery / MiAssistant Flash & Rescue Tool

![MAF logo](assets/maflogo.png)

Open‑source Rust implementation of Xiaomi MiAssistant style recovery flashing for locked bootloader devices. If your Xiaomi / Redmi / POCO phone is stuck in Recovery (Connect to MiAssistant) or you need to reflash an official MIUI/HyperOS recovery package without unlocking the bootloader, MAF helps you:

- Reliable USB device discovery (ADB recovery interface)
- Safer ADB packet handling
- Modular architecture (core lib + CLI + optional GUI)
- Extensible validation + sideload pipeline
- Cross-platform (Windows / Linux / macOS)

## Workspace Layout
```
Cargo.toml            # workspace
crates/
  core/               # core library (USB, ADB, sideload, validation)
  cli/                # command line interface
  gui/                # experimental egui-based GUI (feature: ui)
```

## Status
Core alpha: USB detection, ADB connect handshake, packet send/receive, sideload streaming, validation encrypt/decrypt + JSON parse, CLI subcommands (detect, info, adb, list-roms, flash, md5). GUI is functional (experimental) and provides the main rescue flows described below.

- Device discovery is more permissive than the legacy C tool (does not require the ADB protocol byte which is unreliable on some devices) and auto-claims the correct bulk endpoints.
- Flash flow: compute MD5 of the recovery ZIP, request Xiaomi validation token, stream sideload with progress, handle OKAY/WRTE/CLSE framing reliably.

## Quick Start
1. Boot device into official Recovery (should show “Connect to MiAssistant”).
2. Connect via USB directly (avoid hubs).
3. Run detection:
```
cargo run -p miassistant-cli -- detect
```
4. Fetch device info / confirm communication:
```
cargo run -p miassistant-cli -- info
```
5. Flash an official recovery ROM ZIP (download from Xiaomi sources; keep file unmodified):
```
cargo run -p miassistant-cli -- flash /path/to/miui_ROM.zip
```
6. If interrupted (power loss / Ctrl+C) resume:
```
cargo run -p miassistant-cli -- sideload /path/to/miui_ROM.zip --resume
```

Need help / stuck? Join Discord: https://discord.gg/Mun6CsfQqa

## Building
Install Rust (https://rustup.rs).

```
# CLI build
cargo build -p miassistant-cli --release

# Run detect
cargo run -p miassistant-cli -- detect

# Query device info (must be in recovery / MiAssistant mode)
cargo run -p miassistant-cli -- info

# Flash an official recovery ROM (locked bootloader)
cargo run -p miassistant-cli -- flash /path/to/miui_ROM.zip

# Optional: sideload if you already have a token
# (auto-fetches token if omitted)
cargo run -p miassistant-cli -- sideload /path/to/miui_ROM.zip

# Maintenance actions
cargo run -p miassistant-cli -- format-data
cargo run -p miassistant-cli -- reboot
```

GUI (experimental / pre-built available):
You can build the GUI from source (requires Rust) or download a pre-built package from the GitHub Releases page (no Rust required for end users). To build locally:

```
cargo run -p miassistant-gui --features ui
```

## Developer setup (formatting & pre-commit hooks)

To avoid CI format/clippy failures, install and enable the pre-commit hooks used by this repository:

```bash
# Install pre-commit (one-time)
python -m pip install --user pre-commit

# Install hooks for this repo
pre-commit install

# Run hooks on all files (one-off)
pre-commit run --all-files

# If you prefer strict checks locally, enable strict mode
export PRE_COMMIT_STRICT=1
pre-commit run --all-files
```

Alternatively the repository contains a fallback git hook at `.githooks/pre-commit` (already configured when you clone this repo), which runs `cargo fmt --all` and clippy unless you set `SKIP_CLIPPY=1`.


GUI features (cross‑platform, eframe/egui):
- Device detection + info display
- ROM listing from Xiaomi (auth-compatible)
- File picker for recovery ZIPs
- Token retrieval (Validate) with erase indicator
- Flash with live progress, cancel, and retry
- Format data and reboot actions
- Multi-device selection when multiple ADB recovery interfaces are present
- Language toggle (EN/ES) and dark theme polish

Tip: If Erase=YES is shown after token retrieval, data will be wiped by the official package.

## Packaging
- GitHub Releases attach platform builds on tag pushes:
  - Windows: ZIP and MSI installer (GUI app). Winget/MSIX planned.
  - macOS: .app bundle + DMG
  - Linux: tar.gz with CLI and GUI binaries

GitHub Actions builds and uploads release artifacts on tagged commits; end users can download installers and packaged binaries directly from the Releases page without installing Rust or building locally. See `.github/workflows/release.yml` for details about the CI/release pipeline.

Binary Releases
----------------
If you don't want to build from source, download the appropriate artifact from the project's GitHub Releases page. Typical usage:

- Windows: run the installer or extract the ZIP and run the portable GUI/CLI binary
- macOS: open the `.app` bundle included in the DMG
- Linux: extract the `tar.gz` and run the CLI or GUI binary

Release artifacts are produced by the repository's CI on tagged pushes; check the release notes and attached checksums on each Release for verification.

## Roadmap
- [x] More ADB unit tests (framing, error cases) — header encode/decode tests
- [x] Enhanced logging spans + structured events (tracing in ADB transport)
- [x] Cancellation & resume for sideload (state file `<rom>.sideload.state`, `--resume` and Ctrl+C safe)
- [x] GUI device list + actions (multi-device select, info, token, flash, basic cancel) — early implementation
- [x] Windows packaging (MSI / portable zip) — release workflow builds ZIP + MSI
- [x] Continuous Integration (lint, test, release) — GitHub Actions workflow added
- [x] Fuzz test packet parser — libFuzzer target for ADB header decode (`crates/core/fuzz`)

### Sideload Resume / Cancel
Press Ctrl+C during `sideload` or `flash` to gracefully stop after the current block; progress state is saved to `<file>.sideload.state`. Resume with:
```
cargo run -p miassistant-cli -- sideload /path/to/ROM.zip --resume
```
The `flash` command always resumes automatically if state exists for the target file.

### Fuzzing
Install cargo-fuzz then run:
```
cargo install cargo-fuzz
cd crates/core/fuzz && cargo +nightly fuzz run fuzz_adb_header
```

## License
MIT

## Support & Community
Discord (user support, troubleshooting, development chat): https://discord.gg/Mun6CsfQqa

Please include when asking for help:
- Output of `cargo run -p miassistant-cli -- detect -v`
- Output of `cargo run -p miassistant-cli -- info -v` (redact serial if you wish)
- Exact ROM filename you’re flashing
- Platform (Windows / Linux / macOS) and Rust version (`rustc --version`)

## SEO / Discovery
Keywords (for search engines):
```
xiaomi recovery connect to assistant tool
miassistant tool alternative open source
flash official miui hyperos recovery rom locked bootloader
xiaomi phone stuck recovery need reflash sideload
miui recovery rom validation token erase data
unbrick xiaomi without unlocking bootloader
```
If you found this via “Connect to MiAssistant” screen: this tool communicates over the recovery’s ADB-like bulk endpoints, obtains Xiaomi’s validation token (needed for locked bootloader official packages), and streams the ZIP with integrity checks. It does NOT bypass security or flash unofficial builds.

## Credits
Original C prototype: https://github.com/offici5l/MiAssistantTool (superseded). This fork (MAF) modernizes in Rust with safer USB + packet handling.

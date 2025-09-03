# MiAssistantTool v2 (Rust Rewrite)

This is a ground-up Rust rewrite of the original C-based MiAssistantTool. Goals:

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
Core alpha: USB detection, ADB connect handshake, packet send/receive, sideload streaming, validation encrypt/decrypt + JSON parse, CLI subcommands (detect, info, adb, list-roms, flash, md5). GUI remains placeholder.

## Building
Install Rust (https://rustup.rs).

```
# CLI build
cargo build -p miassistant-cli --release

# Run detect
cargo run -p miassistant-cli -- detect

# Query device info (must be in recovery / MiAssistant mode)
cargo run -p miassistant-cli -- info
```

GUI (placeholder):
```
cargo run -p miassistant-gui --features ui
```

## Roadmap
- [ ] More ADB unit tests (framing, error cases)
- [ ] Enhanced logging spans + structured events
- [ ] Cancellation & resume for sideload
- [ ] GUI device list + actions
- [ ] Windows packaging (MSI / portable zip)
- [ ] Continuous Integration (lint, test, release)
- [ ] Fuzz test packet parser

## License
MIT

# rldyourterm

## Overview

`rldyourterm` is a high-performance terminal platform with a strong focus on:

- low-latency rendering and smooth input/output
- cross-platform consistency (Linux, macOS, Windows)
- GPU-accelerated graphics pipeline
- operational reliability for daily and production workflows

It combines a terminal engine, configuration model, launcher scripts, and
distribution workflow in a single repository.

## Repository Purpose

This repository delivers a production-ready terminal distribution and maintenance
framework, including:

- terminal source and CLI tooling
- runtime packaging and release automation
- platform-aware installers and health checks
- documentation, reference guides, and operational runbooks

The project is intentionally pragmatic: stable defaults, predictable upgrades, and
repeatable setup for local and team usage.

## Design Pillars

### Performance and Stability

- Minimized startup and interaction latency through Rust-centered architecture.
- GPU rendering path for advanced graphics workloads.
- Deterministic startup/install flow with explicit health checks.
- Careful dependency pinning and regular lockfile hygiene.

### Cross-Platform Engineering

- Separate platform layers with shared orchestration.
- Native script entrypoints for Linux, macOS, and Windows.
- Consistent configuration and validation model across all targets.

### Developer Velocity

- Clear folder layout for tooling, configuration, docs, and release operations.
- Reproducible environment bootstrap steps.
- Structured contributor guidance and security/documentation standards.

## Getting Started

Clone the repository and run:

```bash
git clone https://github.com/rldyourmnd/rldyourterm.git
cd rldyourterm
```

Use the platform-specific installer/health scripts under `scripts/`:

- `./scripts/install.sh` for Linux/macOS dispatcher entry
- `.\scripts\install.ps1` on Windows
- `./scripts/health-check.sh` or platform-specific equivalents

Run dry-run mode first to validate environment before applying changes.

## Project Structure

```text
.
├── rldyourterm/                    # Terminal source (submodule content)
├── scripts/                        # Installers, health checks, launch helpers
├── docs/                           # Architecture and usage documentation
├── wiki/                           # Extended operational knowledge base
├── configs/                        # Default configuration layers
└── .github/                        # CI and contribution workflows
```

## Versioning

The repository is currently aligned to `1.0.0` and targets stable, backward-consistent upgrades.

## Build and Run

Build commands are defined in `Makefile` and platform scripts. For a regular workspace
build from inside `rldyourterm`:

```bash
cd rldyourterm
cargo build --workspace --release
```

Check release notes and operational runbooks in `docs/operations/` before upgrading major
dependencies or toolchain baselines.

## Release

- `docs/operations/release.md`

## Contributing

- Review `CONTRIBUTING.md` and `SECURITY.md`.
- Open issues and follow project-specific triage guidance in repository docs.
- Align changes with existing naming, scripting, and documentation standards.

## License

See `LICENSE`.

Version: 1.0.0
Special thanks to the original WezTerm authors and contributors (MIT licensed) for the architectural foundation that inspired this project.

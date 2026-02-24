# Linux (Debian/Ubuntu)

This is the production implementation for Linux hosts.

## Scope

- Distributions: Debian/Ubuntu (APT-based)
- Shell: Fish
- Terminal: rldyourterm
- Prompt: Starship
- Layers: Foundation + Layer 1..5
- Install style: non-interactive (`apt-get` + `DEBIAN_FRONTEND=noninteractive`)
- Reproducibility: Cargo-based tools installed with `cargo install --locked`

## Entrypoints

- Full install: `./scripts/install.sh`
- Flow dry-run: `./scripts/install.sh --dry-run`
- Installer help: `./scripts/install.sh --help`
- Health check: `./scripts/health-check.sh --summary`
- NVML/NVIDIA recovery: `./scripts/linux/fix-nvidia-nvml.sh`

## Layer Scripts

- `scripts/linux/install-foundation.sh`
- `scripts/linux/install-layer-1.sh`
- `scripts/linux/install-layer-2.sh`
- `scripts/linux/install-layer-3.sh`
- `scripts/linux/install-layer-4.sh`
- `scripts/linux/install-layer-5.sh`

## Canonical Linux Docs

- Foundation: `docs/platforms/linux/reference/foundation.md`
- Layers: `docs/platforms/linux/reference/layers/`
- Operations: `docs/operations/`
- Tool catalog (Linux): `docs/platforms/linux/reference/terminal-tool-catalog.md`

## Notes

`./scripts/install.sh` now auto-detects OS. On Linux it runs the existing Linux pipeline.

## rldyourterm Runtime Modes

Linux rldyourterm config is stability-first by default:

- Linux baseline -> X11/XWayland (`enable_wayland=false`).
- Native Wayland is opt-in via override.
- Renderer default -> `OpenGL` (stable baseline).

Explicit runtime overrides:

```bash
rldyourterm-stable --mode stable
rldyourterm-stable --mode wayland
rldyourterm-stable --mode passthrough
rldyourterm-stable --mode software
rldyourterm-stable --mode stable
rldyourterm-stable --mode minimal
```

Preferred wrapper (repo launcher) for quick stable sessions:

```bash
rldyourterm-stable --mode stable
rldyourterm-stable --mode minimal
rldyourterm-stable --mode wayland
rldyourterm-stable --mode software
rldyourterm-stable --mode passthrough
rldyourterm-stable --mode stable -- vim
```

If you pass an additional argument after `--mode`, it is treated as a command. For safety, launcher uses `--` separator:
- valid: `rldyourterm-stable --mode stable -- vim`
- run command directly only when needed: `RLDYOURTERM_STABLE_ALLOW_COMMAND_ARGS=1 rldyourterm-stable somecommand`

## Operational Guarantees

- Official shell installers are downloaded to a temporary file and executed locally (no direct `curl | sh` pattern).
- Binary tarball installs for tools such as `lazygit`, `glow`, and `grepai` attempt checksum validation when release checksums are published.
- `health-check.sh` validates real `ast-grep` resolution (avoids false-positive system `sg` binary).

## CI Coverage

- Linux flow smoke: `./scripts/install.sh --dry-run`
- Shell script linting + syntax validation in CI workflows

## Source References (Context7-Verified)

- Cargo install lockfile semantics (`--locked`):
  <https://github.com/rust-lang/cargo/blob/master/src/doc/src/commands/cargo-install.md>
- npm global install permissions (`prefix ~/.local`):
  <https://docs.npmjs.com/resolving-eacces-permissions-errors-when-installing-packages-globally>

## Optional Integrity Pinning

For `yq` in Layer 1, you can pin checksum explicitly:

```bash
export YQ_SHA256="<sha256-from-official-release>"
./scripts/linux/install-layer-1.sh
```

## Recommended Linux Verification Flow

```bash
bash -n scripts/*.sh scripts/macos/*.sh
./scripts/health-check.sh --summary
./scripts/health-check.sh --strict
```

If NVML reports `nvidia-smi: Failed to initialize NVML: Unknown Error`, run:

```bash
./scripts/linux/fix-nvidia-nvml.sh
```

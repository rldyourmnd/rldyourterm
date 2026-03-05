# CLAUDE.md - rldyourterm

## Project

Crash-intolerant AI terminal runtime written in Rust. A custom terminal emulator optimized for long-running AI CLI sessions (Claude Code, Codex, Gemini CLI, OpenCode).

**Priorities (hard order):** stability > AI CLI compatibility > speed

**Version:** 0.1.0 (MVP)

## Architecture

Cargo workspace with 12 crates following VSA (Vertical Slice Architecture). Dependency flow is strictly inward:

```
app -> ui -> features -> services -> core
              |
              v
         foundation (API traits)
              ^
              |
      foundation-platform (OS implementations)
```

### Crate Map

| Crate | Path | Purpose |
|-------|------|---------|
| rldyourterm-core | crates/core | Terminal domain: grid, cursor, parser, scrollback, state, events |
| rldyourterm-foundation | crates/foundation | API traits: PtyIo, PtyFactory, WindowControl, ClipboardPort |
| rldyourterm-foundation-platform | crates/foundation-platform | OS adapters: portable-pty, winit, arboard |
| rldyourterm-services | crates/services | Controllers: Session, RenderMode, RenderPacing |
| rldyourterm-ui | crates/ui | UiRuntime: bootstrap, command loop, tick, single-window |
| rldyourterm-font | crates/features/font | Glyph rasterization (fontdue + JetBrains Mono Nerd Font), shared cache |
| render_cpu | crates/features/render_cpu | CPU renderer (softbuffer): delta/full modes, dirty row tracking |
| render_gpu | crates/features/render_gpu | GPU renderer (wgpu): dynamic glyph atlas, surface error recovery |
| settings | crates/features/settings | SettingsService: mode, shell, theme, command palette |
| shell_integration | crates/features/shell_integration | Shell detection, resolution (fish+starship, zsh fallback) |
| diagnostics | crates/features/diagnostics | Structured events, correlation IDs, typed payloads |
| rldyourterm-app | crates/app | CLI entry, GUI runtime, TTY runtime, MVP harness |

### Entry Points

- `crates/app/src/main.rs` - CLI parsing, shell resolution, runtime dispatch
- `crates/app/src/gui_runtime.rs` - GUI window path (winit + softbuffer + PTY)
- `crates/app/src/pty_runtime.rs` - TTY fallback path (crossterm + PTY)
- `crates/app/src/shared.rs` - Cross-runtime utilities (key encoding, PTY boundary, display tokens)

## Commands

### Build

```bash
cargo check --workspace          # type-check all crates
cargo check -p rldyourterm-app   # type-check app crate
cargo build -p rldyourterm-app   # debug build
cargo build --workspace          # build all
```

### Run

```bash
# GUI terminal (default)
cargo run -q -p rldyourterm-app -- --mode auto --shell fish --window-count 1

# TTY fallback mode
cargo run -q -p rldyourterm-app -- --mode auto --shell fish --window-count 1 --tty
```

### Test

```bash
cargo test --workspace                          # all tests
cargo test -p rldyourterm-core                  # single crate
cargo test -p rldyourterm-core -- test_name     # single test
```

### Lint and Format

```bash
cargo fmt --all                  # format
cargo fmt --all -- --check       # check formatting
cargo clippy --workspace         # lint
cargo clippy --workspace -- -D warnings  # lint strict
```

### MVP Harness

```bash
bash scripts/mvp/run_matrix.sh 3    # release gate (3 iterations)
bash scripts/mvp/run_matrix.sh 5    # extended soak
```

## Code Conventions

- **Rust edition 2024**, minimum rust-version 1.92
- **Naming**: snake_case functions/variables, PascalCase types/enums
- **Errors**: thiserror for typed domain errors, anyhow at application boundaries
- **Error enums**: one per layer (CoreError, FoundationError, ServiceError, UiRuntimeError)
- **Testing**: inline `#[cfg(test)] mod tests`, descriptive snake_case names, no mock crates
- **Modules**: single-file crates (lib.rs per feature crate)
- **Patterns**: trait-based ports (foundation), platform adapters, controller pattern (services), command/receipt (UI)
- **No silent fallback**: every transition logged with correlation

## CI/CD

GitHub Actions pipeline (`.github/workflows/ci.yml`) runs on push/PR to `main`/`dev`:

| Job | Purpose |
|-----|---------|
| Check | `cargo check --workspace` |
| Clippy | `cargo clippy --workspace -- -D warnings` |
| Test | `cargo test --workspace` |
| Format | `cargo fmt --all -- --check` |
| MSRV | `cargo check --workspace` with Rust 1.92 |
| Audit | `cargo-audit` via rustsec/audit-check |

Dependabot: weekly Cargo + GitHub Actions updates (`.github/dependabot.yml`).

## Quality Gates

Before any commit:

1. `cargo check --workspace` passes
2. `cargo clippy --workspace -- -D warnings` passes
3. `cargo test --workspace` passes
4. `cargo fmt --all -- --check` passes

## Commit Convention

Conventional Commits format:

```
type(scope): description
```

Types: feat, fix, refactor, docs, test, chore, perf, style. English only.

## Quality Standards

Three non-negotiable quality pillars (hard priority order):

### 1. Stability (Crash-Intolerant)
- No `panic!`, `unwrap()`, or `todo!()` in library/production code
- Every error boundary uses typed `Result<T, E>` with `thiserror` enums
- GPU/PTY/runtime faults must not terminate active shell sessions
- Bounded retry with deterministic fallback at every critical boundary
- Event-correlated observability for all state transitions

### 2. AI CLI Compatibility
- First-class support: Claude Code, Codex, Gemini CLI, OpenCode
- Complete VT100/xterm sequence parsing (CSI, OSC, DCS, SGR)
- Correct key encoding (Ctrl+C/D, arrow keys, F-keys, bracketed paste)
- High-throughput PTY I/O for long-running AI sessions
- Full Unicode rendering: ASCII, Latin Extended, Cyrillic, CJK, Nerd Font icons, Box Drawing

### 3. Speed (Ultra-Low Latency)
- Monitor-driven frame pacing (no hardcoded FPS)
- Cadence preservation during monitor transfer (no transient loss)
- Delta rendering with dirty row tracking (CPU renderer)
- Dynamic glyph atlas with partial texture uploads (GPU renderer)
- Bounded CPU/RAM growth under sustained load
- 64KB PTY read buffer (matches MAX_FEED_BYTES_PER_CALL, 16x fewer syscalls)
- Batch text processing: `PrintText(String)` parser action avoids per-char Vec pushes
- Dead event elimination: only grid dirty tracking used by renderers (no CellUpdated/CursorMoved)

## Key Invariants

- Render modes: cpu, gpu, auto (gpu-first with bounded retry, then cpu fallback)
- Single-window baseline for v1.0
- Shell baseline: fish + starship, fallback: zsh
- Scrollback cap: 50,000 lines
- Frame pacing: monitor-driven cadence (no hardcoded FPS)
- Settings UX: in-terminal command palette (Ctrl/Cmd+Shift+P)
- Target platforms: Ubuntu 22.04/24.04/25.10, macOS

## Authority

- `AGENTS.md` is the highest-authority document for project governance
- Planning docs: `planning/` directory
- Serena memories: `.serena/memories/` for project knowledge

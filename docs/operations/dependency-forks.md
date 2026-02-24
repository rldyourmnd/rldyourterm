# Forked dependency policy (x11/graphics)

## Purpose

To make `rldyourterm` independent from external upstream branches, selected
graphics-related git dependencies are now sourced from forks under `rldyourmnd`.

## Active forked git dependencies

| Dependency | Fork source | Upstream source | Branch |
|---|---|---|---|
| `finl_unicode` | `https://github.com/rldyourmnd/finl_unicode.git` | `https://github.com/wez/finl_unicode.git` | `no_std` |
| `xcb-imdkit` | `https://github.com/rldyourmnd/xcb-imdkit-rs.git` | `https://github.com/wez/xcb-imdkit-rs.git` | `main` |

## Operational process

1. Keep fork branches aligned with upstream intent, not automatically forced.
2. Track changes in `rldyourterm/Cargo.toml`:
   - `finl_unicode` → `rldyourmnd/finl_unicode`
   - `xcb-imdkit` → `rldyourmnd/xcb-imdkit-rs`
3. After any dependency update:
   - update the upstream-to-fork merge strategy locally;
   - run a dedicated dependency smoke pass;
   - update `CHANGELOG.md` in `[Unreleased]`.

## Useful commands

```bash
# create forks (one-time, done)
gh repo fork wez/finl_unicode --clone=false --remote=false
gh repo fork wez/xcb-imdkit-rs --clone=false --remote=false

# sync latest upstream into your fork when needed
gh repo sync rldyourmnd/finl_unicode --source wez/finl_unicode
gh repo sync rldyourmnd/xcb-imdkit-rs --source wez/xcb-imdkit-rs
```

## Verification

- Confirm `Cargo.toml` points to `rldyourmnd` fork URLs for all git-based x11/graphics
  dependencies.
- Confirm builds still reference forked dependencies by checking lockfile updates
  during dependency refresh.

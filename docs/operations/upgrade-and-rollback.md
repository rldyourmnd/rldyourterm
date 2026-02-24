# Upgrade and Rollback Playbook

This playbook supports deterministic upgrades without breaking existing terminals.

## Standard Upgrade Procedure

1. Snapshot current state.

```bash
git status --short
cp -a ~/.rldyourterm.lua ~/.rldyourterm.lua.pre-upgrade.bak
cp -a ~/.config/fish/config.fish ~/.config/fish/config.fish.pre-upgrade.bak
cp -a ~/.config/starship.toml ~/.config/starship.toml.pre-upgrade.bak
```

2. Pull latest repo changes.

```bash
git pull --ff-only origin main
```

3. Re-run installer layers in order.

```bash
./scripts/linux/install-foundation.sh
./scripts/linux/install-layer-1.sh
./scripts/linux/install-layer-2.sh
./scripts/linux/install-layer-3.sh
./scripts/linux/install-layer-4.sh
./scripts/linux/install-layer-5.sh
```

4. Run health-check and confirm success criteria.

```bash
./scripts/health-check.sh --summary
```

macOS / Windows:

```bash
./scripts/health-check-macos.sh --summary
```

```powershell
.\scripts\health-check-windows.ps1 -Summary
```

5. Validate runtime.

```bash
exec fish
lazygit
gh --version
grepai help
journalctl --user -b --since '20 minutes ago' | rg -n "size change accounting|frame counter but no frame drawn time|MetaShapedTexture|update-status event: runtime error"
```

Optional renderer/display validation matrix:

```bash
RLDYOURTERM_FORCE_X11=1 RLDYOURTERM_STABLE_RESIZE=1 rldyourterm-stable --mode stable
RLDYOURTERM_FORCE_X11=1 RLDYOURTERM_STABLE_RESIZE=1 rldyourterm-stable --mode stable
RLDYOURTERM_FORCE_X11=1 RLDYOURTERM_MINIMAL_UI=1 rldyourterm-stable --mode minimal
RLDYOURTERM_FORCE_WAYLAND=1 rldyourterm-stable --mode wayland
RLDYOURTERM_SAFE_RENDERER=1 rldyourterm-stable --mode software

# With installed launcher:
rldyourterm-stable --mode stable
rldyourterm-stable --mode minimal
rldyourterm-stable --mode wayland
rldyourterm-stable --mode software
```

## Rollback Procedure

If the upgrade causes breakage:

1. Capture current bad state and switch to a known-good commit in a dedicated rollback branch.

```bash
git log --oneline -n 20
git switch -c rollback/<date>-<issue>
git checkout <known-good-commit>
```

2. Restore previous configs from backup copies.

```bash
mv ~/.rldyourterm.lua.pre-upgrade.bak ~/.rldyourterm.lua
mv ~/.config/fish/config.fish.pre-upgrade.bak ~/.config/fish/config.fish
mv ~/.config/starship.toml.pre-upgrade.bak ~/.config/starship.toml
```

3. Re-run health-check in summary mode for your platform.

```bash
./scripts/health-check.sh --summary
```

```bash
./scripts/health-check-macos.sh --summary
```

```powershell
.\scripts\health-check-windows.ps1 -Summary
```

4. If shell behavior remains broken, start with a fresh login shell session and re-open.

## Controlled Rollback with Git

- For repository-level revert:

```bash
git log --oneline -n 10
git checkout <known-good-commit>
```

- For a hotfix path, apply only a minimal commit.

## Safety Rules

- Never remove `~/.local/bin` content unless restoring a clean backup.
- Never run `apt` commands while package manager lock is active.
- Always run health-check before and after any bulk change.

## Sign-off

- Upgrade complete only when:
  - `./scripts/health-check.sh --summary` passes required checks
  - shell startup and core layer commands work in a fresh shell
  - config parity is intentional (or documented exception)

# Contributing to rldyourterm

First off, thank you for considering contributing to rldyourterm! 🎉

## 📜 Table of Contents

- [Code of Conduct](#code-of-conduct)
- [How Can I Contribute?](#how-can-i-contribute)
- [Development Setup](#development-setup)
- [Pull Request Guidelines](#pull-request-guidelines)
- [Coding Standards](#coding-standards)
- [Tool Evaluation Criteria](#tool-evaluation-criteria)

## 🤝 Code of Conduct

This project and everyone participating in it is governed by our [Code of Conduct](CODE_OF_CONDUCT.md). By participating, you are expected to uphold this code.

## 🔧 How Can I Contribute?

### Report Bugs

Before creating bug reports, please check the [existing issues](https://github.com/rldyourmnd/rldyourterm/issues) as your issue might have already been reported.

When you are creating a bug report, please include as many details as possible:

- **Use a clear and descriptive title**
- **Describe the exact steps to reproduce the problem**
- **Provide specific examples to demonstrate the steps**
- **Describe the behavior you observed and expected**
- **Include your environment details** (OS, shell, terminal)

### Suggest Enhancements

Enhancement suggestions are tracked as [GitHub issues](https://github.com/rldyourmnd/rldyourterm/issues). When creating an enhancement suggestion, include:

- **Use a clear and descriptive title**
- **Provide a step-by-step description of the suggested enhancement**
- **Provide specific examples to demonstrate the steps**
- **Describe the current behavior and explain the expected behavior**
- **Explain why this enhancement would be useful**

### Pull Requests

- Fill in the required template
- Do not include issue numbers in the PR title
- Include screenshots and animated GIFs in your pull request whenever possible
- Follow the coding standards
- Document new code

## ✅ Pre-Flight Checklist

For PRs that touch installation or runtime behavior, include output from:

```bash
bash -n scripts/*.sh
fish -n configs/fish/config.fish
./scripts/health-check.sh --summary
```

Platform-specific checks (when relevant):

- Linux flow smoke: `./scripts/install.sh --dry-run`
- macOS flow smoke: `./scripts/macos/install.sh --dry-run`
- Windows flow smoke: `.\scripts\install-windows.ps1 -DryRun`
- Windows health-check: `.\scripts\health-check-windows.ps1 -Summary`

If you changed tool versions or validation assumptions:

- update platform docs and runbooks under `docs/`
- add a release note in `CHANGELOG.md` under `[Unreleased]`

## 🛠️ Development Setup

```bash
# Fork the repository on GitHub (if you need an alternative)
# Clone repository
git clone https://github.com/YOUR_USERNAME/rldyourterm.git
cd rldyourterm

# Create a branch for your changes
git checkout -b feature/your-feature-name

# Make your changes
# Test your changes

# Commit your changes
git add .
git commit -m "feat: description of your changes"

# Push to your branch
git push origin feature/your-feature-name

# Create a Pull Request on GitHub
```

## 📋 Pull Request Guidelines

1. **One feature per PR** - Keep changes focused and reviewable
2. **Test your changes** - Verify scripts work on fresh systems
3. **Update documentation** - Keep README and docs current
4. **Follow conventions** - Match existing code style
5. **Write clear commit messages** - Use conventional commits format

### Commit Message Format

We follow [Conventional Commits](https://www.conventionalcommits.org/):

```
type(scope): description

[optional body]
```

**Types:**
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `style`: Code style changes (formatting, semicolons, etc.)
- `refactor`: Code refactoring
- `perf`: Performance improvements
- `test`: Adding or updating tests
- `chore`: Maintenance tasks

**Examples:**
```
feat(layer-2): add hyperfine benchmarking tool
fix(foundation): correct rldyourterm config path
docs(readme): update installation instructions
```

## 📏 Coding Standards

### Shell Scripts

```bash
#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════════════
# SCRIPT NAME
# ═══════════════════════════════════════════════════════════════════════════════
# Description of what this script does
# Run: ./scripts/script-name.sh

set -euo pipefail  # Always use strict mode

# Use meaningful variable names
TOOL_NAME="example"

# Use functions with clear names
install_tool() {
    # Implementation
}

# Logging pattern
log_info() { echo -e "[INFO] $1"; }
log_success() { echo -e "[SUCCESS] $1"; }
log_error() { echo -e "[ERROR] $1"; }
```

### Configuration Files

- Include comments explaining options
- Follow tool-specific conventions
- Keep minimal but complete
- Use visual section separators

### Documentation

- Use clear, concise language
- Include code examples
- Update table of contents when adding sections
- Add proper markdown formatting

## 📊 Tool Evaluation Criteria

When suggesting new tools, consider these criteria:

| Criterion | Weight | Description |
|-----------|--------|-------------|
| Performance | 40% | Benchmark scores and actual speed improvements |
| Developer Experience | 25% | Usability, documentation, learning curve |
| Documentation | 15% | Quality of official docs and community resources |
| Maintenance | 10% | Active development, recent releases |
| Community | 10% | Stars, forks, community size |

### Evaluation Process

1. **Research**: Check benchmarks, documentation, GitHub stars
2. **Test**: Install and use the tool in daily workflow
3. **Compare**: Evaluate against existing alternatives
4. **Document**: Add to appropriate layer with clear usage examples

## ❓ Questions?

Feel free to:
- Open an [Issue](https://github.com/rldyourmnd/rldyourterm/issues)
- Ask in an issue with the `question` label

---

Thank you for contributing! 🙌

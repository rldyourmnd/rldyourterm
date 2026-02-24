# v1.0.0 Research Decisions

- Platform priority: Linux (Ubuntu/deb) and macOS.
- Delivery strategy: сначала ядро и стабильность, затем расширения.
- UI strategy: in-terminal command palette first; configuration-as-runtime.
- Rendering: CPU+GPU mandatory from first milestone; adaptive fallback for reliability.
- Shell: default `fish + Starship`, fallback `zsh`.
- Bash policy v1.0: не целевой default, совместимость базовых ANSI выше legacy расширений.
- Theme policy: start with `cuberpunk` profile.
- Stability policy: automatic mode switch and bounded retry on renderer failures.
- Release policy: manual validation, no CI.
- Documentation policy: all v1.0 decisions consolidated in `planning/v1.0.0-development-blueprint.md`.

## Documentation lock mapping
- External stack contracts: `planning/stack/v1.0.0-module-integration-contracts.md`
- Acceptance criteria: `planning/quality/v1.0.0-acceptance-matrix.md`
- Risk controls: `planning/risk/v1.0.0-risk-matrix.md`
- Release readiness: `planning/operations/v1.0.0-release-pack.md`

// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

const AI_CLI_SPAWN_ENV_DEFAULTS: [(&str, &str); 4] = [
    // Widely respected by JS CLIs to avoid periodic self-update checks.
    ("NO_UPDATE_NOTIFIER", "1"),
    // npm: avoid version-notifier network chatter on every install/exec flow.
    ("NPM_CONFIG_UPDATE_NOTIFIER", "false"),
    // npm: suppress end-of-install funding summary noise in long agent loops.
    ("NPM_CONFIG_FUND", "false"),
    // npm: disable TTY progress bars to reduce PTY output pressure.
    ("NPM_CONFIG_PROGRESS", "false"),
];

pub(crate) fn ai_cli_spawn_env_overrides() -> Vec<(String, String)> {
    ai_cli_spawn_env_overrides_with(|key| std::env::var_os(key).is_some())
}

fn ai_cli_spawn_env_overrides_with(is_present: impl Fn(&str) -> bool) -> Vec<(String, String)> {
    AI_CLI_SPAWN_ENV_DEFAULTS
        .iter()
        .filter_map(|(key, value)| {
            if is_present(key) {
                None
            } else {
                Some(((*key).to_owned(), (*value).to_owned()))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn ai_cli_spawn_env_overrides_include_defaults_when_unset() {
        let overrides = ai_cli_spawn_env_overrides_with(|_| false);
        let keys = overrides
            .iter()
            .map(|(key, _)| key.as_str())
            .collect::<BTreeSet<_>>();

        assert_eq!(
            keys,
            BTreeSet::from([
                "NO_UPDATE_NOTIFIER",
                "NPM_CONFIG_UPDATE_NOTIFIER",
                "NPM_CONFIG_FUND",
                "NPM_CONFIG_PROGRESS",
            ])
        );
    }

    #[test]
    fn ai_cli_spawn_env_overrides_respect_preconfigured_environment() {
        let preconfigured = BTreeSet::from(["NO_UPDATE_NOTIFIER", "NPM_CONFIG_FUND"]);
        let overrides = ai_cli_spawn_env_overrides_with(|key| preconfigured.contains(key));
        let keys = overrides
            .iter()
            .map(|(key, _)| key.as_str())
            .collect::<BTreeSet<_>>();

        assert_eq!(
            keys,
            BTreeSet::from(["NPM_CONFIG_PROGRESS", "NPM_CONFIG_UPDATE_NOTIFIER"])
        );
    }
}

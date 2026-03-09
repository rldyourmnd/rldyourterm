// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use rldyourterm_services::render_mode::RenderMode;

use crate::{
    RenderCadencePolicy, RuntimeProfilePreset, SettingsCommand, SettingsCommandParseError,
    ShellTarget, ThemePreset,
};

pub fn parse_palette_command(input: &str) -> Result<SettingsCommand, SettingsCommandParseError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(SettingsCommandParseError::EmptyInput);
    }

    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    let command = tokens[0].to_ascii_lowercase();

    match command.as_str() {
        "mode" => parse_mode_command(&tokens),
        "shell" => parse_shell_command(&tokens),
        "render" => parse_render_command(&tokens),
        "theme" => parse_theme_command(&tokens),
        "profile" => parse_profile_command(&tokens),
        "debug" => parse_debug_command(&tokens),
        _ => Err(SettingsCommandParseError::UnknownCommand { command }),
    }
}

pub(crate) fn canonicalize_palette_input(input: &str) -> String {
    input
        .split_whitespace()
        .map(normalize_token)
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_mode_command(tokens: &[&str]) -> Result<SettingsCommand, SettingsCommandParseError> {
    if tokens.len() < 2 {
        return Err(SettingsCommandParseError::MissingArgument {
            command: "mode".to_string(),
            expected: "cpu|gpu|auto",
        });
    }

    if tokens.len() > 2 {
        return Err(SettingsCommandParseError::UnexpectedTrailingInput {
            command: "mode".to_string(),
            trailing: normalize_trailing_tokens(tokens, 2),
        });
    }

    let mode = match normalize_token(tokens[1]).as_str() {
        "cpu" => RenderMode::Cpu,
        "gpu" => RenderMode::Gpu,
        "auto" => RenderMode::Auto,
        value => {
            return Err(SettingsCommandParseError::InvalidValue {
                field: "mode",
                value: value.to_string(),
                expected: "cpu|gpu|auto",
            });
        }
    };

    Ok(SettingsCommand::SetMode(mode))
}

fn parse_shell_command(tokens: &[&str]) -> Result<SettingsCommand, SettingsCommandParseError> {
    if tokens.len() < 2 {
        return Err(SettingsCommandParseError::MissingArgument {
            command: "shell".to_string(),
            expected: "fish|zsh|auto|auto-init on|off",
        });
    }

    let shell_option = normalize_token(tokens[1]);
    if shell_option == "auto-init" {
        if tokens.len() < 3 {
            return Err(SettingsCommandParseError::MissingArgument {
                command: "shell auto-init".to_string(),
                expected: "on|off",
            });
        }

        if tokens.len() > 3 {
            return Err(SettingsCommandParseError::UnexpectedTrailingInput {
                command: "shell auto-init".to_string(),
                trailing: normalize_trailing_tokens(tokens, 3),
            });
        }

        let enabled = match normalize_token(tokens[2]).as_str() {
            "on" => true,
            "off" => false,
            value => {
                return Err(SettingsCommandParseError::InvalidValue {
                    field: "shell auto-init",
                    value: value.to_string(),
                    expected: "on|off",
                });
            }
        };
        return Ok(SettingsCommand::SetShellAutoInit(enabled));
    }

    if tokens.len() > 2 {
        return Err(SettingsCommandParseError::UnexpectedTrailingInput {
            command: "shell".to_string(),
            trailing: normalize_trailing_tokens(tokens, 2),
        });
    }

    let target = match shell_option.as_str() {
        "fish" => ShellTarget::Fish,
        "zsh" => ShellTarget::Zsh,
        "auto" => ShellTarget::Auto,
        value => {
            return Err(SettingsCommandParseError::InvalidValue {
                field: "shell",
                value: value.to_string(),
                expected: "fish|zsh|auto|auto-init on|off",
            });
        }
    };
    Ok(SettingsCommand::SetShellTarget(target))
}

fn parse_render_command(tokens: &[&str]) -> Result<SettingsCommand, SettingsCommandParseError> {
    if tokens.len() < 2 {
        return Err(SettingsCommandParseError::MissingArgument {
            command: "render".to_string(),
            expected: "cadence monitor-auto",
        });
    }

    if normalize_token(tokens[1]) != "cadence" {
        return Err(SettingsCommandParseError::InvalidValue {
            field: "render",
            value: normalize_token(tokens[1]),
            expected: "cadence monitor-auto",
        });
    }

    if tokens.len() < 3 {
        return Err(SettingsCommandParseError::MissingArgument {
            command: "render cadence".to_string(),
            expected: "monitor-auto",
        });
    }

    if tokens.len() > 3 {
        return Err(SettingsCommandParseError::UnexpectedTrailingInput {
            command: "render cadence".to_string(),
            trailing: normalize_trailing_tokens(tokens, 3),
        });
    }

    match normalize_token(tokens[2]).as_str() {
        "monitor-auto" => Ok(SettingsCommand::SetRenderCadencePolicy(
            RenderCadencePolicy::MonitorAuto,
        )),
        value => Err(SettingsCommandParseError::InvalidValue {
            field: "render cadence",
            value: value.to_string(),
            expected: "monitor-auto",
        }),
    }
}

fn parse_theme_command(tokens: &[&str]) -> Result<SettingsCommand, SettingsCommandParseError> {
    if tokens.len() < 3 {
        return Err(SettingsCommandParseError::MissingArgument {
            command: "theme".to_string(),
            expected: "set <cuberpunk|aurora|monochrome>",
        });
    }
    if normalize_token(tokens[1]) != "set" {
        return Err(SettingsCommandParseError::InvalidValue {
            field: "theme",
            value: normalize_token(tokens[1]),
            expected: "set <cuberpunk|aurora|monochrome>",
        });
    }
    if tokens.len() > 3 {
        return Err(SettingsCommandParseError::UnexpectedTrailingInput {
            command: "theme set".to_string(),
            trailing: normalize_trailing_tokens(tokens, 3),
        });
    }

    let theme = match normalize_token(tokens[2]).as_str() {
        "cuberpunk" => ThemePreset::Cuberpunk,
        "aurora" => ThemePreset::Aurora,
        "monochrome" => ThemePreset::Monochrome,
        value => {
            return Err(SettingsCommandParseError::InvalidValue {
                field: "theme",
                value: value.to_string(),
                expected: "cuberpunk|aurora|monochrome",
            });
        }
    };

    Ok(SettingsCommand::SetTheme(theme))
}

fn parse_profile_command(tokens: &[&str]) -> Result<SettingsCommand, SettingsCommandParseError> {
    if tokens.len() < 2 {
        return Err(SettingsCommandParseError::MissingArgument {
            command: "profile".to_string(),
            expected: "balanced|throughput|stability",
        });
    }
    if tokens.len() > 2 {
        return Err(SettingsCommandParseError::UnexpectedTrailingInput {
            command: "profile".to_string(),
            trailing: normalize_trailing_tokens(tokens, 2),
        });
    }

    let profile = match normalize_token(tokens[1]).as_str() {
        "balanced" => RuntimeProfilePreset::Balanced,
        "throughput" => RuntimeProfilePreset::Throughput,
        "stability" => RuntimeProfilePreset::Stability,
        value => {
            return Err(SettingsCommandParseError::InvalidValue {
                field: "profile",
                value: value.to_string(),
                expected: "balanced|throughput|stability",
            });
        }
    };

    Ok(SettingsCommand::SetRuntimeProfile(profile))
}

fn parse_debug_command(tokens: &[&str]) -> Result<SettingsCommand, SettingsCommandParseError> {
    if tokens.len() < 2 {
        return Err(SettingsCommandParseError::MissingArgument {
            command: "debug".to_string(),
            expected: "on|off",
        });
    }
    if tokens.len() > 2 {
        return Err(SettingsCommandParseError::UnexpectedTrailingInput {
            command: "debug".to_string(),
            trailing: normalize_trailing_tokens(tokens, 2),
        });
    }

    let enabled = match normalize_token(tokens[1]).as_str() {
        "on" => true,
        "off" => false,
        value => {
            return Err(SettingsCommandParseError::InvalidValue {
                field: "debug",
                value: value.to_string(),
                expected: "on|off",
            });
        }
    };

    Ok(SettingsCommand::SetDebugMode(enabled))
}

fn normalize_token(token: &str) -> String {
    token.trim().to_ascii_lowercase()
}

fn normalize_trailing_tokens(tokens: &[&str], start: usize) -> String {
    tokens[start..]
        .iter()
        .map(|token| normalize_token(token))
        .collect::<Vec<_>>()
        .join(" ")
}

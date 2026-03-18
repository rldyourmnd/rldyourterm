// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use anyhow::{Context, Result, anyhow};
use rldyourterm_diagnostics::{DiagnosticsSink, RuntimeCommandSourceKind, SettingsApplySourceKind};
use rldyourterm_services::render_mode::{GpuFailureKind, RenderMode, RenderTransitionReason};
use rldyourterm_services::session::{SessionBoundary, SessionState, SessionTransitionOutcome};
use rldyourterm_settings::{SettingsApplyOutcome, SettingsService};
use rldyourterm_shell_integration::ShellTarget;
use rldyourterm_ui::{
    SINGLE_WINDOW_BASELINE, UiCommandOutcome, UiCommandReceipt, UiRuntime, UiRuntimeCommand,
};
use tracing::{info, warn};

use crate::runtime_shared::display::{render_mode_token, session_boundary_token as boundary_token};
use crate::{
    Cli, MVP_RESULT_LABEL, MVP_STEP_LABEL, MvpProfileArg, gpu_failure_kind_token,
    release_governance_token, shell_token, single_window_enforced_token, state_token, yes_no_token,
};

pub(crate) fn emit_settings_outcome(diagnostics: &DiagnosticsSink, outcome: &SettingsApplyOutcome) {
    if let Err(error) = diagnostics.emit_settings_apply_outcome(
        None,
        SettingsApplySourceKind::RuntimeBootstrap,
        outcome,
    ) {
        warn!(error = ?error, "failed to emit typed settings diagnostics");
    }
}

pub(crate) fn apply_palette_commands(
    diagnostics: &DiagnosticsSink,
    settings: &mut SettingsService,
    commands: &[String],
) {
    for (index, command) in commands.iter().enumerate() {
        let outcome = settings.apply_palette_command(command);
        if let Err(error) =
            diagnostics.emit_settings_palette_outcome(None, (index + 1) as u32, &outcome)
        {
            warn!(
                error = ?error,
                step = index + 1,
                input = %command,
                "failed to emit typed palette settings diagnostics"
            );
        }
    }
}

pub(crate) fn build_bootstrap_commands(cli: &Cli) -> Result<Vec<UiRuntimeCommand>> {
    let mut commands = vec![UiRuntimeCommand::AssertSingleWindow {
        requested: cli.window_count,
    }];
    commands.extend(default_profile_commands(cli.mvp_profile));

    for raw in &cli.mvp_command {
        commands.push(parse_mvp_command(raw)?);
    }

    if commands.len() == 1 {
        commands.push(UiRuntimeCommand::Tick);
    }

    if cli.mvp_repeat > 1 {
        let repeatable = commands
            .iter()
            .copied()
            .filter(|command| !matches!(command, UiRuntimeCommand::AssertSingleWindow { .. }))
            .collect::<Vec<_>>();

        for _ in 1..cli.mvp_repeat {
            commands.extend(repeatable.iter().copied());
        }
    }

    Ok(commands)
}

pub(crate) fn emit_command_receipts(diagnostics: &DiagnosticsSink, receipts: &[UiCommandReceipt]) {
    for (index, receipt) in receipts.iter().enumerate() {
        let command = command_token(receipt.command);
        info!(
            step = index + 1,
            command = %command,
            outcome = ?receipt.outcome,
            state = ?receipt.state,
            mode = ?receipt.render_mode,
            cadence_millihz = receipt.cadence_millihz,
            windows = receipt.window_count,
            "ui command processed"
        );
        if let Err(error) = diagnostics.emit_runtime_command_receipt(
            None,
            RuntimeCommandSourceKind::BootstrapHook,
            Some((index + 1) as u32),
            receipt,
        ) {
            warn!(
                error = ?error,
                step = index + 1,
                command = %command,
                "failed to emit typed runtime command receipt diagnostics"
            );
        }
    }
}

pub(crate) fn should_print_mvp_output(cli: &Cli) -> bool {
    cli.mvp_profile.is_some()
        || !cli.mvp_command.is_empty()
        || !cli.palette_command.is_empty()
        || cli.mvp_repeat > 1
}

pub(crate) fn print_mvp_output(
    cli: &Cli,
    receipts: &[UiCommandReceipt],
    ui: &UiRuntime,
    resolved_shell: ShellTarget,
) {
    let recoverable_observed = receipts.iter().any(|receipt| {
        matches!(
            receipt.outcome,
            UiCommandOutcome::SessionTransition(rldyourterm_services::session::SessionTransition {
                outcome: SessionTransitionOutcome::RecoverableBoundary { .. },
                ..
            })
        )
    });
    let cadence_resync_observed = receipts
        .iter()
        .any(|receipt| matches!(receipt.outcome, UiCommandOutcome::CadenceResynced { .. }));
    let gpu_retry_observed = receipts
        .iter()
        .any(|receipt| matches!(receipt.outcome, UiCommandOutcome::GpuRetryScheduled { .. }));
    let fallback_observed = receipts.iter().any(|receipt| {
        matches!(
            receipt.outcome,
            UiCommandOutcome::RenderModeTransition(
                rldyourterm_services::render_mode::RenderModeTransition {
                    reason: RenderTransitionReason::AutoGpuFallback { .. },
                    ..
                }
            )
        )
    });
    let running_step_observed = receipts
        .iter()
        .any(|receipt| receipt.state == SessionState::Running);

    for (index, receipt) in receipts.iter().enumerate() {
        println!(
            "{MVP_STEP_LABEL} index={} command={} state={} mode={} cadence_millihz={} windows={} single_window_required={} single_window_enforced={} outcome={:?}",
            index + 1,
            command_token(receipt.command),
            state_token(receipt.state),
            render_mode_token(receipt.render_mode),
            receipt.cadence_millihz,
            receipt.window_count,
            SINGLE_WINDOW_BASELINE,
            single_window_enforced_token(receipt.window_count),
            receipt.outcome
        );
    }

    println!(
        "{MVP_RESULT_LABEL} profile={} repeats={} commands={} state={} mode={} cadence_millihz={} windows={} shell={} single_window_required={} single_window_enforced={} release_governance={} recoverable_observed={} cadence_resync_observed={} gpu_retry_observed={} fallback_observed={} running_step_observed={}",
        cli.mvp_profile
            .map(MvpProfileArg::as_str)
            .unwrap_or("custom"),
        cli.mvp_repeat,
        receipts.len(),
        state_token(ui.state()),
        render_mode_token(ui.render_mode()),
        ui.cadence().refresh_rate_millihz,
        ui.window_count(),
        shell_token(resolved_shell),
        SINGLE_WINDOW_BASELINE,
        single_window_enforced_token(ui.window_count()),
        release_governance_token(ui.release_governance()),
        yes_no_token(recoverable_observed),
        yes_no_token(cadence_resync_observed),
        yes_no_token(gpu_retry_observed),
        yes_no_token(fallback_observed),
        yes_no_token(running_step_observed),
    );
}

fn default_profile_commands(profile: Option<MvpProfileArg>) -> Vec<UiRuntimeCommand> {
    let mut commands = match profile {
        Some(MvpProfileArg::Claude) => vec![
            UiRuntimeCommand::Tick,
            UiRuntimeCommand::SetRenderMode(RenderMode::Auto),
            UiRuntimeCommand::RecoverableBoundary(SessionBoundary::PtyRead),
            UiRuntimeCommand::Tick,
            UiRuntimeCommand::ResyncCadence {
                refresh_rate_millihz:
                    crate::runtime_shared::runtime_config::DEFAULT_REFRESH_RATE_MILLIHZ,
            },
        ],
        Some(MvpProfileArg::Codex) => vec![
            UiRuntimeCommand::Tick,
            UiRuntimeCommand::SetRenderMode(RenderMode::Auto),
            UiRuntimeCommand::GpuFailure {
                kind: GpuFailureKind::SurfaceError,
                observed_at_millis: 1_000,
            },
            UiRuntimeCommand::GpuFailure {
                kind: GpuFailureKind::SubmitError,
                observed_at_millis: 1_500,
            },
            UiRuntimeCommand::GpuFailure {
                kind: GpuFailureKind::SwapchainOutOfDate,
                observed_at_millis: 2_000,
            },
            UiRuntimeCommand::Tick,
        ],
        Some(MvpProfileArg::Gemini) => vec![
            UiRuntimeCommand::Tick,
            UiRuntimeCommand::SetRenderMode(RenderMode::Auto),
            UiRuntimeCommand::ResyncCadenceAfterTransfer {
                refresh_rate_millihz: crate::HIGH_REFRESH_RATE_MILLIHZ,
            },
            UiRuntimeCommand::ResyncCadenceAfterTransfer {
                refresh_rate_millihz:
                    crate::runtime_shared::runtime_config::DEFAULT_REFRESH_RATE_MILLIHZ,
            },
        ],
        None => Vec::new(),
    };

    if !commands.is_empty() {
        commands.push(UiRuntimeCommand::AssertSingleWindow {
            requested: SINGLE_WINDOW_BASELINE,
        });
    }

    commands
}

fn parse_mvp_command(raw: &str) -> Result<UiRuntimeCommand> {
    let normalized = raw.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(anyhow!("empty --mvp-command entry"));
    }

    match normalized.as_str() {
        "tick" => return Ok(UiRuntimeCommand::Tick),
        "stop" => return Ok(UiRuntimeCommand::RequestStop),
        "stopped" => return Ok(UiRuntimeCommand::MarkStopped),
        "gpu-frame-ok" => return Ok(UiRuntimeCommand::GpuFramePresented),
        "single-window" => {
            return Ok(UiRuntimeCommand::AssertSingleWindow {
                requested: SINGLE_WINDOW_BASELINE,
            });
        }
        _ => {}
    }

    if let Some(value) = normalized.strip_prefix("single-window:") {
        let requested = value
            .parse::<u8>()
            .context("invalid single-window command: expected single-window:<window-count>")?;
        return Ok(UiRuntimeCommand::AssertSingleWindow { requested });
    }
    if let Some(value) = normalized.strip_prefix("mode:") {
        return Ok(UiRuntimeCommand::SetRenderMode(parse_render_mode(value)?));
    }
    if let Some(value) = normalized.strip_prefix("cadence:") {
        let refresh_rate_millihz = value
            .parse::<u32>()
            .context("invalid cadence command: expected cadence:<refresh-rate-millihz>")?;
        return Ok(UiRuntimeCommand::ResyncCadence {
            refresh_rate_millihz,
        });
    }
    if let Some(value) = normalized.strip_prefix("transfer-cadence:") {
        let refresh_rate_millihz = value.parse::<u32>().context(
            "invalid transfer cadence command: expected transfer-cadence:<refresh-rate-millihz>",
        )?;
        return Ok(UiRuntimeCommand::ResyncCadenceAfterTransfer {
            refresh_rate_millihz,
        });
    }
    if let Some(value) = normalized.strip_prefix("gpu-failure:") {
        return parse_gpu_failure_command(value);
    }
    if let Some(value) = normalized.strip_prefix("recoverable:") {
        return Ok(UiRuntimeCommand::RecoverableBoundary(parse_boundary(
            value,
        )?));
    }
    if let Some(value) = normalized.strip_prefix("fatal:") {
        return Ok(UiRuntimeCommand::FatalBoundary(parse_boundary(value)?));
    }

    Err(anyhow!(
        "unsupported --mvp-command `{raw}`; supported forms: \
tick, stop, stopped, gpu-frame-ok, single-window[:N], mode:<cpu|gpu|auto>, \
cadence:<millihz>, transfer-cadence:<millihz>, gpu-failure:<kind>[:observed-ms], \
recoverable:<boundary>, fatal:<boundary>"
    ))
}

fn parse_gpu_failure_command(raw: &str) -> Result<UiRuntimeCommand> {
    let mut parts = raw.split(':');
    let kind_token = parts
        .next()
        .ok_or_else(|| anyhow!("gpu-failure command requires failure kind"))?;
    let observed_at_millis = match parts.next() {
        Some(token) => token
            .parse::<u64>()
            .context("invalid gpu-failure command: observed-ms must be an integer")?,
        None => 1_000,
    };
    if parts.next().is_some() {
        return Err(anyhow!(
            "invalid gpu-failure command: expected gpu-failure:<kind>[:observed-ms]"
        ));
    }

    let kind = match kind_token {
        "device-lost" => GpuFailureKind::DeviceLost,
        "out-of-memory" => GpuFailureKind::OutOfMemory,
        "surface-error" => GpuFailureKind::SurfaceError,
        "submit-error" => GpuFailureKind::SubmitError,
        "swapchain-out-of-date" => GpuFailureKind::SwapchainOutOfDate,
        "backend-unavailable" => GpuFailureKind::BackendUnavailable,
        _ => {
            return Err(anyhow!(
                "unsupported gpu failure kind `{kind_token}`; expected one of: \
device-lost, out-of-memory, surface-error, submit-error, swapchain-out-of-date, \
backend-unavailable"
            ));
        }
    };

    Ok(UiRuntimeCommand::GpuFailure {
        kind,
        observed_at_millis,
    })
}

fn parse_render_mode(token: &str) -> Result<RenderMode> {
    match token {
        "cpu" => Ok(RenderMode::Cpu),
        "gpu" => Ok(RenderMode::Gpu),
        "auto" => Ok(RenderMode::Auto),
        _ => Err(anyhow!(
            "unsupported mode token `{token}`; expected cpu|gpu|auto"
        )),
    }
}

fn parse_boundary(token: &str) -> Result<SessionBoundary> {
    let normalized = token.replace('_', "-");
    match normalized.as_str() {
        "startup-spawn" => Ok(SessionBoundary::StartupSpawn),
        "pty-read" => Ok(SessionBoundary::PtyRead),
        "pty-write" => Ok(SessionBoundary::PtyWrite),
        "pty-resize" => Ok(SessionBoundary::PtyResize),
        "pty-wait" => Ok(SessionBoundary::PtyWait),
        "pty-writer-acquire" => Ok(SessionBoundary::PtyWriterAcquire),
        "stop" => Ok(SessionBoundary::Stop),
        _ => Err(anyhow!(
            "unsupported boundary token `{token}`; expected one of: \
startup-spawn, pty-read, pty-write, pty-resize, pty-wait, pty-writer-acquire, stop"
        )),
    }
}

fn command_token(command: UiRuntimeCommand) -> String {
    match command {
        UiRuntimeCommand::Tick => "tick".to_string(),
        UiRuntimeCommand::RecoverableBoundary(boundary) => {
            format!("recoverable:{}", boundary_token(boundary))
        }
        UiRuntimeCommand::FatalBoundary(boundary) => format!("fatal:{}", boundary_token(boundary)),
        UiRuntimeCommand::RequestStop => "stop".to_string(),
        UiRuntimeCommand::MarkStopped => "stopped".to_string(),
        UiRuntimeCommand::SetRenderMode(mode) => format!("mode:{}", render_mode_token(mode)),
        UiRuntimeCommand::GpuFailure {
            kind,
            observed_at_millis,
        } => format!(
            "gpu-failure:{}:{}",
            gpu_failure_kind_token(kind),
            observed_at_millis
        ),
        UiRuntimeCommand::GpuFramePresented => "gpu-frame-ok".to_string(),
        UiRuntimeCommand::ResyncCadence {
            refresh_rate_millihz,
        } => format!("cadence:{refresh_rate_millihz}"),
        UiRuntimeCommand::ResyncCadenceAfterTransfer {
            refresh_rate_millihz,
        } => format!("transfer-cadence:{refresh_rate_millihz}"),
        UiRuntimeCommand::AssertSingleWindow { requested } => format!("single-window:{requested}"),
    }
}

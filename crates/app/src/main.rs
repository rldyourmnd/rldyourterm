use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use clap::{Parser, ValueEnum};
use rldyourterm_diagnostics::{DiagnosticsSink, EventKind};
use rldyourterm_foundation::api::clipboard::ClipboardAdapter;
use rldyourterm_foundation_platform::clipboard::PlatformClipboard;
use rldyourterm_render_cpu::CpuRenderer;
use rldyourterm_services::render_mode::{
    ActiveRenderPath, GpuFailureKind, RenderMode, RenderTransitionReason,
};
use rldyourterm_services::session::{SessionBoundary, SessionState};
use rldyourterm_settings::{
    SettingsApplyOutcome, SettingsCommand, SettingsPaletteApplyOutcome, SettingsService,
};
use rldyourterm_shell_integration::{
    ShellAvailability, ShellLaunchPlan, ShellResolution, ShellResolutionReason, ShellTarget,
    resolve_shell,
};
use rldyourterm_ui::{
    DEFAULT_SCROLLBACK_CAP, ReleaseGovernance, SINGLE_WINDOW_BASELINE, UiBootstrapConfig,
    UiBootstrapHooks, UiCommandReceipt, UiRuntime, UiRuntimeCommand,
};
use tracing::{info, warn};

const DEFAULT_REFRESH_RATE_MILLIHZ: u32 = 60_000;
const HIGH_REFRESH_RATE_MILLIHZ: u32 = 144_000;
const MVP_STEP_LABEL: &str = "MVP_STEP";
const MVP_RESULT_LABEL: &str = "MVP_RESULT";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
enum LogLevelArg {
    /// Info-level logging, clean output (default)
    #[default]
    Standard,
    /// Debug-level with module targets, file:line, thread names
    Debug,
    /// Maximum verbosity including wgpu/winit internals
    Trace,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ModeArg {
    Cpu,
    Gpu,
    Auto,
}

impl From<ModeArg> for RenderMode {
    fn from(value: ModeArg) -> Self {
        match value {
            ModeArg::Cpu => Self::Cpu,
            ModeArg::Gpu => Self::Gpu,
            ModeArg::Auto => Self::Auto,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ShellArg {
    Fish,
    Zsh,
}

impl From<ShellArg> for ShellTarget {
    fn from(value: ShellArg) -> Self {
        match value {
            ShellArg::Fish => Self::Fish,
            ShellArg::Zsh => Self::Zsh,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum MvpProfileArg {
    Claude,
    Codex,
    Gemini,
}

impl MvpProfileArg {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
        }
    }
}

#[derive(Debug, Parser)]
#[command(name = "rldyourterm")]
struct Cli {
    #[arg(long, value_enum, default_value = "auto")]
    mode: ModeArg,
    #[arg(long, value_enum, default_value = "fish")]
    shell: ShellArg,
    #[arg(long, default_value_t = SINGLE_WINDOW_BASELINE)]
    window_count: u8,
    #[arg(long, default_value_t = DEFAULT_REFRESH_RATE_MILLIHZ)]
    refresh_rate_millihz: u32,
    #[arg(long, value_enum)]
    mvp_profile: Option<MvpProfileArg>,
    #[arg(long = "mvp-command")]
    mvp_command: Vec<String>,
    #[arg(long = "palette-command")]
    palette_command: Vec<String>,
    #[arg(long, default_value_t = 1)]
    mvp_repeat: u16,
    #[arg(long, default_value_t = false)]
    tty: bool,
    #[arg(long, value_enum, default_value = "standard")]
    log_level: LogLevelArg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunOutcome {
    Harness,
    Interactive { exit_code: i32 },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.log_level);
    match run(cli)? {
        RunOutcome::Harness => Ok(()),
        RunOutcome::Interactive { exit_code } => std::process::exit(normalize_exit_code(exit_code)),
    }
}

fn init_tracing(level: LogLevelArg) {
    let default_filter = match level {
        LogLevelArg::Standard => "warn,rldyourterm=info".to_owned(),
        LogLevelArg::Debug => {
            "warn,rldyourterm=debug,wgpu_hal::gles::egl=off,sctk_adwaita=off".to_owned()
        }
        LogLevelArg::Trace => "trace".to_owned(),
    };
    let show_target = level != LogLevelArg::Standard;
    let show_thread = level != LogLevelArg::Standard;
    let show_loc = level != LogLevelArg::Standard;

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_filter));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(show_target)
        .with_thread_names(show_thread)
        .with_file(show_loc)
        .with_line_number(show_loc)
        .try_init();
}

fn run(cli: Cli) -> Result<RunOutcome> {
    let render_mode: RenderMode = cli.mode.into();
    let preferred_shell: ShellTarget = cli.shell.into();
    let selected_shell = resolve_startup_shell(preferred_shell)?;

    let diagnostics = DiagnosticsSink::default();
    diagnostics.emit_kind(EventKind::SessionStarted, "app bootstrap start");

    let mut settings = SettingsService::default();
    apply_palette_commands(&diagnostics, &mut settings, &cli.palette_command);
    let startup_settings = settings.apply(settings_command_for_mode(render_mode));
    emit_settings_outcome(&diagnostics, startup_settings);

    if harness_enabled(&cli) {
        let bootstrap_commands = build_bootstrap_commands(&cli)?;
        let hooks = UiBootstrapHooks::from_commands(bootstrap_commands);

        let (ui, command_receipts) = UiRuntime::bootstrap_with_hooks(
            UiBootstrapConfig {
                render_mode,
                refresh_rate_millihz: cli.refresh_rate_millihz,
                window_count: cli.window_count,
                scrollback_cap: DEFAULT_SCROLLBACK_CAP,
            },
            &hooks,
        )
        .context("failed to bootstrap UI runtime")?;
        emit_command_receipts(&diagnostics, &command_receipts);

        let post_hook_settings = settings.apply(settings_command_for_mode(ui.render_mode()));
        emit_settings_outcome(&diagnostics, post_hook_settings);

        let cpu_renderer = CpuRenderer::default();
        render_initial_frame(&ui, &cpu_renderer);
        emit_shell_fallback_if_needed(&diagnostics, selected_shell.reason);

        if ui.release_governance() == ReleaseGovernance::ManualOnly {
            diagnostics.emit_kind(EventKind::ResourceWarning, "manual-only release governance");
        }

        info!(
            mvp_profile = cli.mvp_profile.map(MvpProfileArg::as_str).unwrap_or("none"),
            mvp_commands = command_receipts.len(),
            mode = ?ui.render_mode(),
            state = ?ui.state(),
            shell = ?selected_shell.resolved,
            cadence_millihz = ui.cadence().refresh_rate_millihz,
            windows = ui.window_count(),
            single_window_required = SINGLE_WINDOW_BASELINE,
            single_window_enforced = ui.window_count() == SINGLE_WINDOW_BASELINE,
            release_governance = release_governance_token(ui.release_governance()),
            "startup flow completed"
        );

        if should_print_mvp_output(&cli) {
            print_mvp_output(&cli, &command_receipts, &ui, selected_shell.resolved);
        }

        diagnostics.emit_kind(EventKind::SessionEnded, "app bootstrap ready");
        return Ok(RunOutcome::Harness);
    }

    emit_shell_fallback_if_needed(&diagnostics, selected_shell.reason);
    let launch_plan = ShellLaunchPlan::from_resolution(selected_shell);
    let tty_runtime_config = pty_runtime::TtyRuntimeConfig {
        initial_mode: render_mode,
        refresh_rate_millihz: cli.refresh_rate_millihz,
        window_count: cli.window_count,
    };
    let exit_code = if cli.tty {
        pty_runtime::run_interactive_pty(
            &launch_plan.executable,
            &launch_plan.args,
            tty_runtime_config,
        )
        .context("failed to run TTY interactive runtime")?
    } else {
        let clipboard: Arc<dyn ClipboardAdapter> = Arc::new(PlatformClipboard::default());
        match gui_runtime::run_interactive_gui_pty(
            &launch_plan.executable,
            &launch_plan.args,
            render_mode,
            cli.refresh_rate_millihz,
            cli.window_count,
            clipboard,
        ) {
            Ok(code) => code,
            Err(error) => {
                warn!(
                    error = %error,
                    "GUI runtime unavailable; falling back to TTY interactive runtime"
                );
                diagnostics.emit_kind(
                    EventKind::ResourceWarning,
                    format!(
                        "GUI runtime unavailable; falling back to TTY interactive runtime: {error}"
                    ),
                );
                pty_runtime::run_interactive_pty(
                    &launch_plan.executable,
                    &launch_plan.args,
                    tty_runtime_config,
                )
                .context("failed to run TTY interactive runtime after GUI fallback")?
            }
        }
    };
    let normalized_exit_code = normalize_exit_code(exit_code);
    diagnostics.emit_kind(
        EventKind::SessionEnded,
        format!("interactive runtime exited with code={normalized_exit_code}"),
    );
    Ok(RunOutcome::Interactive {
        exit_code: normalized_exit_code,
    })
}

fn normalize_exit_code(exit_code: i32) -> i32 {
    if exit_code < 0 {
        warn!(
            raw_exit_code = exit_code,
            normalized_exit_code = 1,
            "interactive runtime returned negative or sentinel exit code; normalizing to failure"
        );
        1
    } else {
        exit_code
    }
}

fn resolve_startup_shell(preferred_shell: ShellTarget) -> Result<ShellResolution> {
    resolve_shell(preferred_shell, shell_availability())
        .map_err(|err| anyhow!("failed to resolve startup shell target: {err:?}"))
}

fn shell_availability() -> ShellAvailability {
    ShellAvailability {
        fish_available: shell_available_on_path("fish"),
        starship_available: shell_available_on_path("starship"),
        zsh_available: shell_available_on_path("zsh"),
    }
}

fn render_initial_frame(ui: &UiRuntime, cpu_renderer: &CpuRenderer) {
    match ui.active_render_path() {
        ActiveRenderPath::Cpu => {
            let placeholder = rldyourterm_core::state::TerminalState::new(120, 30, 1);
            let _ = cpu_renderer.render_full(&placeholder);
        }
        ActiveRenderPath::Gpu => {
            // GPU render requires an initialized backend (window + surface).
            // Harness runs without a window, so GPU cannot be initialized here.
            warn!(
                "GPU render path selected in harness but no window available; skipping initial frame"
            );
        }
    }
}

fn emit_shell_fallback_if_needed(diagnostics: &DiagnosticsSink, reason: ShellResolutionReason) {
    if matches!(
        reason,
        ShellResolutionReason::FishRequestedFallbackToZsh
            | ShellResolutionReason::AutoFallbackToZsh
    ) {
        warn!("fish baseline unavailable; continuing with zsh fallback");
        diagnostics.emit_kind(
            EventKind::ShellFallbackApplied,
            "fish baseline unavailable; zsh fallback selected",
        );
    }
}

fn build_bootstrap_commands(cli: &Cli) -> Result<Vec<UiRuntimeCommand>> {
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

fn default_profile_commands(profile: Option<MvpProfileArg>) -> Vec<UiRuntimeCommand> {
    let mut commands = match profile {
        Some(MvpProfileArg::Claude) => vec![
            UiRuntimeCommand::Tick,
            UiRuntimeCommand::SetRenderMode(RenderMode::Auto),
            UiRuntimeCommand::RecoverableBoundary(SessionBoundary::PtyRead),
            UiRuntimeCommand::Tick,
            UiRuntimeCommand::ResyncCadence {
                refresh_rate_millihz: DEFAULT_REFRESH_RATE_MILLIHZ,
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
                refresh_rate_millihz: HIGH_REFRESH_RATE_MILLIHZ,
            },
            UiRuntimeCommand::ResyncCadenceAfterTransfer {
                refresh_rate_millihz: DEFAULT_REFRESH_RATE_MILLIHZ,
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
        "surface-error" => GpuFailureKind::SurfaceError,
        "submit-error" => GpuFailureKind::SubmitError,
        "swapchain-out-of-date" => GpuFailureKind::SwapchainOutOfDate,
        _ => {
            return Err(anyhow!(
                "unsupported gpu failure kind `{kind_token}`; expected one of: \
device-lost, surface-error, submit-error, swapchain-out-of-date"
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

fn emit_command_receipts(diagnostics: &DiagnosticsSink, receipts: &[UiCommandReceipt]) {
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
        diagnostics.emit_kind(
            EventKind::SettingsApply,
            format!(
                "ui command step={} command={} state={} mode={} cadence={} windows={}",
                index + 1,
                command,
                state_token(receipt.state),
                render_mode_token(receipt.render_mode),
                receipt.cadence_millihz,
                receipt.window_count
            ),
        );

        match receipt.outcome {
            rldyourterm_ui::UiCommandOutcome::RenderModeTransition(transition) => {
                if matches!(
                    transition.reason,
                    RenderTransitionReason::AutoGpuFallback { .. }
                ) {
                    diagnostics.emit_kind(
                        EventKind::ResourceWarning,
                        format!(
                            "gpu auto-fallback applied step={} command={} mode={} state={}",
                            index + 1,
                            command,
                            render_mode_token(receipt.render_mode),
                            state_token(receipt.state),
                        ),
                    );
                }
            }
            rldyourterm_ui::UiCommandOutcome::GpuRetryScheduled {
                failure_kind,
                failure_streak,
                retry_budget_remaining,
            } => {
                diagnostics.emit_kind(
                    EventKind::ResourceWarning,
                    format!(
                        "gpu retry scheduled step={} command={} kind={} streak={} remaining={}",
                        index + 1,
                        command,
                        gpu_failure_kind_token(failure_kind),
                        failure_streak,
                        retry_budget_remaining,
                    ),
                );
            }
            _ => {}
        }
    }
}

fn emit_settings_outcome(diagnostics: &DiagnosticsSink, outcome: SettingsApplyOutcome) {
    match outcome {
        SettingsApplyOutcome::Applied { current, .. } => {
            diagnostics.emit_kind(
                EventKind::SettingsApply,
                format!(
                    "settings applied mode={} shell_target={:?} shell_auto_init={} cadence_policy={:?}",
                    render_mode_token(current.mode),
                    current.shell_target,
                    current.shell_auto_init,
                    current.render_cadence_policy
                ),
            );
        }
        SettingsApplyOutcome::Noop { .. } => {}
        SettingsApplyOutcome::Rejected { reason, .. } => {
            diagnostics.emit_kind(
                EventKind::ResourceWarning,
                format!("settings command rejected: {reason:?}"),
            );
        }
    }
}

fn apply_palette_commands(
    diagnostics: &DiagnosticsSink,
    settings: &mut SettingsService,
    commands: &[String],
) {
    for (index, command) in commands.iter().enumerate() {
        let outcome = settings.apply_palette_command(command);
        match outcome {
            SettingsPaletteApplyOutcome::Applied { input, current, .. } => {
                diagnostics.emit_kind(
                    EventKind::SettingsApply,
                    format!(
                        "palette command applied step={} input={} mode={} shell_target={:?} shell_auto_init={} cadence_policy={:?}",
                        index + 1,
                        input,
                        render_mode_token(current.mode),
                        current.shell_target,
                        current.shell_auto_init,
                        current.render_cadence_policy,
                    ),
                );
            }
            SettingsPaletteApplyOutcome::Noop { input, state, .. } => {
                diagnostics.emit_kind(
                    EventKind::SettingsApply,
                    format!(
                        "palette command noop step={} input={} mode={} shell_target={:?} shell_auto_init={} cadence_policy={:?}",
                        index + 1,
                        input,
                        render_mode_token(state.mode),
                        state.shell_target,
                        state.shell_auto_init,
                        state.render_cadence_policy,
                    ),
                );
            }
            SettingsPaletteApplyOutcome::Rejected { input, reason, .. } => {
                diagnostics.emit_kind(
                    EventKind::SettingsRejected,
                    format!(
                        "palette command rejected step={} input={} reason={:?}",
                        index + 1,
                        input,
                        reason
                    ),
                );
            }
        }
    }
}

fn harness_enabled(cli: &Cli) -> bool {
    cli.mvp_profile.is_some() || !cli.mvp_command.is_empty() || cli.mvp_repeat > 1
}

fn should_print_mvp_output(cli: &Cli) -> bool {
    cli.mvp_profile.is_some()
        || !cli.mvp_command.is_empty()
        || !cli.palette_command.is_empty()
        || cli.mvp_repeat > 1
}

fn print_mvp_output(
    cli: &Cli,
    receipts: &[UiCommandReceipt],
    ui: &UiRuntime,
    resolved_shell: ShellTarget,
) {
    let recoverable_observed = receipts.iter().any(|receipt| {
        matches!(
            receipt.outcome,
            rldyourterm_ui::UiCommandOutcome::SessionTransition(
                rldyourterm_services::session::SessionTransition {
                    outcome: rldyourterm_services::session::SessionTransitionOutcome::RecoverableBoundary { .. },
                    ..
                }
            )
        )
    });
    let cadence_resync_observed = receipts.iter().any(|receipt| {
        matches!(
            receipt.outcome,
            rldyourterm_ui::UiCommandOutcome::CadenceResynced { .. }
        )
    });
    let gpu_retry_observed = receipts.iter().any(|receipt| {
        matches!(
            receipt.outcome,
            rldyourterm_ui::UiCommandOutcome::GpuRetryScheduled { .. }
        )
    });
    let fallback_observed = receipts.iter().any(|receipt| {
        matches!(
            receipt.outcome,
            rldyourterm_ui::UiCommandOutcome::RenderModeTransition(
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

use shared::{render_mode_token, session_boundary_token as boundary_token};

fn gpu_failure_kind_token(kind: GpuFailureKind) -> &'static str {
    match kind {
        GpuFailureKind::DeviceLost => "device-lost",
        GpuFailureKind::SurfaceError => "surface-error",
        GpuFailureKind::SubmitError => "submit-error",
        GpuFailureKind::SwapchainOutOfDate => "swapchain-out-of-date",
    }
}

fn state_token(state: SessionState) -> &'static str {
    state.as_str()
}

fn shell_token(shell: ShellTarget) -> &'static str {
    match shell {
        ShellTarget::Fish => "fish",
        ShellTarget::Zsh => "zsh",
        ShellTarget::Auto => "auto",
    }
}

fn single_window_enforced_token(window_count: u8) -> &'static str {
    if window_count == SINGLE_WINDOW_BASELINE {
        "yes"
    } else {
        "no"
    }
}

fn release_governance_token(governance: ReleaseGovernance) -> &'static str {
    match governance {
        ReleaseGovernance::ManualOnly => "manual-only",
    }
}

fn yes_no_token(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn settings_command_for_mode(mode: RenderMode) -> SettingsCommand {
    SettingsCommand::SetMode(mode)
}

fn shell_available_on_path(name: &str) -> bool {
    let Some(path_value) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path_value).any(|path| {
        let shell = path.join(name);
        let shell_exe = path.join(format!("{name}.exe"));
        shell.is_file() || shell_exe.is_file()
    })
}

#[cfg(test)]
mod tests {
    use super::normalize_exit_code;

    #[test]
    fn normalizes_negative_exit_code_to_failure() {
        assert_eq!(normalize_exit_code(-1), 1);
        assert_eq!(normalize_exit_code(i32::MIN), 1);
    }

    #[test]
    fn preserves_non_negative_exit_codes() {
        assert_eq!(normalize_exit_code(0), 0);
        assert_eq!(normalize_exit_code(1), 1);
        assert_eq!(normalize_exit_code(42), 42);
    }
}

mod gui_runtime;
mod pty_runtime;
mod shared;

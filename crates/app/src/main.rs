use std::{io::IsTerminal, sync::Arc};

use crate::runtime_shared::runtime_config::DEFAULT_REFRESH_RATE_MILLIHZ;
use anyhow::{Context, Result, anyhow};
use clap::{Parser, ValueEnum};
use rldyourterm_diagnostics::{DiagnosticsSink, EventKind};
use rldyourterm_foundation::api::clipboard::ClipboardAdapter;
use rldyourterm_foundation_platform::clipboard::PlatformClipboard;
use rldyourterm_render_cpu::CpuRenderer;
use rldyourterm_services::render_mode::{ActiveRenderPath, GpuFailureKind, RenderMode};
use rldyourterm_services::session::SessionState;
use rldyourterm_services::terminal::TerminalState;
use rldyourterm_settings::{SettingsCommand, SettingsService};
use rldyourterm_shell_integration::{
    ShellAvailability, ShellLaunchPlan, ShellResolution, ShellResolutionReason, ShellTarget,
    resolve_shell,
};
use rldyourterm_ui::{
    DEFAULT_SCROLLBACK_CAP, DEFAULT_TERMINAL_COLS, DEFAULT_TERMINAL_ROWS, ReleaseGovernance,
    SINGLE_WINDOW_BASELINE, UiBootstrapConfig, UiBootstrapHooks, UiRuntime,
};
use tracing::{info, warn};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TtyStdioSnapshot {
    stdin_is_terminal: bool,
    stdout_is_terminal: bool,
}

impl TtyStdioSnapshot {
    fn capture() -> Self {
        Self {
            stdin_is_terminal: std::io::stdin().is_terminal(),
            stdout_is_terminal: std::io::stdout().is_terminal(),
        }
    }

    const fn interactive_ready(self) -> bool {
        self.stdin_is_terminal && self.stdout_is_terminal
    }
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
        LogLevelArg::Standard => {
            "warn,rldyourterm=info,wgpu_hal::gles::egl=off,sctk_adwaita=off".to_owned()
        }
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
    let refresh_rate_millihz = crate::runtime_shared::runtime_config::sanitize_refresh_rate_millihz(
        cli.refresh_rate_millihz,
    );

    let diagnostics = DiagnosticsSink::default();
    diagnostics.emit_kind(EventKind::SessionStarted, "app bootstrap start");

    let mut settings = SettingsService::default();
    app_harness::apply_palette_commands(&diagnostics, &mut settings, &cli.palette_command);
    let startup_settings = settings.apply(settings_command_for_mode(render_mode));
    app_harness::emit_settings_outcome(&diagnostics, startup_settings);

    if harness_enabled(&cli) {
        let bootstrap_commands = app_harness::build_bootstrap_commands(&cli)?;
        let hooks = UiBootstrapHooks::from_commands(bootstrap_commands);

        let (ui, command_receipts) = UiRuntime::bootstrap_with_hooks(
            UiBootstrapConfig {
                render_mode,
                refresh_rate_millihz,
                window_count: cli.window_count,
                scrollback_cap: DEFAULT_SCROLLBACK_CAP,
            },
            &hooks,
        )
        .context("failed to bootstrap UI runtime")?;
        app_harness::emit_command_receipts(&diagnostics, &command_receipts);

        let post_hook_settings = settings.apply(settings_command_for_mode(ui.render_mode()));
        app_harness::emit_settings_outcome(&diagnostics, post_hook_settings);

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

        if app_harness::should_print_mvp_output(&cli) {
            app_harness::print_mvp_output(&cli, &command_receipts, &ui, selected_shell.resolved);
        }

        diagnostics.emit_kind(EventKind::SessionEnded, "app bootstrap ready");
        return Ok(RunOutcome::Harness);
    }

    emit_shell_fallback_if_needed(&diagnostics, selected_shell.reason);
    let launch_plan = ShellLaunchPlan::from_resolution(selected_shell);
    let tty_stdio_snapshot = TtyStdioSnapshot::capture();
    let tty_runtime_config = pty_runtime::TtyRuntimeConfig {
        initial_mode: render_mode,
        refresh_rate_millihz,
        window_count: cli.window_count,
    };
    let exit_code = if cli.tty {
        if !tty_stdio_snapshot.interactive_ready() {
            return Err(anyhow!(tty_runtime_unavailable_reason(tty_stdio_snapshot)));
        }
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
            refresh_rate_millihz,
            cli.window_count,
            clipboard,
        ) {
            Ok(code) => code,
            Err(error) => {
                if !tty_stdio_snapshot.interactive_ready() {
                    return Err(error).context(format!(
                        "GUI runtime unavailable and TTY fallback is not possible: {}",
                        tty_runtime_unavailable_reason(tty_stdio_snapshot),
                    ));
                }
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

fn tty_runtime_unavailable_reason(snapshot: TtyStdioSnapshot) -> String {
    format!(
        "TTY interactive runtime requires terminal stdin/stdout (stdin_is_terminal={} stdout_is_terminal={})",
        yes_no_token(snapshot.stdin_is_terminal),
        yes_no_token(snapshot.stdout_is_terminal),
    )
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
            let placeholder = TerminalState::new(DEFAULT_TERMINAL_COLS, DEFAULT_TERMINAL_ROWS, 1);
            let _ = cpu_renderer.render_full(&placeholder);
        }
        ActiveRenderPath::Gpu => {
            // GPU render requires an initialized backend (window + surface).
            // Harness runs without a window, so GPU cannot be initialized here.
            info!(
                "GPU render path selected in harness without window; initial frame intentionally skipped"
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

fn harness_enabled(cli: &Cli) -> bool {
    cli.mvp_profile.is_some() || !cli.mvp_command.is_empty() || cli.mvp_repeat > 1
}

fn gpu_failure_kind_token(kind: GpuFailureKind) -> &'static str {
    match kind {
        GpuFailureKind::DeviceLost => "device-lost",
        GpuFailureKind::OutOfMemory => "out-of-memory",
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
        let candidate = path.join(name);
        is_executable_file(&candidate)
    })
}

fn is_executable_file(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match path.metadata() {
        Ok(meta) => meta.is_file() && (meta.permissions().mode() & 0o111 != 0),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{TtyStdioSnapshot, normalize_exit_code, tty_runtime_unavailable_reason};

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

    #[test]
    fn tty_reason_message_contains_terminal_capability_snapshot() {
        let reason = tty_runtime_unavailable_reason(TtyStdioSnapshot {
            stdin_is_terminal: false,
            stdout_is_terminal: true,
        });

        assert!(reason.contains("stdin_is_terminal=no"));
        assert!(reason.contains("stdout_is_terminal=yes"));
    }

    #[test]
    fn tty_snapshot_requires_both_streams_to_be_terminals() {
        assert!(
            TtyStdioSnapshot {
                stdin_is_terminal: true,
                stdout_is_terminal: true,
            }
            .interactive_ready()
        );
        assert!(
            !TtyStdioSnapshot {
                stdin_is_terminal: false,
                stdout_is_terminal: true,
            }
            .interactive_ready()
        );
        assert!(
            !TtyStdioSnapshot {
                stdin_is_terminal: true,
                stdout_is_terminal: false,
            }
            .interactive_ready()
        );
    }
}

mod app_harness;
mod gui_runtime;
mod gui_runtime_backend;
mod pty_runtime;
mod runtime_shared;

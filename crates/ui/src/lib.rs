use std::time::Duration;

use std::error::Error;
use std::fmt::{Display, Formatter};

use rldyourterm_services::error::ServiceError;
use rldyourterm_services::render_mode::{
    ActiveRenderPath, FallbackDecision, GpuFailureKind, RenderMode, RenderModeController,
    RenderModeTransition, RenderTransitionReason,
};
use rldyourterm_services::render_pacing::{
    CadenceResyncTrigger, RenderCadence, RenderPacingController,
};
use rldyourterm_services::session::{
    SessionBoundary, SessionController, SessionState, SessionTransition,
};
use rldyourterm_services::TerminalState;
use tracing::{info, warn};

pub const SINGLE_WINDOW_BASELINE: u8 = 1;
pub const DEFAULT_SCROLLBACK_CAP: usize = 50_000;
pub const DEFAULT_TERMINAL_WIDTH: u16 = 120;
pub const DEFAULT_TERMINAL_HEIGHT: u16 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseGovernance {
    ManualOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiBootstrapConfig {
    pub render_mode: RenderMode,
    pub refresh_rate_millihz: u32,
    pub window_count: u8,
    pub scrollback_cap: usize,
}

impl UiBootstrapConfig {
    pub fn single_window(render_mode: RenderMode, refresh_rate_millihz: u32) -> Self {
        Self {
            render_mode,
            refresh_rate_millihz,
            window_count: SINGLE_WINDOW_BASELINE,
            scrollback_cap: DEFAULT_SCROLLBACK_CAP,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiBootstrapError {
    UnsupportedWindowCount { requested: u8 },
    InvalidRefreshRate,
    InvalidScrollbackCap,
}

impl Display for UiBootstrapError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedWindowCount { requested } => write!(
                f,
                "unsupported window count: {requested}; v1.0 supports a single window only"
            ),
            Self::InvalidRefreshRate => {
                write!(
                    f,
                    "invalid monitor refresh rate: value must be greater than zero"
                )
            }
            Self::InvalidScrollbackCap => {
                write!(f, "invalid scrollback cap: value must be greater than zero")
            }
        }
    }
}

impl Error for UiBootstrapError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiRuntimeError {
    Bootstrap(UiBootstrapError),
    Service(ServiceError),
}

impl Display for UiRuntimeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bootstrap(err) => write!(f, "{err}"),
            Self::Service(err) => write!(f, "{err}"),
        }
    }
}

impl Error for UiRuntimeError {}

impl From<UiBootstrapError> for UiRuntimeError {
    fn from(value: UiBootstrapError) -> Self {
        Self::Bootstrap(value)
    }
}

impl From<ServiceError> for UiRuntimeError {
    fn from(value: ServiceError) -> Self {
        Self::Service(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiRuntimeCommand {
    Tick,
    RecoverableBoundary(SessionBoundary),
    FatalBoundary(SessionBoundary),
    RequestStop,
    MarkStopped,
    SetRenderMode(RenderMode),
    GpuFailure {
        kind: GpuFailureKind,
        observed_at_millis: u64,
    },
    GpuFramePresented,
    ResyncCadence {
        refresh_rate_millihz: u32,
    },
    ResyncCadenceAfterTransfer {
        refresh_rate_millihz: u32,
    },
    AssertSingleWindow {
        requested: u8,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UiBootstrapHooks {
    commands: Vec<UiRuntimeCommand>,
}

impl UiBootstrapHooks {
    pub fn from_commands<I>(commands: I) -> Self
    where
        I: IntoIterator<Item = UiRuntimeCommand>,
    {
        Self {
            commands: commands.into_iter().collect(),
        }
    }

    pub fn commands(&self) -> &[UiRuntimeCommand] {
        &self.commands
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiCommandOutcome {
    Noop,
    SessionTransition(SessionTransition),
    RenderModeTransition(RenderModeTransition),
    CadenceResynced {
        previous_refresh_rate_millihz: Option<u32>,
        current_refresh_rate_millihz: Option<u32>,
        generation: u64,
        schedule_invalidated: bool,
        monitor_transfer: bool,
    },
    GpuRetryScheduled {
        failure_kind: GpuFailureKind,
        failure_streak: u8,
        retry_budget_remaining: u8,
    },
    SingleWindowConfirmed {
        window_count: u8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiCommandReceipt {
    pub command: UiRuntimeCommand,
    pub outcome: UiCommandOutcome,
    pub state: SessionState,
    pub render_mode: RenderMode,
    pub cadence_millihz: u32,
    pub window_count: u8,
}

#[derive(Debug)]
pub struct UiRuntime {
    session: SessionController,
    render_mode: RenderModeController,
    pacing: RenderPacingController,
    terminal: TerminalState,
    window_count: u8,
    release_governance: ReleaseGovernance,
}

impl UiRuntime {
    pub fn bootstrap(config: UiBootstrapConfig) -> Result<Self, UiBootstrapError> {
        validate_single_window(config.window_count)?;
        if config.refresh_rate_millihz == 0 {
            return Err(UiBootstrapError::InvalidRefreshRate);
        }
        if config.scrollback_cap == 0 {
            return Err(UiBootstrapError::InvalidScrollbackCap);
        }

        let session = SessionController::new();
        let render_mode = RenderModeController::new(config.render_mode);
        let pacing = RenderPacingController::new(Some(config.refresh_rate_millihz));
        let cadence = pacing
            .cadence()
            .unwrap_or(RenderCadence::from_monitor(0))
            .refresh_rate_millihz;
        let terminal = TerminalState::new(
            DEFAULT_TERMINAL_WIDTH,
            DEFAULT_TERMINAL_HEIGHT,
            config.scrollback_cap,
        );

        info!(
            mode = ?render_mode.mode(),
            cadence_millihz = cadence,
            windows = config.window_count,
            "ui bootstrap initialized"
        );

        Ok(Self {
            session,
            render_mode,
            pacing,
            terminal,
            window_count: config.window_count,
            release_governance: ReleaseGovernance::ManualOnly,
        })
    }

    pub fn bootstrap_with_hooks(
        config: UiBootstrapConfig,
        hooks: &UiBootstrapHooks,
    ) -> Result<(Self, Vec<UiCommandReceipt>), UiRuntimeError> {
        let mut runtime = Self::bootstrap(config)?;
        let mut receipts = Vec::with_capacity(hooks.commands().len());

        for command in hooks.commands() {
            receipts.push(runtime.handle_command(*command)?);
        }

        Ok((runtime, receipts))
    }

    pub fn handle_command(
        &mut self,
        command: UiRuntimeCommand,
    ) -> Result<UiCommandReceipt, UiRuntimeError> {
        let outcome = match command {
            UiRuntimeCommand::Tick => match self.session.state() {
                SessionState::Starting | SessionState::Degraded => {
                    UiCommandOutcome::SessionTransition(self.session.mark_running()?)
                }
                _ => UiCommandOutcome::Noop,
            },
            UiRuntimeCommand::RecoverableBoundary(boundary) => UiCommandOutcome::SessionTransition(
                self.session.handle_recoverable_boundary(boundary)?,
            ),
            UiRuntimeCommand::FatalBoundary(boundary) => {
                UiCommandOutcome::SessionTransition(self.session.handle_fatal_boundary(boundary)?)
            }
            UiRuntimeCommand::RequestStop => match self.session.state() {
                SessionState::Stopped => UiCommandOutcome::Noop,
                _ => UiCommandOutcome::SessionTransition(self.session.request_stop()?),
            },
            UiRuntimeCommand::MarkStopped => match self.session.state() {
                SessionState::Stopping | SessionState::Stopped => {
                    UiCommandOutcome::SessionTransition(self.session.mark_stopped()?)
                }
                _ => UiCommandOutcome::Noop,
            },
            UiRuntimeCommand::SetRenderMode(mode) => {
                match self.render_mode.set_mode_with_transition(mode) {
                    Some(transition) => UiCommandOutcome::RenderModeTransition(transition),
                    None => UiCommandOutcome::Noop,
                }
            }
            UiRuntimeCommand::GpuFailure {
                kind,
                observed_at_millis,
            } => match self
                .render_mode
                .on_gpu_failure(kind, Duration::from_millis(observed_at_millis))
            {
                FallbackDecision::Noop => UiCommandOutcome::Noop,
                FallbackDecision::RetryGpu {
                    metadata,
                    retry_budget_remaining,
                } => {
                    warn!(
                        correlation_sequence = self.render_mode.transition_sequence(),
                        mode = ?self.render_mode.mode(),
                        active_path = ?self.render_mode.active_path(),
                        failure = ?metadata.failure_kind,
                        failure_observed_at_ms = metadata.observed_at.as_millis(),
                        failure_streak = metadata.failure_streak,
                        retry_budget = metadata.retry_budget,
                        retry_budget_remaining,
                        failure_window_ms = metadata.failure_window.as_millis(),
                        "ui runtime scheduled gpu retry"
                    );
                    UiCommandOutcome::GpuRetryScheduled {
                        failure_kind: metadata.failure_kind,
                        failure_streak: metadata.failure_streak,
                        retry_budget_remaining,
                    }
                }
                FallbackDecision::SwitchToCpu(transition) => {
                    let (
                        failure_kind,
                        failure_observed_at_ms,
                        failure_streak,
                        retry_budget,
                        failure_window_ms,
                    ) = match transition.reason {
                        RenderTransitionReason::AutoGpuFallback { metadata } => (
                            metadata.failure_kind,
                            metadata.observed_at.as_millis(),
                            metadata.failure_streak,
                            metadata.retry_budget,
                            metadata.failure_window.as_millis(),
                        ),
                        RenderTransitionReason::ExplicitModeSet => (
                            kind,
                            u128::from(observed_at_millis),
                            0,
                            self.render_mode.fallback_policy().retry_budget,
                            self.render_mode
                                .fallback_policy()
                                .failure_window
                                .as_millis(),
                        ),
                    };
                    warn!(
                        correlation_sequence = transition.sequence,
                        from_mode = ?transition.from_mode,
                        to_mode = ?transition.to_mode,
                        from = ?transition.from,
                        to = ?transition.to,
                        failure = ?failure_kind,
                        failure_observed_at_ms,
                        failure_streak,
                        retry_budget,
                        failure_window_ms,
                        "ui runtime applied gpu fallback transition"
                    );
                    UiCommandOutcome::RenderModeTransition(transition)
                }
            },
            UiRuntimeCommand::GpuFramePresented => {
                self.render_mode.on_gpu_frame_presented();
                UiCommandOutcome::Noop
            }
            UiRuntimeCommand::ResyncCadence {
                refresh_rate_millihz,
            } => {
                let sample = (refresh_rate_millihz != 0).then_some(refresh_rate_millihz);
                let resync = self.pacing.resync_from_monitor(sample);
                if !resync.schedule_invalidated {
                    UiCommandOutcome::Noop
                } else {
                    UiCommandOutcome::CadenceResynced {
                        previous_refresh_rate_millihz: resync
                            .previous
                            .map(|cadence| cadence.refresh_rate_millihz),
                        current_refresh_rate_millihz: resync
                            .current
                            .map(|cadence| cadence.refresh_rate_millihz),
                        generation: resync.generation,
                        schedule_invalidated: resync.schedule_invalidated,
                        monitor_transfer: matches!(
                            resync.trigger,
                            CadenceResyncTrigger::MonitorTransfer
                        ),
                    }
                }
            }
            UiRuntimeCommand::ResyncCadenceAfterTransfer {
                refresh_rate_millihz,
            } => {
                let sample = (refresh_rate_millihz != 0).then_some(refresh_rate_millihz);
                let resync = self.pacing.resync_after_monitor_transfer(sample);
                if !resync.schedule_invalidated {
                    UiCommandOutcome::Noop
                } else {
                    UiCommandOutcome::CadenceResynced {
                        previous_refresh_rate_millihz: resync
                            .previous
                            .map(|cadence| cadence.refresh_rate_millihz),
                        current_refresh_rate_millihz: resync
                            .current
                            .map(|cadence| cadence.refresh_rate_millihz),
                        generation: resync.generation,
                        schedule_invalidated: resync.schedule_invalidated,
                        monitor_transfer: matches!(
                            resync.trigger,
                            CadenceResyncTrigger::MonitorTransfer
                        ),
                    }
                }
            }
            UiRuntimeCommand::AssertSingleWindow { requested } => {
                validate_single_window(requested)?;
                UiCommandOutcome::SingleWindowConfirmed {
                    window_count: self.window_count,
                }
            }
        };

        Ok(UiCommandReceipt {
            command,
            outcome,
            state: self.session.state(),
            render_mode: self.render_mode.mode(),
            cadence_millihz: self
                .pacing
                .cadence()
                .map(|cadence| cadence.refresh_rate_millihz)
                .unwrap_or(0),
            window_count: self.window_count,
        })
    }

    pub fn tick(&mut self) {
        match self.handle_command(UiRuntimeCommand::Tick) {
            Ok(receipt) => {
                if matches!(receipt.outcome, UiCommandOutcome::SessionTransition(_)) {
                    info!("ui runtime transitioned to running state");
                }
            }
            Err(err) => {
                warn!(error = %err, "ui runtime failed to handle tick command");
            }
        }
    }

    pub fn state(&self) -> SessionState {
        self.session.state()
    }

    pub fn render_mode(&self) -> RenderMode {
        self.render_mode.mode()
    }

    pub fn active_render_path(&self) -> ActiveRenderPath {
        self.render_mode.active_path()
    }

    pub fn cadence(&self) -> RenderCadence {
        self.pacing
            .cadence()
            .unwrap_or(RenderCadence::from_monitor(0))
    }

    pub fn terminal(&self) -> &TerminalState {
        &self.terminal
    }

    pub fn window_count(&self) -> u8 {
        self.window_count
    }

    pub fn release_governance(&self) -> ReleaseGovernance {
        self.release_governance
    }
}

fn validate_single_window(requested: u8) -> Result<(), UiBootstrapError> {
    if requested != SINGLE_WINDOW_BASELINE {
        warn!(
            requested,
            supported = SINGLE_WINDOW_BASELINE,
            "single-window baseline violation"
        );
        return Err(UiBootstrapError::UnsupportedWindowCount { requested });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rldyourterm_services::session::SessionBoundary;

    fn test_config() -> UiBootstrapConfig {
        UiBootstrapConfig::single_window(RenderMode::Auto, 60_000)
    }

    #[test]
    fn bootstrap_hooks_apply_startup_commands() {
        let hooks = UiBootstrapHooks::from_commands([
            UiRuntimeCommand::AssertSingleWindow {
                requested: SINGLE_WINDOW_BASELINE,
            },
            UiRuntimeCommand::Tick,
        ]);

        let (runtime, receipts) =
            UiRuntime::bootstrap_with_hooks(test_config(), &hooks).expect("bootstrap with hooks");

        assert_eq!(runtime.state(), SessionState::Running);
        assert_eq!(receipts.len(), 2);
        assert!(matches!(
            receipts[0].outcome,
            UiCommandOutcome::SingleWindowConfirmed { window_count: 1 }
        ));
        assert!(matches!(
            receipts[1].outcome,
            UiCommandOutcome::SessionTransition(_)
        ));
    }

    #[test]
    fn assert_single_window_command_rejects_multi_window() {
        let mut runtime = UiRuntime::bootstrap(test_config()).expect("bootstrap");

        let err = runtime
            .handle_command(UiRuntimeCommand::AssertSingleWindow { requested: 2 })
            .expect_err("expected single-window check failure");

        assert_eq!(
            err,
            UiRuntimeError::Bootstrap(UiBootstrapError::UnsupportedWindowCount { requested: 2 })
        );
    }

    #[test]
    fn cadence_resync_command_updates_refresh_rate() {
        let mut runtime = UiRuntime::bootstrap(test_config()).expect("bootstrap");

        let receipt = runtime
            .handle_command(UiRuntimeCommand::ResyncCadence {
                refresh_rate_millihz: 144_000,
            })
            .expect("resync command");

        assert_eq!(runtime.cadence().refresh_rate_millihz, 144_000);
        assert!(matches!(
            receipt.outcome,
            UiCommandOutcome::CadenceResynced {
                previous_refresh_rate_millihz: Some(60_000),
                current_refresh_rate_millihz: Some(144_000),
                schedule_invalidated: true,
                monitor_transfer: false,
                ..
            }
        ));
    }

    #[test]
    fn cadence_transfer_resync_invalidates_even_on_same_refresh() {
        let mut runtime = UiRuntime::bootstrap(test_config()).expect("bootstrap");
        let receipt = runtime
            .handle_command(UiRuntimeCommand::ResyncCadenceAfterTransfer {
                refresh_rate_millihz: 60_000,
            })
            .expect("transfer resync");

        assert!(matches!(
            receipt.outcome,
            UiCommandOutcome::CadenceResynced {
                previous_refresh_rate_millihz: Some(60_000),
                current_refresh_rate_millihz: Some(60_000),
                schedule_invalidated: true,
                monitor_transfer: true,
                ..
            }
        ));
    }

    #[test]
    fn gpu_failure_commands_drive_auto_fallback_path() {
        let mut runtime = UiRuntime::bootstrap(test_config()).expect("bootstrap");
        assert_eq!(runtime.active_render_path(), ActiveRenderPath::Gpu);

        let first = runtime
            .handle_command(UiRuntimeCommand::GpuFailure {
                kind: GpuFailureKind::SurfaceError,
                observed_at_millis: 1_000,
            })
            .expect("first gpu failure");
        assert_eq!(runtime.active_render_path(), ActiveRenderPath::Gpu);
        assert!(matches!(
            first.outcome,
            UiCommandOutcome::GpuRetryScheduled {
                failure_kind: GpuFailureKind::SurfaceError,
                failure_streak: 1,
                retry_budget_remaining: 1
            }
        ));

        let second = runtime
            .handle_command(UiRuntimeCommand::GpuFailure {
                kind: GpuFailureKind::SubmitError,
                observed_at_millis: 1_500,
            })
            .expect("second gpu failure");
        assert_eq!(runtime.active_render_path(), ActiveRenderPath::Gpu);
        assert!(matches!(
            second.outcome,
            UiCommandOutcome::GpuRetryScheduled {
                failure_kind: GpuFailureKind::SubmitError,
                failure_streak: 2,
                retry_budget_remaining: 0
            }
        ));

        let third = runtime
            .handle_command(UiRuntimeCommand::GpuFailure {
                kind: GpuFailureKind::SwapchainOutOfDate,
                observed_at_millis: 2_000,
            })
            .expect("third gpu failure");
        assert_eq!(runtime.active_render_path(), ActiveRenderPath::Cpu);
        assert!(matches!(
            third.outcome,
            UiCommandOutcome::RenderModeTransition(RenderModeTransition {
                from: rldyourterm_services::render_mode::ActiveRenderPath::Gpu,
                to: rldyourterm_services::render_mode::ActiveRenderPath::Cpu,
                reason:
                    rldyourterm_services::render_mode::RenderTransitionReason::AutoGpuFallback { .. },
                ..
            })
        ));
    }

    #[test]
    fn forced_gpu_mode_does_not_auto_fallback_on_gpu_failure_command() {
        let config = UiBootstrapConfig::single_window(RenderMode::Gpu, 60_000);
        let mut runtime = UiRuntime::bootstrap(config).expect("bootstrap");
        assert_eq!(runtime.active_render_path(), ActiveRenderPath::Gpu);

        let receipt = runtime
            .handle_command(UiRuntimeCommand::GpuFailure {
                kind: GpuFailureKind::SurfaceError,
                observed_at_millis: 250,
            })
            .expect("forced gpu failure command");

        assert!(matches!(receipt.outcome, UiCommandOutcome::Noop));
        assert_eq!(runtime.active_render_path(), ActiveRenderPath::Gpu);
    }

    #[test]
    fn recoverable_boundary_then_tick_returns_running() {
        let mut runtime = UiRuntime::bootstrap(test_config()).expect("bootstrap");

        let degraded = runtime
            .handle_command(UiRuntimeCommand::RecoverableBoundary(
                SessionBoundary::PtyRead,
            ))
            .expect("recoverable boundary");
        assert_eq!(runtime.state(), SessionState::Degraded);
        assert!(matches!(
            degraded.outcome,
            UiCommandOutcome::SessionTransition(_)
        ));

        let resumed = runtime
            .handle_command(UiRuntimeCommand::Tick)
            .expect("tick after degrade");
        assert_eq!(runtime.state(), SessionState::Running);
        assert!(matches!(
            resumed.outcome,
            UiCommandOutcome::SessionTransition(_)
        ));
    }
}

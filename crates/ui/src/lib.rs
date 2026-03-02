use std::error::Error;
use std::fmt::{Display, Formatter};

use rldyourterm_core::state::TerminalState;
use rldyourterm_services::error::ServiceError;
use rldyourterm_services::render_mode::{RenderMode, RenderModeController, RenderModeTransition};
use rldyourterm_services::render_pacing::RenderCadence;
use rldyourterm_services::session::{
    SessionBoundary, SessionController, SessionState, SessionTransition,
};
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
    ResyncCadence { refresh_rate_millihz: u32 },
    AssertSingleWindow { requested: u8 },
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
        previous_refresh_rate_millihz: u32,
        current_refresh_rate_millihz: u32,
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
    cadence: RenderCadence,
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
        let cadence = RenderCadence::from_monitor(config.refresh_rate_millihz);
        let terminal = TerminalState::new(
            DEFAULT_TERMINAL_WIDTH,
            DEFAULT_TERMINAL_HEIGHT,
            config.scrollback_cap,
        );

        info!(
            mode = ?render_mode.mode(),
            cadence_millihz = cadence.refresh_rate_millihz,
            windows = config.window_count,
            "ui bootstrap initialized"
        );

        Ok(Self {
            session,
            render_mode,
            cadence,
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
            UiRuntimeCommand::ResyncCadence {
                refresh_rate_millihz,
            } => {
                if refresh_rate_millihz == 0 {
                    return Err(UiRuntimeError::Bootstrap(
                        UiBootstrapError::InvalidRefreshRate,
                    ));
                }
                if refresh_rate_millihz == self.cadence.refresh_rate_millihz {
                    UiCommandOutcome::Noop
                } else {
                    let previous = self.cadence.refresh_rate_millihz;
                    self.cadence = RenderCadence::from_monitor(refresh_rate_millihz);
                    UiCommandOutcome::CadenceResynced {
                        previous_refresh_rate_millihz: previous,
                        current_refresh_rate_millihz: self.cadence.refresh_rate_millihz,
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
            cadence_millihz: self.cadence.refresh_rate_millihz,
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

    pub fn cadence(&self) -> RenderCadence {
        self.cadence
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
                previous_refresh_rate_millihz: 60_000,
                current_refresh_rate_millihz: 144_000
            }
        ));
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

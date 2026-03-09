use std::error::Error;
use std::fmt::{Display, Formatter};

use rldyourterm_services::error::ServiceError;
use rldyourterm_services::render_mode::{
    ActiveRenderPath, GpuFailureKind, RenderMode, RenderModeController, RenderModeTransition,
};
use rldyourterm_services::render_pacing::{RenderCadence, RenderPacingController};
use rldyourterm_services::session::{
    SessionBoundary, SessionController, SessionState, SessionTransition,
};
use tracing::info;

pub const SINGLE_WINDOW_BASELINE: u8 = 1;
pub const DEFAULT_SCROLLBACK_CAP: usize = rldyourterm_services::terminal::DEFAULT_SCROLLBACK_CAP;
pub const DEFAULT_TERMINAL_COLS: u16 = 120;
pub const DEFAULT_TERMINAL_ROWS: u16 = 32;

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
    window_count: u8,
    release_governance: ReleaseGovernance,
}

impl UiRuntime {
    pub fn bootstrap(config: UiBootstrapConfig) -> Result<Self, UiBootstrapError> {
        commands::validate_single_window(config.window_count)?;
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

    pub fn window_count(&self) -> u8 {
        self.window_count
    }

    pub fn release_governance(&self) -> ReleaseGovernance {
        self.release_governance
    }
}

mod commands;
#[cfg(test)]
mod tests;

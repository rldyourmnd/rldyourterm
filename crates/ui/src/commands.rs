use std::time::Duration;

use rldyourterm_services::render_mode::{FallbackDecision, RenderTransitionReason};
use rldyourterm_services::render_pacing::CadenceResyncTrigger;
use rldyourterm_services::session::SessionState;
use tracing::warn;

use crate::{
    SINGLE_WINDOW_BASELINE, UiBootstrapError, UiCommandOutcome, UiCommandReceipt, UiRuntime,
    UiRuntimeCommand, UiRuntimeError,
};

pub(super) fn validate_single_window(requested: u8) -> Result<(), UiBootstrapError> {
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

impl UiRuntime {
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
            } => self.handle_gpu_failure_command(kind, observed_at_millis),
            UiRuntimeCommand::GpuFramePresented => {
                self.render_mode.on_gpu_frame_presented();
                UiCommandOutcome::Noop
            }
            UiRuntimeCommand::ResyncCadence {
                refresh_rate_millihz,
            } => self.handle_cadence_resync(refresh_rate_millihz),
            UiRuntimeCommand::ResyncCadenceAfterTransfer {
                refresh_rate_millihz,
            } => self.handle_transfer_cadence_resync(refresh_rate_millihz),
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

    fn handle_gpu_failure_command(
        &mut self,
        kind: rldyourterm_services::render_mode::GpuFailureKind,
        observed_at_millis: u64,
    ) -> UiCommandOutcome {
        match self
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
        }
    }

    fn handle_cadence_resync(&mut self, refresh_rate_millihz: u32) -> UiCommandOutcome {
        let sample = (refresh_rate_millihz != 0).then_some(refresh_rate_millihz);
        let resync = self.pacing.resync_from_monitor(sample);
        if !resync.schedule_invalidated {
            return UiCommandOutcome::Noop;
        }

        UiCommandOutcome::CadenceResynced {
            previous_refresh_rate_millihz: resync
                .previous
                .map(|cadence| cadence.refresh_rate_millihz),
            current_refresh_rate_millihz: resync
                .current
                .map(|cadence| cadence.refresh_rate_millihz),
            generation: resync.generation,
            schedule_invalidated: resync.schedule_invalidated,
            monitor_transfer: matches!(resync.trigger, CadenceResyncTrigger::MonitorTransfer),
        }
    }

    fn handle_transfer_cadence_resync(&mut self, refresh_rate_millihz: u32) -> UiCommandOutcome {
        let sample = (refresh_rate_millihz != 0).then_some(refresh_rate_millihz);
        let resync = self.pacing.resync_after_monitor_transfer(sample);
        debug_assert!(
            resync.schedule_invalidated,
            "monitor transfer resync must always invalidate schedule"
        );
        UiCommandOutcome::CadenceResynced {
            previous_refresh_rate_millihz: resync
                .previous
                .map(|cadence| cadence.refresh_rate_millihz),
            current_refresh_rate_millihz: resync
                .current
                .map(|cadence| cadence.refresh_rate_millihz),
            generation: resync.generation,
            schedule_invalidated: resync.schedule_invalidated,
            monitor_transfer: matches!(resync.trigger, CadenceResyncTrigger::MonitorTransfer),
        }
    }
}

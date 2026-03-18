// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use rldyourterm_diagnostics::{CorrelationId, DiagnosticsSink, Event, EventKind};
use rldyourterm_services::render_mode::{ActiveRenderPath, GpuFailureKind, RenderMode};
use rldyourterm_ui::{UiCommandOutcome, UiCommandReceipt, UiRuntime, UiRuntimeCommand};
use winit::event_loop::ControlFlow;

pub(crate) const DEFERRED_GPU_INIT_RETRY_BUDGET: u8 = 3;
const DEFERRED_GPU_INIT_MIN_BACKOFF: Duration = Duration::from_millis(50);
const DEFERRED_GPU_INIT_MAX_BACKOFF: Duration = Duration::from_millis(400);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GpuFailureHandling {
    RetryScheduled {
        failure_streak: u8,
        retry_budget_remaining: u8,
    },
    FallbackToCpu {
        transition_sequence: u64,
    },
    FatalForcedGpu,
    Ignored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RenderWaitPolicy {
    EventDriven,
    CadenceTimed(Duration),
}

impl RenderWaitPolicy {
    pub(crate) fn control_flow(self, now: Instant) -> ControlFlow {
        match self {
            Self::EventDriven => ControlFlow::Wait,
            Self::CadenceTimed(interval) => ControlFlow::WaitUntil(now + interval),
        }
    }
}

pub(crate) fn dispatch_gpu_failure_command(
    ui_runtime: &mut UiRuntime,
    failure_kind: GpuFailureKind,
    observed_at_millis: u64,
) -> Result<(UiCommandReceipt, GpuFailureHandling)> {
    let receipt = ui_runtime
        .handle_command(UiRuntimeCommand::GpuFailure {
            kind: failure_kind,
            observed_at_millis,
        })
        .context("failed to dispatch UiRuntimeCommand::GpuFailure")?;

    match receipt.outcome {
        UiCommandOutcome::GpuRetryScheduled {
            failure_streak,
            retry_budget_remaining,
            ..
        } => Ok((
            receipt,
            GpuFailureHandling::RetryScheduled {
                failure_streak,
                retry_budget_remaining,
            },
        )),
        UiCommandOutcome::RenderModeTransition(transition) => Ok((
            receipt,
            GpuFailureHandling::FallbackToCpu {
                transition_sequence: transition.sequence,
            },
        )),
        UiCommandOutcome::Noop
            if ui_runtime.render_mode() == RenderMode::Gpu
                && ui_runtime.active_render_path() == ActiveRenderPath::Gpu =>
        {
            Ok((receipt, GpuFailureHandling::FatalForcedGpu))
        }
        UiCommandOutcome::Noop => Ok((receipt, GpuFailureHandling::Ignored)),
        outcome @ (UiCommandOutcome::SessionTransition(_)
        | UiCommandOutcome::CadenceResynced { .. }
        | UiCommandOutcome::SingleWindowConfirmed { .. }) => Err(anyhow!(
            "unexpected UI outcome for GPU failure command: {outcome:?}"
        )),
    }
}

pub(crate) fn render_wait_policy(
    gpu_lane_ready: bool,
    redraw_pending: bool,
    cadence_interval: Option<Duration>,
) -> RenderWaitPolicy {
    if gpu_lane_ready {
        RenderWaitPolicy::EventDriven
    } else if redraw_pending {
        cadence_interval
            .map(RenderWaitPolicy::CadenceTimed)
            .unwrap_or(RenderWaitPolicy::EventDriven)
    } else {
        RenderWaitPolicy::EventDriven
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DeferredGpuInitState {
    pending: bool,
    failures: u8,
    next_retry_at: Option<Instant>,
}

impl DeferredGpuInitState {
    pub(crate) fn new(pending: bool) -> Self {
        Self {
            pending,
            failures: 0,
            next_retry_at: None,
        }
    }

    pub(crate) fn is_pending(&self) -> bool {
        self.pending
    }

    pub(crate) fn begin_attempt(&mut self) -> u8 {
        self.next_retry_at = None;
        self.next_attempt()
    }

    pub(crate) fn next_attempt(&self) -> u8 {
        self.failures.saturating_add(1)
    }

    pub(crate) fn retry_deadline(&self) -> Option<Instant> {
        self.next_retry_at
    }

    pub(crate) fn clear(&mut self) {
        self.pending = false;
        self.failures = 0;
        self.next_retry_at = None;
    }

    pub(crate) fn mark_ready(&mut self) {
        self.pending = false;
        self.failures = 0;
        self.next_retry_at = None;
    }

    pub(crate) fn schedule_retry(&mut self, attempt: u8, backoff: Duration) {
        self.pending = true;
        self.failures = attempt;
        self.next_retry_at = Some(Instant::now() + backoff);
    }

    pub(crate) fn record_failure_attempt(&mut self, attempt: u8) {
        self.failures = attempt;
    }

    pub(crate) fn mark_exhausted(&mut self, attempt: u8) {
        self.pending = false;
        self.failures = attempt;
        self.next_retry_at = None;
    }

    pub(crate) fn sync_with_target_path(
        &mut self,
        target_path: ActiveRenderPath,
        gpu_initialized: bool,
    ) {
        if target_path == ActiveRenderPath::Cpu {
            self.clear();
            return;
        }

        self.pending = !gpu_initialized;
        if gpu_initialized {
            self.failures = 0;
            self.next_retry_at = None;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackendSyncAction {
    None,
    ReleaseGpuBackend,
}

#[derive(Debug, Clone)]
pub(crate) struct RenderBackendCoordinator {
    deferred_gpu_init: DeferredGpuInitState,
    render_attempt_sequence: u64,
    gpu_failure_sequence: u64,
}

impl RenderBackendCoordinator {
    pub(crate) fn new(initial_mode: RenderMode) -> Self {
        Self {
            deferred_gpu_init: DeferredGpuInitState::new(initial_mode != RenderMode::Cpu),
            render_attempt_sequence: 0,
            gpu_failure_sequence: 0,
        }
    }

    pub(crate) fn begin_render_attempt(&mut self) -> u64 {
        self.render_attempt_sequence = self.render_attempt_sequence.saturating_add(1);
        self.render_attempt_sequence
    }

    pub(crate) fn current_render_attempt_sequence(&self) -> u64 {
        self.render_attempt_sequence
    }

    pub(crate) fn next_gpu_failure_sequence(&mut self) -> u64 {
        self.gpu_failure_sequence = self.gpu_failure_sequence.saturating_add(1);
        self.gpu_failure_sequence
    }

    pub(crate) fn deferred_gpu_init_pending(&self) -> bool {
        self.deferred_gpu_init.is_pending()
    }

    pub(crate) fn deferred_retry_deadline(&self) -> Option<Instant> {
        self.deferred_gpu_init.retry_deadline()
    }

    pub(crate) fn begin_deferred_attempt(&mut self) -> u8 {
        self.deferred_gpu_init.begin_attempt()
    }

    pub(crate) fn mark_deferred_ready(&mut self) {
        self.deferred_gpu_init.mark_ready();
    }

    pub(crate) fn record_deferred_failure_attempt(&mut self, attempt: u8) {
        self.deferred_gpu_init.record_failure_attempt(attempt);
    }

    pub(crate) fn schedule_deferred_retry(&mut self, attempt: u8, backoff: Duration) {
        self.deferred_gpu_init.schedule_retry(attempt, backoff);
    }

    pub(crate) fn mark_deferred_exhausted(&mut self, attempt: u8) {
        self.deferred_gpu_init.mark_exhausted(attempt);
    }

    pub(crate) fn clear_deferred_gpu_init(&mut self) {
        self.deferred_gpu_init.clear();
    }

    pub(crate) fn sync_with_target_path(
        &mut self,
        target_path: ActiveRenderPath,
        gpu_initialized: bool,
    ) -> BackendSyncAction {
        if target_path == ActiveRenderPath::Cpu {
            self.deferred_gpu_init.clear();
            if gpu_initialized {
                return BackendSyncAction::ReleaseGpuBackend;
            }
            return BackendSyncAction::None;
        }

        self.deferred_gpu_init
            .sync_with_target_path(target_path, gpu_initialized);
        BackendSyncAction::None
    }

    pub(crate) fn is_gpu_lane_ready(
        &self,
        target_path: ActiveRenderPath,
        gpu_initialized: bool,
    ) -> bool {
        target_path == ActiveRenderPath::Gpu && gpu_initialized
    }

    pub(crate) fn wait_policy(
        &self,
        target_path: ActiveRenderPath,
        gpu_initialized: bool,
        redraw_pending: bool,
        cadence_interval: Option<Duration>,
    ) -> RenderWaitPolicy {
        render_wait_policy(
            self.is_gpu_lane_ready(target_path, gpu_initialized),
            redraw_pending,
            cadence_interval,
        )
    }
}

fn gpu_auto_fallback_correlation_id(
    transition_sequence: u64,
    gpu_failure_sequence: u64,
) -> CorrelationId {
    CorrelationId::new(format!(
        "gpu-auto-fallback-transition-{transition_sequence}-failure-{gpu_failure_sequence}"
    ))
}

pub(crate) fn emit_gpu_auto_fallback_observability(
    diagnostics: &DiagnosticsSink,
    transition_sequence: u64,
    gpu_failure_sequence: u64,
    render_attempt_sequence: u64,
    failure_kind: GpuFailureKind,
    observed_at_millis: u64,
) -> (Event, String) {
    let correlation_id =
        gpu_auto_fallback_correlation_id(transition_sequence, gpu_failure_sequence);
    let diagnostics_message = format!(
        "gpu auto-fallback applied transition-seq={transition_sequence} failure-seq={gpu_failure_sequence} render-attempt-seq={render_attempt_sequence} failure-kind={failure_kind:?} observed-ms={observed_at_millis}"
    );
    let event = diagnostics
        .with_correlation(correlation_id.clone())
        .emit_kind(EventKind::RenderModeTransition, diagnostics_message);
    let notice = format!(
        "[runtime] gpu auto-fallback transition-seq={transition_sequence} failure-seq={gpu_failure_sequence} render-attempt-seq={render_attempt_sequence} failure={failure_kind:?} observed-ms={observed_at_millis} correlation-id={}",
        correlation_id.as_str()
    );
    (event, notice)
}

pub(crate) fn deferred_gpu_init_backoff(attempt: u8) -> Duration {
    let exponent = attempt.saturating_sub(1).min(3);
    let multiplier = 1_u32 << exponent;
    DEFERRED_GPU_INIT_MIN_BACKOFF
        .saturating_mul(multiplier)
        .min(DEFERRED_GPU_INIT_MAX_BACKOFF)
}

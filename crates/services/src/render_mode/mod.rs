// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use serde::{Deserialize, Serialize};
use std::time::Duration;

pub use rldyourterm_core::RenderMode;
use tracing::{info, warn};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GpuFailureKind {
    DeviceLost,
    OutOfMemory,
    SurfaceError,
    SubmitError,
    SwapchainOutOfDate,
    BackendUnavailable,
}

impl GpuFailureKind {
    #[must_use]
    pub const fn is_immediate_fallback(self) -> bool {
        matches!(
            self,
            Self::DeviceLost | Self::OutOfMemory | Self::BackendUnavailable
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActiveRenderPath {
    Cpu,
    Gpu,
}

impl ActiveRenderPath {
    fn from_mode(mode: RenderMode) -> Self {
        match mode {
            RenderMode::Cpu => Self::Cpu,
            RenderMode::Gpu | RenderMode::Auto => Self::Gpu,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoFallbackPolicy {
    pub retry_budget: u8,
    pub failure_window: Duration,
}

impl Default for AutoFallbackPolicy {
    fn default() -> Self {
        Self {
            retry_budget: 2,
            failure_window: Duration::from_secs(3),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoFallbackMetadata {
    pub failure_kind: GpuFailureKind,
    pub observed_at: Duration,
    pub failure_streak: u8,
    pub retry_budget: u8,
    pub failure_window: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "kebab-case")]
pub enum RenderTransitionReason {
    ExplicitModeSet,
    AutoGpuFallback { metadata: AutoFallbackMetadata },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderModeTransition {
    pub sequence: u64,
    pub from_mode: RenderMode,
    pub to_mode: RenderMode,
    pub from: ActiveRenderPath,
    pub to: ActiveRenderPath,
    pub reason: RenderTransitionReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackDecision {
    Noop,
    RetryGpu {
        metadata: AutoFallbackMetadata,
        retry_budget_remaining: u8,
    },
    SwitchToCpu(RenderModeTransition),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderModeController {
    mode: RenderMode,
    active_path: ActiveRenderPath,
    policy: AutoFallbackPolicy,
    failure_streak: u8,
    failure_window_start: Option<Duration>,
    transition_sequence: u64,
}

impl Default for RenderModeController {
    fn default() -> Self {
        Self::new(RenderMode::Auto)
    }
}

impl RenderModeController {
    pub fn new(mode: RenderMode) -> Self {
        Self {
            mode,
            active_path: ActiveRenderPath::from_mode(mode),
            policy: AutoFallbackPolicy::default(),
            failure_streak: 0,
            failure_window_start: None,
            transition_sequence: 0,
        }
    }

    pub fn mode(&self) -> RenderMode {
        self.mode
    }

    pub fn active_path(&self) -> ActiveRenderPath {
        self.active_path
    }

    pub fn fallback_policy(&self) -> AutoFallbackPolicy {
        self.policy
    }

    pub fn set_fallback_policy(&mut self, policy: AutoFallbackPolicy) {
        self.policy = policy;
        self.reset_failure_window();
    }

    pub fn set_mode(&mut self, mode: RenderMode) {
        let _ = self.set_mode_with_transition(mode);
    }

    pub fn set_mode_with_transition(&mut self, mode: RenderMode) -> Option<RenderModeTransition> {
        let previous_mode = self.mode;
        let previous_path = self.active_path;
        self.mode = mode;
        self.reset_failure_window();

        let next_path = ActiveRenderPath::from_mode(mode);
        let mode_changed = previous_mode != mode;
        let path_changed = previous_path != next_path;
        if !mode_changed && !path_changed {
            return None;
        }

        self.active_path = next_path;
        let transition = self.next_transition(
            previous_mode,
            mode,
            previous_path,
            next_path,
            RenderTransitionReason::ExplicitModeSet,
        );
        info!(
            sequence = transition.sequence,
            from_mode = ?transition.from_mode,
            to_mode = ?transition.to_mode,
            from = ?transition.from,
            to = ?transition.to,
            "render mode transition",
        );
        Some(transition)
    }

    pub fn on_gpu_failure(
        &mut self,
        failure_kind: GpuFailureKind,
        observed_at: Duration,
    ) -> FallbackDecision {
        if self.mode != RenderMode::Auto || self.active_path != ActiveRenderPath::Gpu {
            return FallbackDecision::Noop;
        }

        let failure_streak = self.record_failure(observed_at);
        if failure_kind.is_immediate_fallback()
            || self.retry_budget_exhausted(failure_streak, self.policy.retry_budget)
        {
            return self.switch_to_cpu_from_auto(failure_kind, observed_at, failure_streak);
        }

        let metadata = self.fallback_metadata(failure_kind, observed_at, failure_streak);
        FallbackDecision::RetryGpu {
            metadata,
            retry_budget_remaining: metadata.retry_budget.saturating_sub(failure_streak),
        }
    }

    pub fn on_gpu_frame_presented(&mut self) {
        if self.mode == RenderMode::Auto && self.active_path == ActiveRenderPath::Gpu {
            self.reset_failure_window();
        }
    }

    pub fn transition_sequence(&self) -> u64 {
        self.transition_sequence
    }

    fn record_failure(&mut self, observed_at: Duration) -> u8 {
        let within_window = self
            .failure_window_start
            .map(|start| observed_at.saturating_sub(start) <= self.policy.failure_window)
            .unwrap_or(false);

        if within_window {
            self.failure_streak = self.failure_streak.saturating_add(1);
        } else {
            self.failure_window_start = Some(observed_at);
            self.failure_streak = 1;
        }
        self.failure_streak
    }

    fn reset_failure_window(&mut self) {
        self.failure_streak = 0;
        self.failure_window_start = None;
    }

    fn retry_budget_exhausted(&self, failure_streak: u8, retry_budget: u8) -> bool {
        failure_streak > retry_budget || (failure_streak == u8::MAX && retry_budget == u8::MAX)
    }

    fn switch_to_cpu_from_auto(
        &mut self,
        failure_kind: GpuFailureKind,
        observed_at: Duration,
        failure_streak: u8,
    ) -> FallbackDecision {
        let fallback_mode = self.mode;
        let from = self.active_path;
        self.active_path = ActiveRenderPath::Cpu;
        self.reset_failure_window();
        let metadata = self.fallback_metadata(failure_kind, observed_at, failure_streak);

        let transition = self.next_transition(
            fallback_mode,
            fallback_mode,
            from,
            ActiveRenderPath::Cpu,
            RenderTransitionReason::AutoGpuFallback { metadata },
        );
        warn!(
            sequence = transition.sequence,
            from_mode = ?transition.from_mode,
            to_mode = ?transition.to_mode,
            from = ?transition.from,
            to = ?transition.to,
            failure = ?metadata.failure_kind,
            failure_observed_at_ms = metadata.observed_at.as_millis(),
            failure_streak = metadata.failure_streak,
            retry_budget = metadata.retry_budget,
            failure_window_ms = metadata.failure_window.as_millis(),
            "auto mode fallback from gpu to cpu",
        );

        FallbackDecision::SwitchToCpu(transition)
    }

    fn fallback_metadata(
        &self,
        failure_kind: GpuFailureKind,
        observed_at: Duration,
        failure_streak: u8,
    ) -> AutoFallbackMetadata {
        AutoFallbackMetadata {
            failure_kind,
            observed_at,
            failure_streak,
            retry_budget: self.policy.retry_budget,
            failure_window: self.policy.failure_window,
        }
    }

    fn next_transition(
        &mut self,
        from_mode: RenderMode,
        to_mode: RenderMode,
        from: ActiveRenderPath,
        to: ActiveRenderPath,
        reason: RenderTransitionReason,
    ) -> RenderModeTransition {
        self.transition_sequence = self.transition_sequence.saturating_add(1);
        RenderModeTransition {
            sequence: self.transition_sequence,
            from_mode,
            to_mode,
            from,
            to,
            reason,
        }
    }
}

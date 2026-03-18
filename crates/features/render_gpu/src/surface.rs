// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use std::error::Error;
use std::fmt;

pub const DEFAULT_SURFACE_RETRY_BUDGET: u8 = 3;
pub const DEFAULT_SURFACE_RECONFIGURE_RETRY_BUDGET: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceErrorCategory {
    Retryable,
    ReconfigureRequired,
    DegradeRequired,
}

impl SurfaceErrorCategory {
    #[must_use]
    pub const fn default_action(self) -> SurfaceRecoveryAction {
        match self {
            Self::Retryable => SurfaceRecoveryAction::RetryAcquire,
            Self::ReconfigureRequired => SurfaceRecoveryAction::ReconfigureSurface,
            Self::DegradeRequired => SurfaceRecoveryAction::DegradeToCpu,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceRecoveryAction {
    RetryAcquire,
    ReconfigureSurface,
    DegradeToCpu,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceErrorDecision {
    pub source: wgpu::SurfaceError,
    pub category: SurfaceErrorCategory,
    pub action: SurfaceRecoveryAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceRecoveryPolicy {
    acquire_retry_budget: u8,
    reconfigure_retry_budget: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SurfaceRuntimeState {
    consecutive_acquire_failures: u8,
    consecutive_reconfigure_failures: u8,
}

impl SurfaceRuntimeState {
    #[must_use]
    pub const fn new(
        consecutive_acquire_failures: u8,
        consecutive_reconfigure_failures: u8,
    ) -> Self {
        Self {
            consecutive_acquire_failures,
            consecutive_reconfigure_failures,
        }
    }

    #[must_use]
    pub const fn consecutive_acquire_failures(self) -> u8 {
        self.consecutive_acquire_failures
    }

    #[must_use]
    pub const fn consecutive_reconfigure_failures(self) -> u8 {
        self.consecutive_reconfigure_failures
    }

    fn clear_acquire_failures(&mut self) {
        self.consecutive_acquire_failures = 0;
    }

    fn clear_reconfigure_failures(&mut self) {
        self.consecutive_reconfigure_failures = 0;
    }

    fn reset_failures(&mut self) {
        self.clear_acquire_failures();
        self.clear_reconfigure_failures();
    }

    fn mark_retry_acquire(&mut self) {
        self.consecutive_acquire_failures = self.consecutive_acquire_failures.saturating_add(1);
        self.clear_reconfigure_failures();
    }

    fn mark_reconfigure_attempt(&mut self) {
        self.clear_acquire_failures();
        self.consecutive_reconfigure_failures =
            self.consecutive_reconfigure_failures.saturating_add(1);
    }

    fn mark_degrade(&mut self) {
        self.reset_failures();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceConfigurationDecision {
    pub source: SurfaceConfigurationError,
    pub action: SurfaceRecoveryAction,
}

impl SurfaceRecoveryPolicy {
    #[must_use]
    pub const fn new(retry_budget: u8) -> Self {
        Self::with_reconfigure_retry_budget(retry_budget, retry_budget)
    }

    #[must_use]
    pub const fn with_reconfigure_retry_budget(
        acquire_retry_budget: u8,
        reconfigure_retry_budget: u8,
    ) -> Self {
        Self {
            acquire_retry_budget,
            reconfigure_retry_budget,
        }
    }

    #[must_use]
    pub const fn retry_budget(self) -> u8 {
        self.acquire_retry_budget
    }

    #[must_use]
    pub const fn acquire_retry_budget(self) -> u8 {
        self.acquire_retry_budget
    }

    #[must_use]
    pub const fn reconfigure_retry_budget(self) -> u8 {
        self.reconfigure_retry_budget
    }

    #[must_use]
    pub fn classify(
        self,
        error: wgpu::SurfaceError,
        state: SurfaceRuntimeState,
    ) -> SurfaceErrorDecision {
        let category = classify_surface_error(&error);
        let action = match category {
            SurfaceErrorCategory::Retryable
                if state.consecutive_acquire_failures < self.acquire_retry_budget =>
            {
                SurfaceRecoveryAction::RetryAcquire
            }
            SurfaceErrorCategory::Retryable => SurfaceRecoveryAction::DegradeToCpu,
            SurfaceErrorCategory::ReconfigureRequired
                if state.consecutive_reconfigure_failures < self.reconfigure_retry_budget =>
            {
                SurfaceRecoveryAction::ReconfigureSurface
            }
            SurfaceErrorCategory::ReconfigureRequired => SurfaceRecoveryAction::DegradeToCpu,
            SurfaceErrorCategory::DegradeRequired => SurfaceRecoveryAction::DegradeToCpu,
        };

        SurfaceErrorDecision {
            source: error,
            category,
            action,
        }
    }

    pub fn on_acquire_success(self, state: &mut SurfaceRuntimeState) {
        state.reset_failures();
    }

    pub fn on_reconfigure_success(self, state: &mut SurfaceRuntimeState) {
        state.reset_failures();
    }

    #[must_use]
    pub fn on_surface_acquire_error(
        self,
        state: &mut SurfaceRuntimeState,
        error: wgpu::SurfaceError,
    ) -> SurfaceErrorDecision {
        let decision = self.classify(error, *state);
        match decision.action {
            SurfaceRecoveryAction::RetryAcquire => state.mark_retry_acquire(),
            SurfaceRecoveryAction::ReconfigureSurface => state.mark_reconfigure_attempt(),
            SurfaceRecoveryAction::DegradeToCpu => state.mark_degrade(),
        }
        decision
    }

    #[must_use]
    pub fn on_surface_configuration_error(
        self,
        state: &mut SurfaceRuntimeState,
        error: SurfaceConfigurationError,
    ) -> SurfaceConfigurationDecision {
        let action = if state.consecutive_reconfigure_failures < self.reconfigure_retry_budget {
            state.mark_reconfigure_attempt();
            SurfaceRecoveryAction::ReconfigureSurface
        } else {
            state.mark_degrade();
            SurfaceRecoveryAction::DegradeToCpu
        };

        SurfaceConfigurationDecision {
            source: error,
            action,
        }
    }
}

impl Default for SurfaceRecoveryPolicy {
    fn default() -> Self {
        Self::with_reconfigure_retry_budget(
            DEFAULT_SURFACE_RETRY_BUDGET,
            DEFAULT_SURFACE_RECONFIGURE_RETRY_BUDGET,
        )
    }
}

#[must_use]
pub fn classify_surface_error(error: &wgpu::SurfaceError) -> SurfaceErrorCategory {
    match error {
        wgpu::SurfaceError::Timeout => SurfaceErrorCategory::Retryable,
        wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost => {
            SurfaceErrorCategory::ReconfigureRequired
        }
        wgpu::SurfaceError::OutOfMemory | wgpu::SurfaceError::Other => {
            SurfaceErrorCategory::DegradeRequired
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceConfigurationError {
    ZeroWidth,
    ZeroHeight,
}

impl fmt::Display for SurfaceConfigurationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroWidth => f.write_str("surface configuration width must be non-zero"),
            Self::ZeroHeight => f.write_str("surface configuration height must be non-zero"),
        }
    }
}

impl Error for SurfaceConfigurationError {}

pub fn validate_surface_configuration(
    config: &wgpu::SurfaceConfiguration,
) -> Result<(), SurfaceConfigurationError> {
    if config.width == 0 {
        return Err(SurfaceConfigurationError::ZeroWidth);
    }
    if config.height == 0 {
        return Err(SurfaceConfigurationError::ZeroHeight);
    }
    Ok(())
}

pub fn update_surface_extent(
    config: &mut wgpu::SurfaceConfiguration,
    width: u32,
    height: u32,
    max_texture_dimension_2d: u32,
) -> Result<(), SurfaceConfigurationError> {
    if width == 0 {
        return Err(SurfaceConfigurationError::ZeroWidth);
    }
    if height == 0 {
        return Err(SurfaceConfigurationError::ZeroHeight);
    }

    let max_texture_dimension_2d = max_texture_dimension_2d.max(1);
    config.width = width.min(max_texture_dimension_2d);
    config.height = height.min(max_texture_dimension_2d);
    Ok(())
}

pub fn update_frame_latency_hint(
    config: &mut wgpu::SurfaceConfiguration,
    desired_maximum_frame_latency: u32,
) {
    config.desired_maximum_frame_latency = desired_maximum_frame_latency;
}

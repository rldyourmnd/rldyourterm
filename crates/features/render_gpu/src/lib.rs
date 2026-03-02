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
    // Mirrors wgpu `Surface::get_current_texture` error semantics:
    // timeout -> retry acquire, outdated/lost -> reconfigure swapchain, OOM -> degrade.
    match error {
        wgpu::SurfaceError::Timeout => SurfaceErrorCategory::Retryable,
        wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost => {
            SurfaceErrorCategory::ReconfigureRequired
        }
        wgpu::SurfaceError::OutOfMemory => SurfaceErrorCategory::DegradeRequired,
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
    // `wgpu::Surface::configure` panics when width/height are zero; fail fast here.
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
) -> Result<(), SurfaceConfigurationError> {
    if width == 0 {
        return Err(SurfaceConfigurationError::ZeroWidth);
    }
    if height == 0 {
        return Err(SurfaceConfigurationError::ZeroHeight);
    }

    config.width = width;
    config.height = height;
    Ok(())
}

pub fn update_frame_latency_hint(
    config: &mut wgpu::SurfaceConfiguration,
    desired_maximum_frame_latency: u32,
) {
    // Callers provide monitor-driven pacing inputs; renderer keeps this as an explicit hint.
    config.desired_maximum_frame_latency = desired_maximum_frame_latency;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuRenderer {
    policy: SurfaceRecoveryPolicy,
}

impl GpuRenderer {
    #[must_use]
    pub const fn new(policy: SurfaceRecoveryPolicy) -> Self {
        Self { policy }
    }

    #[must_use]
    pub const fn recovery_policy(self) -> SurfaceRecoveryPolicy {
        self.policy
    }

    #[must_use]
    pub fn classify_surface_error(
        &self,
        error: wgpu::SurfaceError,
        consecutive_retry_failures: u8,
    ) -> SurfaceErrorDecision {
        self.policy.classify(
            error,
            SurfaceRuntimeState::new(consecutive_retry_failures, 0),
        )
    }

    #[must_use]
    pub fn handle_surface_acquire_error(
        &self,
        state: &mut SurfaceRuntimeState,
        error: wgpu::SurfaceError,
    ) -> SurfaceErrorDecision {
        self.policy.on_surface_acquire_error(state, error)
    }

    #[must_use]
    pub fn handle_surface_configuration_error(
        &self,
        state: &mut SurfaceRuntimeState,
        error: SurfaceConfigurationError,
    ) -> SurfaceConfigurationDecision {
        self.policy.on_surface_configuration_error(state, error)
    }

    pub fn on_surface_acquire_success(&self, state: &mut SurfaceRuntimeState) {
        self.policy.on_acquire_success(state)
    }

    pub fn on_surface_reconfigure_success(&self, state: &mut SurfaceRuntimeState) {
        self.policy.on_reconfigure_success(state)
    }

    pub fn validate_surface_configuration(
        &self,
        config: &wgpu::SurfaceConfiguration,
    ) -> Result<(), SurfaceConfigurationError> {
        validate_surface_configuration(config)
    }

    pub fn render(&self) {}
}

impl Default for GpuRenderer {
    fn default() -> Self {
        Self::new(SurfaceRecoveryPolicy::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_is_retryable() {
        assert_eq!(
            classify_surface_error(&wgpu::SurfaceError::Timeout),
            SurfaceErrorCategory::Retryable
        );
    }

    #[test]
    fn outdated_and_lost_require_reconfigure() {
        assert_eq!(
            classify_surface_error(&wgpu::SurfaceError::Outdated),
            SurfaceErrorCategory::ReconfigureRequired
        );
        assert_eq!(
            classify_surface_error(&wgpu::SurfaceError::Lost),
            SurfaceErrorCategory::ReconfigureRequired
        );
    }

    #[test]
    fn oom_degrades_to_cpu() {
        assert_eq!(
            classify_surface_error(&wgpu::SurfaceError::OutOfMemory),
            SurfaceErrorCategory::DegradeRequired
        );
    }

    #[test]
    fn retryable_errors_degrade_when_budget_is_exhausted() {
        let policy = SurfaceRecoveryPolicy::new(1);
        let first = policy.classify(wgpu::SurfaceError::Timeout, SurfaceRuntimeState::new(0, 0));
        let second = policy.classify(wgpu::SurfaceError::Timeout, SurfaceRuntimeState::new(1, 0));
        assert_eq!(first.action, SurfaceRecoveryAction::RetryAcquire);
        assert_eq!(second.action, SurfaceRecoveryAction::DegradeToCpu);
    }

    #[test]
    fn zero_size_surface_config_is_rejected() {
        let mut config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8Unorm,
            width: 1,
            height: 1,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };
        assert_eq!(validate_surface_configuration(&config), Ok(()));

        config.width = 0;
        assert_eq!(
            validate_surface_configuration(&config),
            Err(SurfaceConfigurationError::ZeroWidth)
        );

        config.width = 1;
        config.height = 0;
        assert_eq!(
            validate_surface_configuration(&config),
            Err(SurfaceConfigurationError::ZeroHeight)
        );
    }

    #[test]
    fn acquire_timeout_transitions_from_retry_to_degrade() {
        let policy = SurfaceRecoveryPolicy::with_reconfigure_retry_budget(2, 2);
        let mut state = SurfaceRuntimeState::default();

        let first = policy.on_surface_acquire_error(&mut state, wgpu::SurfaceError::Timeout);
        assert_eq!(first.action, SurfaceRecoveryAction::RetryAcquire);
        assert_eq!(state.consecutive_acquire_failures(), 1);
        assert_eq!(state.consecutive_reconfigure_failures(), 0);

        let second = policy.on_surface_acquire_error(&mut state, wgpu::SurfaceError::Timeout);
        assert_eq!(second.action, SurfaceRecoveryAction::RetryAcquire);
        assert_eq!(state.consecutive_acquire_failures(), 2);
        assert_eq!(state.consecutive_reconfigure_failures(), 0);

        let third = policy.on_surface_acquire_error(&mut state, wgpu::SurfaceError::Timeout);
        assert_eq!(third.action, SurfaceRecoveryAction::DegradeToCpu);
        assert_eq!(state.consecutive_acquire_failures(), 0);
        assert_eq!(state.consecutive_reconfigure_failures(), 0);
    }

    #[test]
    fn acquire_outdated_uses_reconfigure_budget_before_degrade() {
        let policy = SurfaceRecoveryPolicy::with_reconfigure_retry_budget(2, 1);
        let mut state = SurfaceRuntimeState::default();

        let first = policy.on_surface_acquire_error(&mut state, wgpu::SurfaceError::Outdated);
        assert_eq!(first.action, SurfaceRecoveryAction::ReconfigureSurface);
        assert_eq!(state.consecutive_acquire_failures(), 0);
        assert_eq!(state.consecutive_reconfigure_failures(), 1);

        let second = policy.on_surface_acquire_error(&mut state, wgpu::SurfaceError::Lost);
        assert_eq!(second.action, SurfaceRecoveryAction::DegradeToCpu);
        assert_eq!(state.consecutive_acquire_failures(), 0);
        assert_eq!(state.consecutive_reconfigure_failures(), 0);
    }

    #[test]
    fn successful_reconfigure_resets_failure_counters() {
        let policy = SurfaceRecoveryPolicy::with_reconfigure_retry_budget(2, 2);
        let mut state = SurfaceRuntimeState::default();

        let _ = policy.on_surface_acquire_error(&mut state, wgpu::SurfaceError::Timeout);
        let _ = policy.on_surface_acquire_error(&mut state, wgpu::SurfaceError::Outdated);
        assert_eq!(state.consecutive_acquire_failures(), 0);
        assert_eq!(state.consecutive_reconfigure_failures(), 1);

        policy.on_reconfigure_success(&mut state);
        assert_eq!(state.consecutive_acquire_failures(), 0);
        assert_eq!(state.consecutive_reconfigure_failures(), 0);

        let after_reset = policy.on_surface_acquire_error(&mut state, wgpu::SurfaceError::Outdated);
        assert_eq!(
            after_reset.action,
            SurfaceRecoveryAction::ReconfigureSurface
        );
    }

    #[test]
    fn configuration_errors_reconfigure_then_degrade_when_budget_exhausted() {
        let policy = SurfaceRecoveryPolicy::with_reconfigure_retry_budget(1, 1);
        let mut state = SurfaceRuntimeState::default();

        let first =
            policy.on_surface_configuration_error(&mut state, SurfaceConfigurationError::ZeroWidth);
        assert_eq!(first.action, SurfaceRecoveryAction::ReconfigureSurface);
        assert_eq!(state.consecutive_reconfigure_failures(), 1);

        let second = policy
            .on_surface_configuration_error(&mut state, SurfaceConfigurationError::ZeroHeight);
        assert_eq!(second.action, SurfaceRecoveryAction::DegradeToCpu);
        assert_eq!(state.consecutive_acquire_failures(), 0);
        assert_eq!(state.consecutive_reconfigure_failures(), 0);
    }

    #[test]
    fn acquire_success_resets_all_failure_counters() {
        let policy = SurfaceRecoveryPolicy::with_reconfigure_retry_budget(2, 2);
        let mut state = SurfaceRuntimeState::new(2, 1);

        policy.on_acquire_success(&mut state);
        assert_eq!(state.consecutive_acquire_failures(), 0);
        assert_eq!(state.consecutive_reconfigure_failures(), 0);
    }

    #[test]
    fn retry_path_clears_reconfigure_failure_streak() {
        let policy = SurfaceRecoveryPolicy::with_reconfigure_retry_budget(2, 2);
        let mut state = SurfaceRuntimeState::default();

        let reconfigure = policy.on_surface_acquire_error(&mut state, wgpu::SurfaceError::Outdated);
        assert_eq!(
            reconfigure.action,
            SurfaceRecoveryAction::ReconfigureSurface
        );
        assert_eq!(state.consecutive_acquire_failures(), 0);
        assert_eq!(state.consecutive_reconfigure_failures(), 1);

        let retry = policy.on_surface_acquire_error(&mut state, wgpu::SurfaceError::Timeout);
        assert_eq!(retry.action, SurfaceRecoveryAction::RetryAcquire);
        assert_eq!(state.consecutive_acquire_failures(), 1);
        assert_eq!(state.consecutive_reconfigure_failures(), 0);
    }

    #[test]
    fn configuration_error_resets_acquire_failure_streak() {
        let policy = SurfaceRecoveryPolicy::with_reconfigure_retry_budget(2, 2);
        let mut state = SurfaceRuntimeState::default();

        let timeout = policy.on_surface_acquire_error(&mut state, wgpu::SurfaceError::Timeout);
        assert_eq!(timeout.action, SurfaceRecoveryAction::RetryAcquire);
        assert_eq!(state.consecutive_acquire_failures(), 1);
        assert_eq!(state.consecutive_reconfigure_failures(), 0);

        let config_error =
            policy.on_surface_configuration_error(&mut state, SurfaceConfigurationError::ZeroWidth);
        assert_eq!(
            config_error.action,
            SurfaceRecoveryAction::ReconfigureSurface
        );
        assert_eq!(state.consecutive_acquire_failures(), 0);
        assert_eq!(state.consecutive_reconfigure_failures(), 1);
    }

    #[test]
    fn oom_degrade_is_immediate_and_clears_failure_counters() {
        let policy = SurfaceRecoveryPolicy::with_reconfigure_retry_budget(2, 2);
        let mut state = SurfaceRuntimeState::new(2, 1);

        let decision = policy.on_surface_acquire_error(&mut state, wgpu::SurfaceError::OutOfMemory);
        assert_eq!(decision.action, SurfaceRecoveryAction::DegradeToCpu);
        assert_eq!(state.consecutive_acquire_failures(), 0);
        assert_eq!(state.consecutive_reconfigure_failures(), 0);
    }

    #[test]
    fn update_frame_latency_hint_is_explicit_and_monitor_driven() {
        let mut config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8Unorm,
            width: 1,
            height: 1,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };

        update_frame_latency_hint(&mut config, 4);
        assert_eq!(config.desired_maximum_frame_latency, 4);
    }
}

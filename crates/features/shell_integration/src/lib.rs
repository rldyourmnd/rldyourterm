// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

pub use rldyourterm_services::shell_target::ShellTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FishBaselineFailureCause {
    FishUnavailable,
    StarshipUnavailable,
    FishAndStarshipUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ShellAvailability {
    pub fish_available: bool,
    pub starship_available: bool,
    pub zsh_available: bool,
}

impl ShellAvailability {
    pub fn fish_baseline_ready(&self) -> bool {
        self.fish_available && self.starship_available
    }

    pub fn fish_baseline_failure_cause(&self) -> Option<FishBaselineFailureCause> {
        if self.fish_baseline_ready() {
            return None;
        }

        match (self.fish_available, self.starship_available) {
            (false, false) => Some(FishBaselineFailureCause::FishAndStarshipUnavailable),
            (false, true) => Some(FishBaselineFailureCause::FishUnavailable),
            (true, false) => Some(FishBaselineFailureCause::StarshipUnavailable),
            (true, true) => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellResolutionReason {
    FishBaselineReady,
    FishRequestedFallbackToZsh,
    AutoSelectedFishBaseline,
    AutoFallbackToZsh,
    ZshRequested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShellResolution {
    pub requested: ShellTarget,
    pub resolved: ShellTarget,
    pub fallback_applied: bool,
    pub reason: ShellResolutionReason,
    pub fallback_cause: Option<FishBaselineFailureCause>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellResolutionError {
    FishBaselineUnavailableAndZshUnavailable,
    ZshRequestedButUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellLaunchProfile {
    FishStarshipBaseline,
    ZshRequested,
    ZshFallback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellLaunchPlan {
    pub resolution: ShellResolution,
    pub executable: String,
    pub args: Vec<String>,
    pub profile: ShellLaunchProfile,
}

impl ShellLaunchPlan {
    pub fn from_resolution(resolution: ShellResolution) -> Self {
        let resolution = normalize_resolution_for_launch_plan(resolution);
        let (executable, profile) = match resolution.resolved {
            ShellTarget::Fish => ("fish", ShellLaunchProfile::FishStarshipBaseline),
            ShellTarget::Zsh if resolution.fallback_applied => {
                ("zsh", ShellLaunchProfile::ZshFallback)
            }
            ShellTarget::Zsh => ("zsh", ShellLaunchProfile::ZshRequested),
            ShellTarget::Auto => ("zsh", ShellLaunchProfile::ZshFallback),
        };

        Self {
            resolution,
            executable: executable.to_string(),
            args: vec!["-i".to_string(), "-l".to_string()],
            profile,
        }
    }
}

fn normalize_resolution_for_launch_plan(mut resolution: ShellResolution) -> ShellResolution {
    if resolution.requested == ShellTarget::Zsh {
        return ShellResolution {
            requested: ShellTarget::Zsh,
            resolved: ShellTarget::Zsh,
            fallback_applied: false,
            reason: ShellResolutionReason::ZshRequested,
            fallback_cause: None,
        };
    }

    if fish_baseline_launch_ready(&resolution) {
        return resolution;
    }

    let default_cause = if matches!(resolution.resolved, ShellTarget::Fish | ShellTarget::Auto) {
        FishBaselineFailureCause::StarshipUnavailable
    } else {
        FishBaselineFailureCause::FishAndStarshipUnavailable
    };

    resolution.resolved = ShellTarget::Zsh;
    resolution.fallback_applied = true;
    resolution.reason = fallback_reason_for_fish_baseline_failure(resolution.requested);
    resolution.fallback_cause = Some(resolution.fallback_cause.unwrap_or(default_cause));
    resolution
}

fn fish_baseline_launch_ready(resolution: &ShellResolution) -> bool {
    matches!(resolution.resolved, ShellTarget::Fish)
        && !resolution.fallback_applied
        && resolution.fallback_cause.is_none()
        && matches!(
            resolution.reason,
            ShellResolutionReason::FishBaselineReady
                | ShellResolutionReason::AutoSelectedFishBaseline
        )
}

fn fallback_reason_for_fish_baseline_failure(requested: ShellTarget) -> ShellResolutionReason {
    match requested {
        ShellTarget::Fish => ShellResolutionReason::FishRequestedFallbackToZsh,
        ShellTarget::Auto => ShellResolutionReason::AutoFallbackToZsh,
        ShellTarget::Zsh => ShellResolutionReason::ZshRequested,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellDiagnosticsEvent {
    ShellFallbackApplied(ShellResolution),
    ShellResolved(ShellResolution),
    ShellResolutionFailed {
        requested: ShellTarget,
        error: ShellResolutionError,
        fallback_cause: Option<FishBaselineFailureCause>,
    },
    ShellLaunchPlanned(ShellLaunchPlan),
}

pub trait ShellDiagnosticsHook {
    fn on_shell_event(&mut self, event: ShellDiagnosticsEvent);
}

pub fn resolve_shell_with_hook(
    requested: ShellTarget,
    availability: ShellAvailability,
    hook: &mut impl ShellDiagnosticsHook,
) -> Result<ShellResolution, ShellResolutionError> {
    let resolution = resolve_shell(requested, availability);
    match resolution {
        Ok(resolution) => {
            if resolution.fallback_applied {
                tracing::warn!(
                    requested = ?resolution.requested,
                    resolved = ?resolution.resolved,
                    reason = ?resolution.reason,
                    fallback_cause = ?resolution.fallback_cause,
                    "shell resolution applied fallback"
                );
                hook.on_shell_event(ShellDiagnosticsEvent::ShellFallbackApplied(resolution));
            } else {
                tracing::info!(
                    requested = ?resolution.requested,
                    resolved = ?resolution.resolved,
                    reason = ?resolution.reason,
                    "shell resolution completed"
                );
            }
            hook.on_shell_event(ShellDiagnosticsEvent::ShellResolved(resolution));
            Ok(resolution)
        }
        Err(error) => {
            let fallback_cause =
                fallback_cause_for_resolution_error(requested, availability, error);
            tracing::warn!(
                requested = ?requested,
                ?error,
                ?fallback_cause,
                "shell resolution failed"
            );
            hook.on_shell_event(ShellDiagnosticsEvent::ShellResolutionFailed {
                requested,
                error,
                fallback_cause,
            });
            Err(error)
        }
    }
}

pub fn plan_shell_launch(
    requested: ShellTarget,
    availability: ShellAvailability,
) -> Result<ShellLaunchPlan, ShellResolutionError> {
    resolve_shell(requested, availability).map(ShellLaunchPlan::from_resolution)
}

pub fn plan_shell_launch_with_hook(
    requested: ShellTarget,
    availability: ShellAvailability,
    hook: &mut impl ShellDiagnosticsHook,
) -> Result<ShellLaunchPlan, ShellResolutionError> {
    let resolution = resolve_shell_with_hook(requested, availability, hook)?;
    let plan = ShellLaunchPlan::from_resolution(resolution);
    tracing::info!(
        executable = %plan.executable,
        profile = ?plan.profile,
        "shell launch plan prepared"
    );
    hook.on_shell_event(ShellDiagnosticsEvent::ShellLaunchPlanned(plan.clone()));
    Ok(plan)
}

pub fn resolve_shell(
    requested: ShellTarget,
    availability: ShellAvailability,
) -> Result<ShellResolution, ShellResolutionError> {
    match requested {
        ShellTarget::Fish => {
            if availability.fish_baseline_ready() {
                Ok(ShellResolution {
                    requested,
                    resolved: ShellTarget::Fish,
                    fallback_applied: false,
                    reason: ShellResolutionReason::FishBaselineReady,
                    fallback_cause: None,
                })
            } else if availability.zsh_available {
                Ok(ShellResolution {
                    requested,
                    resolved: ShellTarget::Zsh,
                    fallback_applied: true,
                    reason: ShellResolutionReason::FishRequestedFallbackToZsh,
                    fallback_cause: availability.fish_baseline_failure_cause(),
                })
            } else {
                Err(ShellResolutionError::FishBaselineUnavailableAndZshUnavailable)
            }
        }
        ShellTarget::Auto => {
            if availability.fish_baseline_ready() {
                Ok(ShellResolution {
                    requested,
                    resolved: ShellTarget::Fish,
                    fallback_applied: false,
                    reason: ShellResolutionReason::AutoSelectedFishBaseline,
                    fallback_cause: None,
                })
            } else if availability.zsh_available {
                Ok(ShellResolution {
                    requested,
                    resolved: ShellTarget::Zsh,
                    fallback_applied: true,
                    reason: ShellResolutionReason::AutoFallbackToZsh,
                    fallback_cause: availability.fish_baseline_failure_cause(),
                })
            } else {
                Err(ShellResolutionError::FishBaselineUnavailableAndZshUnavailable)
            }
        }
        ShellTarget::Zsh => {
            if availability.zsh_available {
                Ok(ShellResolution {
                    requested,
                    resolved: ShellTarget::Zsh,
                    fallback_applied: false,
                    reason: ShellResolutionReason::ZshRequested,
                    fallback_cause: None,
                })
            } else {
                Err(ShellResolutionError::ZshRequestedButUnavailable)
            }
        }
    }
}

fn fallback_cause_for_resolution_error(
    requested: ShellTarget,
    availability: ShellAvailability,
    error: ShellResolutionError,
) -> Option<FishBaselineFailureCause> {
    match error {
        ShellResolutionError::FishBaselineUnavailableAndZshUnavailable
            if matches!(requested, ShellTarget::Fish | ShellTarget::Auto) =>
        {
            availability.fish_baseline_failure_cause()
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests;

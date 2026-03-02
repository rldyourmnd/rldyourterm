#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellTarget {
    Fish,
    Zsh,
    Auto,
}

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
        let (executable, profile) = match resolution.resolved {
            ShellTarget::Fish => ("fish", ShellLaunchProfile::FishStarshipBaseline),
            ShellTarget::Zsh if resolution.fallback_applied => {
                ("zsh", ShellLaunchProfile::ZshFallback)
            }
            ShellTarget::Zsh => ("zsh", ShellLaunchProfile::ZshRequested),
            ShellTarget::Auto => ("fish", ShellLaunchProfile::FishStarshipBaseline),
        };

        Self {
            resolution,
            executable: executable.to_string(),
            args: vec!["-i".to_string(), "-l".to_string()],
            profile,
        }
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
mod tests {
    use super::*;

    fn full_shells() -> ShellAvailability {
        ShellAvailability {
            fish_available: true,
            starship_available: true,
            zsh_available: true,
        }
    }

    #[test]
    fn fish_prefers_fish_when_baseline_is_ready() {
        let resolution = resolve_shell(ShellTarget::Fish, full_shells()).unwrap();

        assert_eq!(
            resolution,
            ShellResolution {
                requested: ShellTarget::Fish,
                resolved: ShellTarget::Fish,
                fallback_applied: false,
                reason: ShellResolutionReason::FishBaselineReady,
                fallback_cause: None,
            }
        );
    }

    #[test]
    fn fish_falls_back_to_zsh_when_starship_is_missing() {
        let resolution = resolve_shell(
            ShellTarget::Fish,
            ShellAvailability {
                fish_available: true,
                starship_available: false,
                zsh_available: true,
            },
        )
        .unwrap();

        assert_eq!(resolution.resolved, ShellTarget::Zsh);
        assert!(resolution.fallback_applied);
        assert_eq!(
            resolution.reason,
            ShellResolutionReason::FishRequestedFallbackToZsh
        );
        assert_eq!(
            resolution.fallback_cause,
            Some(FishBaselineFailureCause::StarshipUnavailable)
        );
    }

    #[test]
    fn auto_mode_is_deterministic_and_uses_same_fallback_rules() {
        let resolution = resolve_shell(
            ShellTarget::Auto,
            ShellAvailability {
                fish_available: false,
                starship_available: true,
                zsh_available: true,
            },
        )
        .unwrap();

        assert_eq!(resolution.resolved, ShellTarget::Zsh);
        assert!(resolution.fallback_applied);
        assert_eq!(resolution.reason, ShellResolutionReason::AutoFallbackToZsh);
        assert_eq!(
            resolution.fallback_cause,
            Some(FishBaselineFailureCause::FishUnavailable)
        );
    }

    #[test]
    fn zsh_request_without_zsh_returns_explicit_error() {
        let err = resolve_shell(
            ShellTarget::Zsh,
            ShellAvailability {
                fish_available: true,
                starship_available: true,
                zsh_available: false,
            },
        )
        .unwrap_err();

        assert_eq!(err, ShellResolutionError::ZshRequestedButUnavailable);
    }

    #[test]
    fn returns_error_when_neither_baseline_fish_nor_zsh_is_available() {
        let err = resolve_shell(
            ShellTarget::Fish,
            ShellAvailability {
                fish_available: false,
                starship_available: false,
                zsh_available: false,
            },
        )
        .unwrap_err();

        assert_eq!(
            err,
            ShellResolutionError::FishBaselineUnavailableAndZshUnavailable
        );
    }

    #[test]
    fn shell_launch_plan_is_deterministic_for_fish_baseline() {
        let plan = plan_shell_launch(ShellTarget::Fish, full_shells()).unwrap();
        assert_eq!(plan.executable, "fish");
        assert_eq!(plan.args, vec!["-i".to_string(), "-l".to_string()]);
        assert_eq!(plan.profile, ShellLaunchProfile::FishStarshipBaseline);
        assert_eq!(plan.resolution.resolved, ShellTarget::Fish);
    }

    #[test]
    fn shell_launch_plan_is_deterministic_for_fallback_zsh() {
        let plan = plan_shell_launch(
            ShellTarget::Auto,
            ShellAvailability {
                fish_available: false,
                starship_available: true,
                zsh_available: true,
            },
        )
        .unwrap();
        assert_eq!(plan.executable, "zsh");
        assert_eq!(plan.profile, ShellLaunchProfile::ZshFallback);
        assert_eq!(plan.resolution.resolved, ShellTarget::Zsh);
    }

    #[derive(Default)]
    struct RecordingHook {
        events: Vec<ShellDiagnosticsEvent>,
    }

    impl ShellDiagnosticsHook for RecordingHook {
        fn on_shell_event(&mut self, event: ShellDiagnosticsEvent) {
            self.events.push(event);
        }
    }

    #[test]
    fn diagnostics_hook_observes_resolution_and_launch_plan() {
        let mut hook = RecordingHook::default();
        let plan = plan_shell_launch_with_hook(
            ShellTarget::Auto,
            ShellAvailability {
                fish_available: false,
                starship_available: true,
                zsh_available: true,
            },
            &mut hook,
        )
        .unwrap();

        assert_eq!(hook.events.len(), 3);
        assert_eq!(
            hook.events[0],
            ShellDiagnosticsEvent::ShellFallbackApplied(plan.resolution)
        );
        assert_eq!(
            hook.events[1],
            ShellDiagnosticsEvent::ShellResolved(plan.resolution)
        );
        assert_eq!(
            hook.events[2],
            ShellDiagnosticsEvent::ShellLaunchPlanned(plan)
        );
    }

    #[test]
    fn diagnostics_hook_observes_resolution_errors() {
        let mut hook = RecordingHook::default();
        let err = resolve_shell_with_hook(
            ShellTarget::Fish,
            ShellAvailability {
                fish_available: false,
                starship_available: false,
                zsh_available: false,
            },
            &mut hook,
        )
        .unwrap_err();

        assert_eq!(
            err,
            ShellResolutionError::FishBaselineUnavailableAndZshUnavailable
        );
        assert_eq!(
            hook.events,
            vec![ShellDiagnosticsEvent::ShellResolutionFailed {
                requested: ShellTarget::Fish,
                error: ShellResolutionError::FishBaselineUnavailableAndZshUnavailable,
                fallback_cause: Some(FishBaselineFailureCause::FishAndStarshipUnavailable),
            }]
        );
    }

    #[test]
    fn diagnostics_hook_zsh_resolution_error_has_no_fallback_cause() {
        let mut hook = RecordingHook::default();
        let err = resolve_shell_with_hook(
            ShellTarget::Zsh,
            ShellAvailability {
                fish_available: true,
                starship_available: true,
                zsh_available: false,
            },
            &mut hook,
        )
        .unwrap_err();

        assert_eq!(err, ShellResolutionError::ZshRequestedButUnavailable);
        assert_eq!(
            hook.events,
            vec![ShellDiagnosticsEvent::ShellResolutionFailed {
                requested: ShellTarget::Zsh,
                error: ShellResolutionError::ZshRequestedButUnavailable,
                fallback_cause: None,
            }]
        );
    }
}

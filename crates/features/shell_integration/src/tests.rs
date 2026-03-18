// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

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

#[test]
fn fish_baseline_failure_cause_matrix_is_explicit() {
    let cases = [
        (
            false,
            false,
            Some(FishBaselineFailureCause::FishAndStarshipUnavailable),
        ),
        (false, true, Some(FishBaselineFailureCause::FishUnavailable)),
        (
            true,
            false,
            Some(FishBaselineFailureCause::StarshipUnavailable),
        ),
        (true, true, None),
    ];

    for (fish_available, starship_available, expected_cause) in cases {
        let availability = ShellAvailability {
            fish_available,
            starship_available,
            zsh_available: true,
        };

        assert_eq!(availability.fish_baseline_failure_cause(), expected_cause);
    }
}

#[test]
fn launch_plan_falls_back_to_zsh_when_fish_exists_without_starship_activation_guarantee() {
    let plan = plan_shell_launch(
        ShellTarget::Fish,
        ShellAvailability {
            fish_available: true,
            starship_available: false,
            zsh_available: true,
        },
    )
    .unwrap();

    assert_eq!(plan.executable, "zsh");
    assert_eq!(plan.profile, ShellLaunchProfile::ZshFallback);
    assert_eq!(
        plan.resolution.reason,
        ShellResolutionReason::FishRequestedFallbackToZsh
    );
    assert_eq!(
        plan.resolution.fallback_cause,
        Some(FishBaselineFailureCause::StarshipUnavailable)
    );
}

#[test]
fn auto_launch_plan_uses_same_starship_activation_fallback_cause() {
    let plan = plan_shell_launch(
        ShellTarget::Auto,
        ShellAvailability {
            fish_available: true,
            starship_available: false,
            zsh_available: true,
        },
    )
    .unwrap();

    assert_eq!(plan.executable, "zsh");
    assert_eq!(plan.profile, ShellLaunchProfile::ZshFallback);
    assert_eq!(
        plan.resolution.reason,
        ShellResolutionReason::AutoFallbackToZsh
    );
    assert_eq!(
        plan.resolution.fallback_cause,
        Some(FishBaselineFailureCause::StarshipUnavailable)
    );
}

#[test]
fn from_resolution_enforces_fish_starship_baseline_health_before_fish_launch() {
    let plan = ShellLaunchPlan::from_resolution(ShellResolution {
        requested: ShellTarget::Fish,
        resolved: ShellTarget::Fish,
        fallback_applied: true,
        reason: ShellResolutionReason::FishBaselineReady,
        fallback_cause: None,
    });

    assert_eq!(plan.executable, "zsh");
    assert_eq!(plan.profile, ShellLaunchProfile::ZshFallback);
    assert_eq!(plan.resolution.resolved, ShellTarget::Zsh);
    assert!(plan.resolution.fallback_applied);
    assert_eq!(
        plan.resolution.reason,
        ShellResolutionReason::FishRequestedFallbackToZsh
    );
    assert_eq!(
        plan.resolution.fallback_cause,
        Some(FishBaselineFailureCause::StarshipUnavailable)
    );
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

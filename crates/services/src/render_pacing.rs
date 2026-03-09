// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use std::time::{Duration, Instant};

use tracing::info;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderCadence {
    pub refresh_rate_millihz: u32,
}

impl RenderCadence {
    pub fn from_monitor(refresh_rate_millihz: u32) -> Self {
        Self {
            refresh_rate_millihz,
        }
    }

    pub fn checked_from_monitor(refresh_rate_millihz: u32) -> Option<Self> {
        if refresh_rate_millihz == 0 {
            return None;
        }
        Some(Self::from_monitor(refresh_rate_millihz))
    }

    pub fn frame_interval(&self) -> Option<Duration> {
        if self.refresh_rate_millihz == 0 {
            return None;
        }

        let nanos_per_frame = 1_000_000_000_000u128 / u128::from(self.refresh_rate_millihz);
        let nanos_per_frame = nanos_per_frame.max(1);
        let nanos_per_frame = if nanos_per_frame > u128::from(u64::MAX) {
            u64::MAX
        } else {
            nanos_per_frame as u64
        };

        Some(Duration::from_nanos(nanos_per_frame))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CadenceResync {
    pub changed: bool,
    pub schedule_invalidated: bool,
    pub generation: u64,
    pub previous: Option<RenderCadence>,
    pub current: Option<RenderCadence>,
    pub trigger: CadenceResyncTrigger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CadenceResyncTrigger {
    MonitorTimingSample,
    MonitorTransfer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameSchedule {
    pub deadline: Instant,
    pub generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderPacingController {
    cadence: Option<RenderCadence>,
    generation: u64,
}

impl Default for RenderPacingController {
    fn default() -> Self {
        Self::new(None)
    }
}

impl RenderPacingController {
    pub fn new(refresh_rate_millihz: Option<u32>) -> Self {
        Self {
            cadence: refresh_rate_millihz.and_then(RenderCadence::checked_from_monitor),
            generation: 0,
        }
    }

    pub fn cadence(&self) -> Option<RenderCadence> {
        self.cadence
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn resync_from_monitor(&mut self, refresh_rate_millihz: Option<u32>) -> CadenceResync {
        self.resync(
            refresh_rate_millihz,
            CadenceResyncTrigger::MonitorTimingSample,
            false,
        )
    }

    pub fn resync_after_monitor_transfer(
        &mut self,
        refresh_rate_millihz: Option<u32>,
    ) -> CadenceResync {
        self.resync(
            refresh_rate_millihz,
            CadenceResyncTrigger::MonitorTransfer,
            true,
        )
    }

    pub fn next_frame_deadline(&self, last_presented_at: Instant) -> Option<Instant> {
        self.cadence
            .and_then(|cadence| cadence.frame_interval())
            .map(|interval| last_presented_at + interval)
    }

    pub fn schedule_next_frame(&self, last_presented_at: Instant) -> Option<FrameSchedule> {
        self.next_frame_deadline(last_presented_at)
            .map(|deadline| FrameSchedule {
                deadline,
                generation: self.generation,
            })
    }

    pub fn is_schedule_stale(&self, schedule_generation: u64) -> bool {
        schedule_generation != self.generation
    }

    fn resync(
        &mut self,
        refresh_rate_millihz: Option<u32>,
        trigger: CadenceResyncTrigger,
        force_schedule_invalidation: bool,
    ) -> CadenceResync {
        let next = refresh_rate_millihz
            .and_then(RenderCadence::checked_from_monitor)
            .or(self.cadence);
        let previous = self.cadence;
        let changed = previous != next;
        let schedule_invalidated = changed || force_schedule_invalidation;
        let current = if schedule_invalidated {
            self.cadence = next;
            self.generation = self.generation.saturating_add(1);
            info!(
                generation = self.generation,
                trigger = ?trigger,
                cadence_changed = changed,
                previous_refresh_millihz = ?previous.map(|value| value.refresh_rate_millihz),
                current_refresh_millihz = ?self.cadence.map(|value| value.refresh_rate_millihz),
                "render cadence re-synced from monitor timing",
            );
            self.cadence
        } else {
            self.cadence
        };

        CadenceResync {
            changed,
            schedule_invalidated,
            generation: self.generation,
            previous,
            current,
            trigger,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cadence_from_monitor_preserves_refresh_rate() {
        let cadence = RenderCadence::from_monitor(60_000);
        assert_eq!(cadence.refresh_rate_millihz, 60_000);
    }

    #[test]
    fn frame_interval_is_monitor_driven() {
        let cadence = RenderCadence::from_monitor(60_000);
        assert_eq!(
            cadence.frame_interval(),
            Some(Duration::from_nanos(1_000_000_000_000 / 60_000))
        );
    }

    #[test]
    fn zero_refresh_rate_disables_cadence_interval() {
        let cadence = RenderCadence::from_monitor(0);
        assert_eq!(cadence.frame_interval(), None);
        assert_eq!(RenderCadence::checked_from_monitor(0), None);
    }

    #[test]
    fn controller_resync_changes_generation_when_refresh_changes() {
        let mut controller = RenderPacingController::new(Some(60_000));

        let resync = controller.resync_after_monitor_transfer(Some(144_000));
        assert!(resync.changed);
        assert!(resync.schedule_invalidated);
        assert_eq!(resync.generation, 1);
        assert_eq!(resync.trigger, CadenceResyncTrigger::MonitorTransfer);
        assert_eq!(
            resync.previous,
            Some(RenderCadence {
                refresh_rate_millihz: 60_000,
            })
        );
        assert_eq!(
            resync.current,
            Some(RenderCadence {
                refresh_rate_millihz: 144_000,
            })
        );
        assert_eq!(controller.generation(), 1);
    }

    #[test]
    fn controller_resync_is_noop_for_unchanged_refresh() {
        let mut controller = RenderPacingController::new(Some(60_000));

        let first = controller.resync_from_monitor(Some(60_000));
        assert!(!first.changed);
        assert!(!first.schedule_invalidated);
        assert_eq!(first.generation, 0);
        assert_eq!(first.trigger, CadenceResyncTrigger::MonitorTimingSample);

        let second = controller.resync_from_monitor(Some(60_000));
        assert!(!second.changed);
        assert!(!second.schedule_invalidated);
        assert_eq!(second.generation, 0);
        assert_eq!(second.trigger, CadenceResyncTrigger::MonitorTimingSample);
    }

    #[test]
    fn controller_preserves_cadence_when_monitor_timing_unavailable() {
        let mut controller = RenderPacingController::new(Some(60_000));

        let resync = controller.resync_from_monitor(None);
        assert!(!resync.changed);
        assert!(!resync.schedule_invalidated);
        assert_eq!(resync.generation, 0);
        assert_eq!(resync.trigger, CadenceResyncTrigger::MonitorTimingSample);
        assert_eq!(
            controller.cadence(),
            Some(RenderCadence {
                refresh_rate_millihz: 60_000,
            })
        );
    }

    #[test]
    fn next_frame_deadline_is_unavailable_without_monitor_timing() {
        let controller = RenderPacingController::new(None);
        assert_eq!(controller.next_frame_deadline(Instant::now()), None);
    }

    #[test]
    fn frame_schedule_contains_generation_token_for_resync_detection() {
        let mut controller = RenderPacingController::new(Some(60_000));
        let now = Instant::now();

        let schedule = controller
            .schedule_next_frame(now)
            .expect("expected schedule with known cadence");
        assert!(!controller.is_schedule_stale(schedule.generation));

        let _ = controller.resync_after_monitor_transfer(Some(144_000));
        assert!(controller.is_schedule_stale(schedule.generation));
    }

    #[test]
    fn monitor_transfer_forces_schedule_invalidation_even_when_refresh_is_unchanged() {
        let mut controller = RenderPacingController::new(Some(60_000));
        let now = Instant::now();
        let schedule = controller
            .schedule_next_frame(now)
            .expect("expected schedule with known cadence");

        let resync = controller.resync_after_monitor_transfer(Some(60_000));
        assert!(!resync.changed);
        assert!(resync.schedule_invalidated);
        assert_eq!(resync.trigger, CadenceResyncTrigger::MonitorTransfer);
        assert_eq!(resync.generation, 1);
        assert!(controller.is_schedule_stale(schedule.generation));
    }

    #[test]
    fn repeated_monitor_transfer_events_increment_generation_deterministically() {
        let mut controller = RenderPacingController::new(Some(60_000));

        let first = controller.resync_after_monitor_transfer(Some(60_000));
        let second = controller.resync_after_monitor_transfer(Some(60_000));

        assert!(!first.changed);
        assert!(first.schedule_invalidated);
        assert_eq!(first.generation, 1);
        assert_eq!(first.current, controller.cadence());

        assert!(!second.changed);
        assert!(second.schedule_invalidated);
        assert_eq!(second.generation, 2);
        assert_eq!(second.current, controller.cadence());
    }

    #[test]
    fn monitor_transfer_to_missing_timing_preserves_cadence_and_invalidates_schedule() {
        let mut controller = RenderPacingController::new(Some(144_000));
        let schedule = controller
            .schedule_next_frame(Instant::now())
            .expect("expected schedule with known cadence");

        let resync = controller.resync_after_monitor_transfer(None);
        assert!(!resync.changed);
        assert!(resync.schedule_invalidated);
        assert_eq!(resync.generation, 1);
        assert_eq!(
            resync.previous,
            Some(RenderCadence {
                refresh_rate_millihz: 144_000,
            })
        );
        assert_eq!(
            resync.current,
            Some(RenderCadence {
                refresh_rate_millihz: 144_000,
            })
        );
        assert_eq!(
            controller.cadence(),
            Some(RenderCadence {
                refresh_rate_millihz: 144_000,
            })
        );
        assert!(controller.is_schedule_stale(schedule.generation));
        assert!(controller.schedule_next_frame(Instant::now()).is_some());
    }

    #[test]
    fn monitor_transfer_with_missing_timing_is_deterministic_when_already_unknown() {
        let mut controller = RenderPacingController::new(None);

        let first = controller.resync_after_monitor_transfer(None);
        assert!(!first.changed);
        assert!(first.schedule_invalidated);
        assert_eq!(first.generation, 1);
        assert_eq!(first.previous, None);
        assert_eq!(first.current, None);

        let second = controller.resync_after_monitor_transfer(Some(0));
        assert!(!second.changed);
        assert!(second.schedule_invalidated);
        assert_eq!(second.generation, 2);
        assert_eq!(second.previous, None);
        assert_eq!(second.current, None);
    }

    #[test]
    fn zero_refresh_sample_is_noop_when_cadence_is_already_missing() {
        let mut controller = RenderPacingController::new(None);

        let resync = controller.resync_from_monitor(Some(0));
        assert!(!resync.changed);
        assert!(!resync.schedule_invalidated);
        assert_eq!(resync.generation, 0);
        assert_eq!(resync.previous, None);
        assert_eq!(resync.current, None);
        assert_eq!(controller.cadence(), None);
    }

    #[test]
    fn schedule_generation_stays_stable_without_resync() {
        let controller = RenderPacingController::new(Some(60_000));
        let now = Instant::now();

        let first = controller
            .schedule_next_frame(now)
            .expect("expected schedule with known cadence");
        let second = controller
            .schedule_next_frame(now + Duration::from_millis(5))
            .expect("expected schedule with known cadence");

        assert_eq!(first.generation, 0);
        assert_eq!(second.generation, 0);
        assert!(!controller.is_schedule_stale(first.generation));
        assert!(!controller.is_schedule_stale(second.generation));
    }

    #[test]
    fn monitor_timing_sample_with_same_refresh_keeps_existing_schedule_fresh() {
        let mut controller = RenderPacingController::new(Some(60_000));
        let now = Instant::now();
        let schedule = controller
            .schedule_next_frame(now)
            .expect("expected schedule with known cadence");

        let resync = controller.resync_from_monitor(Some(60_000));
        assert!(!resync.changed);
        assert!(!resync.schedule_invalidated);
        assert_eq!(resync.trigger, CadenceResyncTrigger::MonitorTimingSample);
        assert_eq!(resync.generation, 0);
        assert!(!controller.is_schedule_stale(schedule.generation));
    }

    #[test]
    fn non_standard_refresh_rate_still_uses_monitor_timing_formula() {
        let cadence = RenderCadence::from_monitor(59_940);
        let expected = Duration::from_nanos(1_000_000_000_000 / 59_940);
        assert_eq!(cadence.frame_interval(), Some(expected));
    }
}

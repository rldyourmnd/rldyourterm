// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use crate::api::common::MonitorTiming;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowHealth {
    Healthy,
    Degraded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorTimingSource {
    CurrentMonitor,
    PrimaryMonitorFallback,
    CachedMonitorFallback,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorTimingReading {
    pub timing: Option<MonitorTiming>,
    pub source: MonitorTimingSource,
    pub health: WindowHealth,
}

impl MonitorTimingReading {
    pub const fn from_current_monitor(timing: MonitorTiming) -> Self {
        let health = if timing.refresh_rate_millihz.is_some() {
            WindowHealth::Healthy
        } else {
            WindowHealth::Degraded
        };
        Self {
            timing: Some(timing),
            source: MonitorTimingSource::CurrentMonitor,
            health,
        }
    }

    pub const fn from_primary_monitor_fallback(timing: MonitorTiming) -> Self {
        Self {
            timing: Some(timing),
            source: MonitorTimingSource::PrimaryMonitorFallback,
            health: WindowHealth::Degraded,
        }
    }

    pub const fn from_cached_fallback(timing: MonitorTiming) -> Self {
        Self {
            timing: Some(timing),
            source: MonitorTimingSource::CachedMonitorFallback,
            health: WindowHealth::Degraded,
        }
    }

    pub const fn unavailable() -> Self {
        Self {
            timing: None,
            source: MonitorTimingSource::Unavailable,
            health: WindowHealth::Degraded,
        }
    }
}

// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use serde::{Deserialize, Serialize};

use crate::render_mode::{GpuFailureKind, RenderMode, RenderModeTransition};
use crate::session::{SessionBoundary, SessionState, SessionTransition};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "kebab-case")]
pub enum UiRuntimeCommand {
    Tick,
    RecoverableBoundary(SessionBoundary),
    FatalBoundary(SessionBoundary),
    RequestStop,
    MarkStopped,
    SetRenderMode(RenderMode),
    GpuFailure {
        kind: GpuFailureKind,
        observed_at_millis: u64,
    },
    GpuFramePresented,
    ResyncCadence {
        refresh_rate_millihz: u32,
    },
    ResyncCadenceAfterTransfer {
        refresh_rate_millihz: u32,
    },
    AssertSingleWindow {
        requested: u8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "kebab-case")]
pub enum UiCommandOutcome {
    Noop,
    SessionTransition(SessionTransition),
    RenderModeTransition(RenderModeTransition),
    CadenceResynced {
        previous_refresh_rate_millihz: Option<u32>,
        current_refresh_rate_millihz: Option<u32>,
        generation: u64,
        schedule_invalidated: bool,
        monitor_transfer: bool,
    },
    GpuRetryScheduled {
        failure_kind: GpuFailureKind,
        failure_streak: u8,
        retry_budget_remaining: u8,
    },
    SingleWindowConfirmed {
        window_count: u8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiCommandReceipt {
    pub command: UiRuntimeCommand,
    pub outcome: UiCommandOutcome,
    pub state: SessionState,
    pub render_mode: RenderMode,
    pub cadence_millihz: u32,
    pub window_count: u8,
}

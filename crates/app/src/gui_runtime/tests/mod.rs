// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

mod geometry;
mod keys;
mod output;
mod render;
mod runtime;

use super::{CLIPBOARD_PASTE_CAP_BYTES, RenderMode, cap_paste_text, read_clipboard_text_for_paste};
use rldyourterm_foundation::api::clipboard::ClipboardAdapter;
use rldyourterm_foundation::api::common::{ContractResult, MonitorTiming};
use rldyourterm_foundation::api::window::WindowControl;
use rldyourterm_foundation::error::{
    ClipboardFailureCode, ClipboardOperation, FoundationError, Recoverability, WindowFailureCode,
    WindowOperation,
};
use rldyourterm_ui::{UiBootstrapConfig, UiRuntime};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StubClipboardScenario {
    Text(&'static str),
    Empty,
    Error,
}

pub(super) struct StubClipboard {
    pub(super) scenario: StubClipboardScenario,
}

impl ClipboardAdapter for StubClipboard {
    fn set_text(&self, _text: &str) -> ContractResult<()> {
        Ok(())
    }

    fn get_text(&self) -> ContractResult<Option<String>> {
        match self.scenario {
            StubClipboardScenario::Text(text) => Ok(Some(text.to_owned())),
            StubClipboardScenario::Empty => Ok(None),
            StubClipboardScenario::Error => Err(FoundationError::clipboard(
                ClipboardOperation::GetText,
                ClipboardFailureCode::BoundaryFault,
                Recoverability::Degrade,
                "test clipboard failure",
                None,
            )),
        }
    }

    fn clear(&self) -> ContractResult<()> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StubWindowControlScenario {
    Timing(Option<u32>),
    Error,
}

pub(super) struct StubWindowControl {
    pub(super) scenario: StubWindowControlScenario,
}

impl WindowControl for StubWindowControl {
    fn request_redraw(&self) -> ContractResult<()> {
        Ok(())
    }

    fn set_title(&self, _title: &str) -> ContractResult<()> {
        Ok(())
    }

    fn current_monitor_timing(&self) -> ContractResult<MonitorTiming> {
        match self.scenario {
            StubWindowControlScenario::Timing(refresh_rate_millihz) => Ok(MonitorTiming {
                monitor_name: Some("stub-monitor".to_owned()),
                refresh_rate_millihz,
            }),
            StubWindowControlScenario::Error => Err(FoundationError::window(
                WindowOperation::QueryMonitorTiming,
                WindowFailureCode::BoundaryFault,
                Recoverability::Degrade,
                "monitor timing unavailable",
                None,
            )),
        }
    }

    fn close(&self) -> ContractResult<()> {
        Ok(())
    }
}

pub(super) fn test_ui_runtime(mode: RenderMode) -> UiRuntime {
    UiRuntime::bootstrap(UiBootstrapConfig::single_window(mode, 60_000))
        .expect("ui runtime bootstrap")
}

#[test]
fn clipboard_dispatch_returns_non_empty_text() {
    let clipboard = StubClipboard {
        scenario: StubClipboardScenario::Text("wave3"),
    };
    assert_eq!(
        read_clipboard_text_for_paste(&clipboard),
        Some("wave3".to_owned())
    );
}

#[test]
fn clipboard_dispatch_ignores_empty_text() {
    let clipboard = StubClipboard {
        scenario: StubClipboardScenario::Empty,
    };
    assert_eq!(read_clipboard_text_for_paste(&clipboard), None);
}

#[test]
fn clipboard_dispatch_ignores_adapter_errors() {
    let clipboard = StubClipboard {
        scenario: StubClipboardScenario::Error,
    };
    assert_eq!(read_clipboard_text_for_paste(&clipboard), None);
}

#[test]
fn clipboard_paste_cap_limits_payload_to_64kb() {
    let payload = "x".repeat(70 * 1024);
    assert_eq!(cap_paste_text(&payload).len(), CLIPBOARD_PASTE_CAP_BYTES);
}

#[test]
fn clipboard_paste_cap_preserves_utf8_boundary() {
    let payload = format!("{}🚀", "a".repeat(CLIPBOARD_PASTE_CAP_BYTES - 1));
    let capped = cap_paste_text(&payload);
    assert_eq!(capped.len(), CLIPBOARD_PASTE_CAP_BYTES - 1);
    assert_eq!(capped.chars().last(), Some('a'));
}

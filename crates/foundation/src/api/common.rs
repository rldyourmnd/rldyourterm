// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

pub use crate::error::CorrelationId;
use crate::error::FoundationResult;

pub type ContractResult<T> = FoundationResult<T>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewportSize {
    pub cols: u16,
    pub rows: u16,
    pub width_px: u16,
    pub height_px: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontMetrics {
    pub cell_width: u16,
    pub cell_height: u16,
    pub ascender: i16,
    pub descender: i16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorTiming {
    pub monitor_name: Option<String>,
    pub refresh_rate_millihz: Option<u32>,
}

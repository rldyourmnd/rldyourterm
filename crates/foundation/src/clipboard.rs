// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardHealth {
    Available,
    Degraded,
    Unavailable,
}

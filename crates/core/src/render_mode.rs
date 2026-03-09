// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    Cpu,
    Gpu,
    Auto,
}

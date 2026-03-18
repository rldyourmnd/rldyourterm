// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use crate::api::common::ContractResult;

pub trait ClipboardAdapter: Send + Sync {
    fn set_text(&self, text: &str) -> ContractResult<()>;
    fn get_text(&self) -> ContractResult<Option<String>>;
    fn clear(&self) -> ContractResult<()>;
}

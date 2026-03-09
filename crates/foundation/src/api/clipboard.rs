// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use crate::api::common::ContractResult;

pub trait ClipboardAdapter: Send + Sync {
    fn set_text(&self, text: &str) -> ContractResult<()>;
    fn get_text(&self) -> ContractResult<Option<String>>;
    fn clear(&self) -> ContractResult<()>;
}

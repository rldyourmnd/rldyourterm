// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

#![no_main]

use libfuzzer_sys::fuzz_target;
use rldyourterm_core::Parser;

fuzz_target!(|data: &[u8]| {
    let mut parser = Parser::default();
    let mut actions = Vec::new();

    for chunk in data.chunks(64) {
        parser.feed_into(chunk, &mut actions);
    }

    parser.resync_after_truncation_into(&mut actions);
});

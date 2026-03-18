// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

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

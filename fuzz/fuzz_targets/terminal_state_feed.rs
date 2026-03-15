// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

#![no_main]

use libfuzzer_sys::fuzz_target;
use rldyourterm_core::TerminalState;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    let chunk_size = (data[0] as usize).max(1);
    let input = &data[1..];

    let mut terminal = TerminalState::new(80, 24, 100);
    let mut responses = Vec::new();

    for chunk in input.chunks(chunk_size) {
        terminal.feed_terminal_responses_into(chunk, &mut responses);
        responses.clear();
    }

    // Exercise resize when the fuzzer provides enough data: the first byte
    // already consumed above doubles as a resize trigger when input length
    // exceeds 64 bytes (arbitrary threshold to keep the common path cheap).
    if input.len() > 64 {
        let new_cols = ((data[0] as u16) % 200).max(1);
        let new_rows = if input.len() > 1 {
            ((input[0] as u16) % 100).max(1)
        } else {
            24
        };
        terminal.resize(new_cols, new_rows);

        // Feed remaining tail after resize to explore post-resize paths.
        for chunk in input[64..].chunks(chunk_size) {
            terminal.feed_terminal_responses_into(chunk, &mut responses);
            responses.clear();
        }
    }
});

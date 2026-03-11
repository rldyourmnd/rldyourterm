// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use crate::cli::{Cli, ScaleArg};
use crate::data::Workload;
use rldyourterm_services::terminal::{MAX_FEED_BYTES_PER_CALL, TerminalState};
use std::hint::black_box;

pub fn seeded_terminal_state(cli: &Cli, workload: &Workload) -> TerminalState {
    let mut state = TerminalState::new(cli.cols, cli.rows, cli.scrollback_cap);
    feed_bytes_in_chunks(
        &mut state,
        &workload.render_seed,
        canonical_chunk_bytes(cli.chunk_bytes),
        &mut Vec::new(),
    );
    state
}

pub fn feed_bytes_in_chunks(
    state: &mut TerminalState,
    bytes: &[u8],
    chunk_bytes: usize,
    responses: &mut Vec<Vec<u8>>,
) {
    for chunk in bytes.chunks(chunk_bytes) {
        state.feed_terminal_responses_into(chunk, responses);
        black_box(responses.len());
    }
}

pub fn chunk_count(bytes: &[u8], chunk_bytes: usize) -> usize {
    bytes.len().div_ceil(chunk_bytes)
}

pub fn canonical_chunk_bytes(requested: usize) -> usize {
    requested.clamp(1, MAX_FEED_BYTES_PER_CALL)
}

pub const fn scale_name(scale: ScaleArg) -> &'static str {
    match scale {
        ScaleArg::Quick => "quick",
        ScaleArg::Standard => "standard",
        ScaleArg::Stress => "stress",
    }
}

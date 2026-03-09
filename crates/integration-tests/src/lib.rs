use rldyourterm_core::TerminalState;

/// Feed bytes into terminal via the public API.
/// Returns the response payloads (DA1, DSR, etc.) for optional inspection.
pub fn feed(state: &mut TerminalState, bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut responses = Vec::new();
    state.feed_terminal_responses_into(bytes, &mut responses);
    responses
}

/// Feed bytes without caring about responses.
pub fn feed_bytes(state: &mut TerminalState, bytes: &[u8]) {
    let mut responses = Vec::new();
    state.feed_terminal_responses_into(bytes, &mut responses);
}

/// Create a standard terminal for testing (80x24, 1000-line scrollback).
pub fn term() -> TerminalState {
    TerminalState::new(80, 24, 1000)
}

/// Create a terminal with custom dimensions.
pub fn term_sized(cols: u16, rows: u16) -> TerminalState {
    TerminalState::new(cols, rows, 1000)
}

/// Create a terminal with custom dimensions and scrollback cap.
pub fn term_full(cols: u16, rows: u16, scrollback: usize) -> TerminalState {
    TerminalState::new(cols, rows, scrollback)
}

/// Extract the full visible grid content as a multi-line string (trimmed per-row).
pub fn grid_content(state: &TerminalState) -> String {
    let h = state.grid.height();
    let mut lines = Vec::with_capacity(h as usize);
    for row in 0..h {
        if let Ok(s) = state.grid.row_string(row) {
            lines.push(s.trim_end().to_string());
        }
    }
    // Trim trailing empty lines
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

/// Get a single row as a string, trimmed of trailing spaces.
pub fn row(state: &TerminalState, row_idx: u16) -> String {
    state
        .grid
        .row_string(row_idx)
        .unwrap_or_default()
        .trim_end()
        .to_string()
}

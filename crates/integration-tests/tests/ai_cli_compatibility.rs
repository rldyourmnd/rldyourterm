// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use rldyourterm_core::{MouseFormat, MouseMode, TerminalState};
use rldyourterm_integration_tests::{feed, feed_bytes, row, term, term_sized};

// ── Fish shell startup patterns ────────────────────────────

#[test]
fn fish_right_prompt_pattern() {
    // Fish draws right-prompt by: fill to last column, CR, CUF to reposition.
    // This pattern triggered the "staircase bug" when deferred wrap was incorrect.
    let mut t = term_sized(10, 3);
    // Fill row 0 completely
    feed_bytes(&mut t, b"0123456789");
    assert!(t.cursor.wrap_pending);
    assert_eq!(t.cursor.row, 0);
    // CR (fish does this after right prompt)
    feed_bytes(&mut t, b"\r");
    assert_eq!(t.cursor.row, 0); // must NOT wrap to next line
    assert_eq!(t.cursor.col, 0);
    // CUF to col 5 (fish repositions cursor for command line)
    feed_bytes(&mut t, b"\x1b[5C");
    assert_eq!(t.cursor.row, 0);
    assert_eq!(t.cursor.col, 5);
}

#[test]
fn fish_osc_7_cwd_tracking() {
    let mut t = term();
    // Fish sends OSC 7 after each command to report CWD
    feed_bytes(&mut t, b"\x1b]7;file://localhost/home/user/project\x07");
    assert_eq!(t.cwd(), "/home/user/project");
    // Change directory
    feed_bytes(&mut t, b"\x1b]7;file://localhost/tmp\x07");
    assert_eq!(t.cwd(), "/tmp");
}

#[test]
fn fish_osc_133_shell_markers() {
    let mut t = term();
    // Fish sends OSC 133 prompt markers - terminal should not crash
    feed_bytes(&mut t, b"\x1b]133;A\x07");
    feed_bytes(&mut t, b"$ command\r\n");
    feed_bytes(&mut t, b"\x1b]133;C\x07");
    feed_bytes(&mut t, b"output line\r\n");
    feed_bytes(&mut t, b"\x1b]133;D;0\x07");
    // Terminal should still be functional
    assert_eq!(row(&t, 0), "$ command");
    assert_eq!(row(&t, 1), "output line");
}

#[test]
fn fish_da1_query_response() {
    let mut t = term();
    // Fish sends DA1 (Primary Device Attributes) during startup
    let responses = feed(&mut t, b"\x1b[c");
    assert!(!responses.is_empty());
    assert_eq!(responses[0], b"\x1b[?1;2c");
}

// ── Vim / neovim patterns ──────────────────────────────────

#[test]
fn vim_alternate_screen_lifecycle() {
    let mut t = term_sized(20, 5);
    // Write content on main screen
    feed_bytes(&mut t, b"Main screen content");
    assert_eq!(row(&t, 0), "Main screen content");

    // Vim enters alternate screen (CSI ?1049h)
    feed_bytes(&mut t, b"\x1b[?1049h");
    // Alternate screen should be clean
    assert_eq!(row(&t, 0), "");

    // Vim draws UI on alternate screen
    feed_bytes(&mut t, b"Vim UI here");
    assert_eq!(row(&t, 0), "Vim UI here");

    // Vim exits alternate screen (CSI ?1049l)
    feed_bytes(&mut t, b"\x1b[?1049l");
    // Main screen content should be restored
    assert_eq!(row(&t, 0), "Main screen content");
}

#[test]
fn vim_mouse_mode_enable_disable() {
    let mut t = term();
    assert_eq!(t.mouse_mode(), MouseMode::Off);
    // Vim enables mouse tracking + SGR encoding
    feed_bytes(&mut t, b"\x1b[?1000h\x1b[?1006h");
    assert_eq!(t.mouse_mode(), MouseMode::Basic);
    assert_eq!(t.mouse_format(), MouseFormat::Sgr);
    // Vim disables on exit
    feed_bytes(&mut t, b"\x1b[?1006l\x1b[?1000l");
    assert_eq!(t.mouse_mode(), MouseMode::Off);
    assert_eq!(t.mouse_format(), MouseFormat::Normal);
}

#[test]
fn vim_scroll_region_editing() {
    let mut t = term_sized(10, 5);
    // Fill grid with identifiable rows (5 rows + 5 \r\n = last \r\n scrolls row A off)
    for row_idx in 0..5u16 {
        let ch = (b'A' + row_idx as u8) as char;
        feed_bytes(
            &mut t,
            format!("{}\r\n", ch.to_string().repeat(10)).as_bytes(),
        );
    }
    // Grid: [B*10, C*10, D*10, E*10, ""] - A scrolled to scrollback
    // Set scroll region (rows 2-4, 1-indexed = rows 1-3 zero-indexed)
    feed_bytes(&mut t, b"\x1b[2;4r");
    // Move to bottom of region (row 4, 1-indexed = row 3) and newline
    feed_bytes(&mut t, b"\x1b[4;1H\n");
    // Row 0 should be unchanged (outside scroll region)
    assert_eq!(row(&t, 0), "BBBBBBBBBB");
}

#[test]
fn vim_cursor_shape_changes() {
    let mut t = term();
    // Vim changes cursor shape: block (default) -> bar (insert mode)
    assert_eq!(t.cursor_shape(), 0);
    // DECSCUSR 5 = blinking bar
    feed_bytes(&mut t, b"\x1b[5 q");
    assert_eq!(t.cursor_shape(), 5);
    // DECSCUSR 2 = steady block (back to normal mode)
    feed_bytes(&mut t, b"\x1b[2 q");
    assert_eq!(t.cursor_shape(), 2);
    // Reset
    feed_bytes(&mut t, b"\x1b[0 q");
    assert_eq!(t.cursor_shape(), 0);
}

// ── Claude Code / AI streaming output ──────────────────────

#[test]
fn long_streaming_text_with_sgr_colors() {
    let mut t = term();
    // Simulate Claude Code streaming: colored code blocks
    let mut output = Vec::new();
    // Green header
    output.extend_from_slice(b"\x1b[32m# Code Review\x1b[0m\r\n");
    // Code block with syntax highlighting
    output.extend_from_slice(b"\x1b[36mfn \x1b[33mmain\x1b[0m() {\r\n");
    output.extend_from_slice(b"    \x1b[32mprintln!\x1b[0m(\x1b[31m\"hello\"\x1b[0m);\r\n");
    output.extend_from_slice(b"}\r\n");
    feed_bytes(&mut t, &output);

    assert_eq!(row(&t, 0), "# Code Review");
    assert_eq!(row(&t, 1), "fn main() {");
    assert_eq!(row(&t, 2), "    println!(\"hello\");");
    assert_eq!(row(&t, 3), "}");
}

#[test]
fn rapid_line_output_with_scrollback() {
    let mut t = TerminalState::new(40, 5, 100);
    // Simulate AI CLI streaming 50 lines (should scroll and populate scrollback)
    for i in 0..50 {
        feed_bytes(&mut t, format!("Line {:04}\r\n", i).as_bytes());
    }
    // Last 5 lines visible on screen (rows 0-3 have lines 46-49, row 4 is blank cursor row)
    assert_eq!(row(&t, 0), "Line 0046");
    assert_eq!(row(&t, 3), "Line 0049");
    // Scrollback should have earlier lines
    assert!(!t.scrollback.is_empty());
    assert_eq!(t.scrollback.get(0), Some("Line 0000"));
}

#[test]
fn streaming_text_mixed_with_cursor_movement() {
    let mut t = term_sized(40, 5);
    // AI CLI output with status bar updates (cursor movement interleaved)
    feed_bytes(&mut t, b"Processing...\r\n");
    feed_bytes(&mut t, b"Step 1 complete\r\n");
    // Save cursor, move to row 0, update status, restore
    feed_bytes(&mut t, b"\x1b7\x1b[1;1H\x1b[2KProcessing... DONE\x1b8");
    assert_eq!(row(&t, 0), "Processing... DONE");
    assert_eq!(row(&t, 1), "Step 1 complete");
}

// ── Bracketed paste ────────────────────────────────────────

#[test]
fn bracketed_paste_mode_toggle() {
    let mut t = term();
    assert!(!t.bracketed_paste_enabled());
    feed_bytes(&mut t, b"\x1b[?2004h");
    assert!(t.bracketed_paste_enabled());
    feed_bytes(&mut t, b"\x1b[?2004l");
    assert!(!t.bracketed_paste_enabled());
}

// ── tmux / screen patterns ─────────────────────────────────

#[test]
fn synchronized_output_mode() {
    let mut t = term();
    assert!(!t.synchronized_output_enabled());
    // tmux uses synchronized output for flicker-free redraws
    feed_bytes(&mut t, b"\x1b[?2026h");
    assert!(t.synchronized_output_enabled());
    feed_bytes(&mut t, b"\x1b[?2026l");
    assert!(!t.synchronized_output_enabled());
}

// ── Starship prompt patterns ───────────────────────────────

#[test]
fn starship_complex_prompt_rendering() {
    let mut t = term();
    // Starship renders complex prompts with multiple SGR + Unicode
    let prompt = concat!(
        "\x1b[1;32m❯\x1b[0m ",       // green bold arrow
        "\x1b[36m~/project\x1b[0m ", // cyan path
        "\x1b[33m⎇ main\x1b[0m ",    // yellow branch
        "\x1b[31m✗\x1b[0m ",         // red dirty marker
    );
    feed_bytes(&mut t, prompt.as_bytes());
    let content = row(&t, 0);
    assert!(content.contains('❯'));
    assert!(content.contains("~/project"));
    assert!(content.contains("main"));
    assert!(content.contains('✗'));
}

// ── Combined mode interactions ─────────────────────────────

#[test]
fn alternate_screen_preserves_scrollback() {
    let mut t = TerminalState::new(20, 3, 100);
    // Fill scrollback
    for i in 0..10 {
        feed_bytes(&mut t, format!("Line {}\r\n", i).as_bytes());
    }
    let scrollback_before = t.scrollback.len();
    assert!(scrollback_before > 0);

    // Enter alternate screen - main scrollback is saved internally;
    // t.scrollback is now the alt screen's scrollback (cap=0)
    feed_bytes(&mut t, b"\x1b[?1049h");

    // Write on alt screen (scrollback accumulation should not affect main)
    for i in 0..50 {
        feed_bytes(&mut t, format!("Alt {}\r\n", i).as_bytes());
    }

    // Exit alternate screen - main scrollback should be restored
    feed_bytes(&mut t, b"\x1b[?1049l");
    assert_eq!(t.scrollback.len(), scrollback_before);
}

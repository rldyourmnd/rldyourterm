// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use rldyourterm_core::{MouseFormat, MouseMode, TerminalState};
use rldyourterm_integration_tests::{feed, feed_bytes, row, term, term_sized};

// ── Mode stacking ───────────────────────────────────────────

#[test]
fn bracketed_paste_survives_sgr_reset() {
    let mut t = term();
    feed_bytes(&mut t, b"\x1b[?2004h");
    assert!(t.bracketed_paste_enabled());
    // SGR reset should NOT affect private modes
    feed_bytes(&mut t, b"\x1b[0m");
    assert!(t.bracketed_paste_enabled());
}

#[test]
fn mouse_mode_independent_of_mouse_format() {
    let mut t = term();
    // Enable SGR format first
    feed_bytes(&mut t, b"\x1b[?1006h");
    assert_eq!(t.mouse_format(), MouseFormat::Sgr);
    assert_eq!(t.mouse_mode(), MouseMode::Off);
    // Enable basic mouse mode
    feed_bytes(&mut t, b"\x1b[?1000h");
    assert_eq!(t.mouse_mode(), MouseMode::Basic);
    assert_eq!(t.mouse_format(), MouseFormat::Sgr);
    // Disable mouse mode, format should persist
    feed_bytes(&mut t, b"\x1b[?1000l");
    assert_eq!(t.mouse_mode(), MouseMode::Off);
    assert_eq!(t.mouse_format(), MouseFormat::Sgr);
    // Disable format
    feed_bytes(&mut t, b"\x1b[?1006l");
    assert_eq!(t.mouse_format(), MouseFormat::Normal);
}

#[test]
fn mouse_mode_upgrade_path() {
    let mut t = term();
    // Basic -> ButtonTrack -> AnyEvent (upgrade)
    feed_bytes(&mut t, b"\x1b[?1000h");
    assert_eq!(t.mouse_mode(), MouseMode::Basic);
    feed_bytes(&mut t, b"\x1b[?1002h");
    assert_eq!(t.mouse_mode(), MouseMode::ButtonTrack);
    feed_bytes(&mut t, b"\x1b[?1003h");
    assert_eq!(t.mouse_mode(), MouseMode::AnyEvent);
    // Disable AnyEvent, should go to Off (not back to ButtonTrack)
    feed_bytes(&mut t, b"\x1b[?1003l");
    assert_eq!(t.mouse_mode(), MouseMode::Off);
}

// ── Cursor shape lifecycle ──────────────────────────────────

#[test]
fn cursor_shape_all_decscusr_values() {
    let mut t = term();
    // DECSCUSR values: 0=default, 1=blinking block, 2=steady block,
    // 3=blinking underline, 4=steady underline, 5=blinking bar, 6=steady bar
    for shape in 0..=6 {
        feed_bytes(&mut t, format!("\x1b[{} q", shape).as_bytes());
        assert_eq!(t.cursor_shape(), shape, "cursor shape {} failed", shape);
    }
}

#[test]
fn cursor_shape_preserved_through_text_output() {
    let mut t = term();
    feed_bytes(&mut t, b"\x1b[5 q"); // blinking bar
    feed_bytes(&mut t, b"Hello World");
    assert_eq!(t.cursor_shape(), 5);
    feed_bytes(&mut t, b"\x1b[1mBold text\x1b[0m");
    assert_eq!(t.cursor_shape(), 5);
}

// ── Alternate screen + mode interactions ────────────────────

#[test]
fn alternate_screen_saves_restores_cursor_position() {
    let mut t = term_sized(20, 10);
    feed_bytes(&mut t, b"\x1b[5;10H"); // row 4, col 9
    assert_eq!(t.cursor.row, 4);
    assert_eq!(t.cursor.col, 9);
    // Enter alt screen (saves cursor)
    feed_bytes(&mut t, b"\x1b[?1049h");
    assert_eq!(t.cursor.row, 0);
    assert_eq!(t.cursor.col, 0);
    // Move cursor on alt screen
    feed_bytes(&mut t, b"\x1b[3;7H");
    assert_eq!(t.cursor.row, 2);
    assert_eq!(t.cursor.col, 6);
    // Exit alt screen (restores cursor)
    feed_bytes(&mut t, b"\x1b[?1049l");
    assert_eq!(t.cursor.row, 4);
    assert_eq!(t.cursor.col, 9);
}

#[test]
fn alternate_screen_clears_on_enter() {
    let mut t = term_sized(20, 5);
    feed_bytes(&mut t, b"Main content here");
    feed_bytes(&mut t, b"\x1b[?1049h");
    // Alt screen should be clean
    for r in 0..5 {
        assert_eq!(row(&t, r), "", "alt screen row {} should be blank", r);
    }
    feed_bytes(&mut t, b"\x1b[?1049l");
}

#[test]
fn alternate_screen_modes_persist() {
    let mut t = term();
    // Set modes before alt screen
    feed_bytes(&mut t, b"\x1b[?2004h"); // bracketed paste
    feed_bytes(&mut t, b"\x1b[?1004h"); // focus reporting
    // Enter alt screen
    feed_bytes(&mut t, b"\x1b[?1049h");
    // Modes should persist on alt screen
    assert!(t.bracketed_paste_enabled());
    assert!(t.focus_reporting_enabled());
    // Exit
    feed_bytes(&mut t, b"\x1b[?1049l");
    assert!(t.bracketed_paste_enabled());
    assert!(t.focus_reporting_enabled());
    // Clean up
    feed_bytes(&mut t, b"\x1b[?2004l\x1b[?1004l");
}

#[test]
fn alternate_screen_scroll_region_reset_on_exit() {
    let mut t = term_sized(20, 10);
    // Enter alt screen
    feed_bytes(&mut t, b"\x1b[?1049h");
    // Set scroll region on alt screen
    feed_bytes(&mut t, b"\x1b[3;8r");
    // Exit alt screen
    feed_bytes(&mut t, b"\x1b[?1049l");
    // Scroll region should be reset to full screen
    // Verify by scrolling - content should scroll entire grid
    for i in 0..20 {
        feed_bytes(&mut t, format!("Line {}\r\n", i).as_bytes());
    }
    // If scroll region was properly reset, row 0 should have scrolled content
    assert!(row(&t, 0).starts_with("Line"));
}

// ── Tab stops ───────────────────────────────────────────────

#[test]
fn default_tab_stops_every_8_columns() {
    let mut t = term_sized(40, 3);
    feed_bytes(&mut t, b"\t");
    assert_eq!(t.cursor.col, 8);
    feed_bytes(&mut t, b"\t");
    assert_eq!(t.cursor.col, 16);
    feed_bytes(&mut t, b"\t");
    assert_eq!(t.cursor.col, 24);
    feed_bytes(&mut t, b"\t");
    assert_eq!(t.cursor.col, 32);
}

#[test]
fn custom_tab_stop_via_hts() {
    let mut t = term_sized(40, 3);
    // Clear all tab stops (CSI 3g)
    feed_bytes(&mut t, b"\x1b[3g");
    // Set tab stop at col 5 (ESC H = HTS at current cursor position)
    feed_bytes(&mut t, b"\x1b[1;6H\x1bH"); // move to col 5, set tab
    // Set tab stop at col 15
    feed_bytes(&mut t, b"\x1b[1;16H\x1bH");
    // Home cursor and tab
    feed_bytes(&mut t, b"\x1b[H\t");
    assert_eq!(t.cursor.col, 5);
    feed_bytes(&mut t, b"\t");
    assert_eq!(t.cursor.col, 15);
}

#[test]
fn tab_clear_current_position() {
    let mut t = term_sized(40, 3);
    // Default tab at col 8 - verify it works
    feed_bytes(&mut t, b"\t");
    assert_eq!(t.cursor.col, 8);
    // Move to col 8, clear tab there (CSI 0g)
    feed_bytes(&mut t, b"\x1b[1;9H\x1b[0g");
    // Now tab from col 0 should skip col 8 and go to col 16
    feed_bytes(&mut t, b"\x1b[H\t");
    assert_eq!(t.cursor.col, 16);
}

#[test]
fn tab_clear_all() {
    let mut t = term_sized(40, 3);
    // Clear all tabs (CSI 3g)
    feed_bytes(&mut t, b"\x1b[3g");
    // Tab from col 0 should go to last column (no stops)
    feed_bytes(&mut t, b"\x1b[H\t");
    assert_eq!(t.cursor.col, 39);
}

#[test]
fn tab_stops_survive_resize_width_expansion() {
    let mut t = TerminalState::new(20, 5, 100);
    // Set custom tab at col 5
    feed_bytes(&mut t, b"\x1b[3g"); // clear all
    feed_bytes(&mut t, b"\x1b[1;6H\x1bH"); // set at col 5
    // Resize wider
    t.resize(40, 5);
    // Custom tab at col 5 should survive
    feed_bytes(&mut t, b"\x1b[H\t");
    assert_eq!(t.cursor.col, 5);
    // New columns should get default 8-col tabs (col 24, 32 are new)
    feed_bytes(&mut t, b"\t");
    // Next tab after col 5 should be at col 24 (next default stop in new range)
    // or wherever the next stop is
    assert!(t.cursor.col > 5);
}

// ── DSR responses ───────────────────────────────────────────

#[test]
fn dsr_cursor_position_at_boundaries() {
    let mut t = term_sized(10, 5);
    // At origin
    feed_bytes(&mut t, b"\x1b[H");
    let responses = feed(&mut t, b"\x1b[6n");
    assert_eq!(responses[0], b"\x1b[1;1R");
    // At last cell
    feed_bytes(&mut t, b"\x1b[5;10H");
    let responses = feed(&mut t, b"\x1b[6n");
    assert_eq!(responses[0], b"\x1b[5;10R");
}

#[test]
fn da1_response_is_consistent() {
    let mut t = term();
    let r1 = feed(&mut t, b"\x1b[c");
    let r2 = feed(&mut t, b"\x1b[c");
    assert_eq!(r1, r2, "DA1 response must be deterministic");
}

// ── Synchronized output ─────────────────────────────────────

#[test]
fn synchronized_output_toggle() {
    let mut t = term();
    assert!(!t.synchronized_output_enabled());
    feed_bytes(&mut t, b"\x1b[?2026h");
    assert!(t.synchronized_output_enabled());
    // Content should still be accepted during synchronized mode
    feed_bytes(&mut t, b"Synced content");
    assert_eq!(row(&t, 0), "Synced content");
    feed_bytes(&mut t, b"\x1b[?2026l");
    assert!(!t.synchronized_output_enabled());
}

// ── Focus reporting ─────────────────────────────────────────

#[test]
fn focus_reporting_toggle() {
    let mut t = term();
    assert!(!t.focus_reporting_enabled());
    feed_bytes(&mut t, b"\x1b[?1004h");
    assert!(t.focus_reporting_enabled());
    feed_bytes(&mut t, b"\x1b[?1004l");
    assert!(!t.focus_reporting_enabled());
}

// ── Auto-wrap mode ──────────────────────────────────────────

#[test]
fn auto_wrap_disabled_stays_at_last_column() {
    let mut t = term_sized(5, 2);
    // Disable auto-wrap (CSI ?7l)
    feed_bytes(&mut t, b"\x1b[?7l");
    feed_bytes(&mut t, b"ABCDEF");
    // Cursor should stay at last column, 'F' overwrites 'E'
    assert_eq!(t.cursor.col, 4);
    assert_eq!(t.cursor.row, 0);
    assert_eq!(row(&t, 0), "ABCDF");
}

#[test]
fn auto_wrap_enabled_wraps_to_next_line() {
    let mut t = term_sized(5, 2);
    // Auto-wrap should be on by default
    feed_bytes(&mut t, b"ABCDE");
    assert!(t.cursor.wrap_pending);
    feed_bytes(&mut t, b"F");
    // F triggers wrap to next line
    assert_eq!(t.cursor.row, 1);
    assert_eq!(row(&t, 0), "ABCDE");
    assert_eq!(row(&t, 1), "F");
}

// ── Window title ────────────────────────────────────────────

#[test]
fn window_title_osc_0_and_osc_2() {
    let mut t = term();
    // OSC 0 sets both icon name and title
    feed_bytes(&mut t, b"\x1b]0;Title One\x07");
    assert_eq!(t.window_title(), "Title One");
    // OSC 2 sets title only
    feed_bytes(&mut t, b"\x1b]2;Title Two\x07");
    assert_eq!(t.window_title(), "Title Two");
}

#[test]
fn window_title_with_special_characters() {
    let mut t = term();
    feed_bytes(&mut t, b"\x1b]0;~/project (main*)\x07");
    assert_eq!(t.window_title(), "~/project (main*)");
}

// ── DECSC/DECRC cursor save/restore ─────────────────────────

#[test]
fn cursor_save_restore_via_esc_7_8() {
    let mut t = term_sized(20, 10);
    feed_bytes(&mut t, b"\x1b[5;10H"); // row 4, col 9
    feed_bytes(&mut t, b"\x1b7"); // save
    feed_bytes(&mut t, b"\x1b[1;1H"); // move to origin
    assert_eq!(t.cursor.row, 0);
    assert_eq!(t.cursor.col, 0);
    feed_bytes(&mut t, b"\x1b8"); // restore
    assert_eq!(t.cursor.row, 4);
    assert_eq!(t.cursor.col, 9);
}

#[test]
fn cursor_save_restore_preserves_pen_attributes() {
    let mut t = term_sized(20, 5);
    // Set bold + italic, save cursor
    feed_bytes(&mut t, b"\x1b[1;3m\x1b7");
    // Reset attributes, write
    feed_bytes(&mut t, b"\x1b[0mNormal");
    // Restore cursor (should also restore pen)
    feed_bytes(&mut t, b"\x1b8");
    feed_bytes(&mut t, b"Styled");
    let cells = t.grid.row_cells(t.cursor.row).unwrap();
    // Find "Styled" text and check attributes
    let styled_start = t.cursor.col as usize - 6;
    assert!(cells[styled_start].attrs.bold);
    assert!(cells[styled_start].attrs.italic);
}

// ── CSI REP (repeat last char) ──────────────────────────────

#[test]
fn rep_repeats_last_printed_character() {
    let mut t = term_sized(20, 3);
    feed_bytes(&mut t, b"X\x1b[4b"); // print X, then repeat 4 times
    assert_eq!(row(&t, 0), "XXXXX"); // 1 original + 4 repeats
}

#[test]
fn rep_with_no_prior_char_is_noop() {
    let mut t = term_sized(20, 3);
    feed_bytes(&mut t, b"\x1b[5b"); // repeat with nothing printed
    assert_eq!(row(&t, 0), ""); // no effect
}

#[test]
fn rep_after_csi_uses_last_printed_char() {
    let mut t = term_sized(20, 3);
    feed_bytes(&mut t, b"A\x1b[1;1H\x1b[3b"); // print A, home, repeat 3
    // REP should repeat 'A' 3 times from cursor position
    assert_eq!(row(&t, 0), "AAA");
}

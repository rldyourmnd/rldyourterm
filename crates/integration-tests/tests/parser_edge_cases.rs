// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use rldyourterm_integration_tests::{feed, feed_bytes, row, term, term_sized};

// ── Truncated and malformed CSI sequences ──────────────────

#[test]
fn truncated_csi_discarded_cleanly() {
    let mut t = term_sized(10, 2);
    // Incomplete CSI (no final byte) followed by normal text
    feed_bytes(&mut t, b"\x1b[31ABC");
    // '3', '1' are CSI params, 'A' is CUU (cursor up) which completes the CSI,
    // then 'B', 'C' are printed
    assert_eq!(row(&t, 0), "BC");
}

#[test]
fn csi_with_too_many_params_is_discarded() {
    let mut t = term_sized(10, 2);
    // CSI with 40 numeric parameters (exceeds MAX_CSI_PARAMS = 32)
    let mut seq = b"\x1b[".to_vec();
    for i in 0..40 {
        if i > 0 {
            seq.push(b';');
        }
        seq.extend_from_slice(b"1");
    }
    seq.push(b'm'); // SGR final byte
    seq.extend_from_slice(b"OK");
    feed_bytes(&mut t, &seq);
    // Oversized CSI should be discarded, "OK" should still print
    assert_eq!(row(&t, 0), "OK");
}

#[test]
fn csi_with_large_numeric_param_clamped() {
    let mut t = term_sized(10, 5);
    // CSI with param 99999 for cursor down - should clamp to grid boundary
    feed_bytes(&mut t, b"\x1b[99999B");
    assert_eq!(t.cursor.row, 4); // clamped to last row
}

#[test]
fn empty_csi_params_use_defaults() {
    let mut t = term_sized(10, 5);
    t.cursor.row = 2;
    t.cursor.col = 5;
    // CSI H with no params = home cursor (1;1 default)
    feed_bytes(&mut t, b"\x1b[H");
    assert_eq!(t.cursor.row, 0);
    assert_eq!(t.cursor.col, 0);
}

#[test]
fn csi_with_only_semicolons_uses_defaults() {
    let mut t = term_sized(10, 5);
    // CSI ;;H = same as CSI 1;1H (defaults for missing params)
    feed_bytes(&mut t, b"\x1b[;;H");
    assert_eq!(t.cursor.row, 0);
    assert_eq!(t.cursor.col, 0);
}

#[test]
fn escape_within_csi_aborts_csi() {
    let mut t = term_sized(20, 2);
    // Start CSI, then interrupt with ESC. Parser aborts first CSI,
    // processes ESC+[ as new CSI intro, but intermediate bytes from the
    // interrupted state cause the rest to print as text.
    feed_bytes(&mut t, b"\x1b[1\x1b[31mHello");
    let content = row(&t, 0);
    // The important invariant: "Hello" must appear, terminal must not crash.
    assert!(
        content.contains("Hello"),
        "Hello must be visible after ESC-interrupted CSI"
    );
}

// ── UTF-8 boundary conditions ──────────────────────────────

#[test]
fn incomplete_utf8_produces_replacement() {
    let mut t = term_sized(10, 2);
    // Incomplete 3-byte UTF-8 sequence (missing last byte) followed by ASCII
    feed_bytes(&mut t, &[0xE4, 0xB8, b'X']);
    // Parser should handle gracefully (replacement char or skip + X)
    let content = row(&t, 0);
    assert!(content.contains('X'));
}

#[test]
fn utf8_split_across_feeds() {
    let mut t = term_sized(10, 2);
    // U+4E16 (世) = E4 B8 96 - split across two feeds
    feed_bytes(&mut t, &[0xE4, 0xB8]);
    feed_bytes(&mut t, &[0x96]);
    assert_eq!(row(&t, 0), "世");
}

#[test]
fn mixed_ascii_and_cjk() {
    let mut t = term_sized(20, 2);
    feed_bytes(&mut t, "Hello世界Test".as_bytes());
    assert_eq!(row(&t, 0), "Hello世界Test");
    // 5 + 2 + 2 + 4 = 13 columns
    assert_eq!(t.cursor.col, 13);
}

// ── OSC edge cases ─────────────────────────────────────────

#[test]
fn osc_terminated_by_bel() {
    let mut t = term();
    feed_bytes(&mut t, b"\x1b]0;My Title\x07");
    assert_eq!(t.window_title(), "My Title");
}

#[test]
fn osc_terminated_by_st() {
    let mut t = term();
    feed_bytes(&mut t, b"\x1b]0;ST Title\x1b\\");
    assert_eq!(t.window_title(), "ST Title");
}

#[test]
fn osc_with_empty_payload() {
    let mut t = term();
    feed_bytes(&mut t, b"\x1b]0;First\x07");
    assert_eq!(t.window_title(), "First");
    // Empty title
    feed_bytes(&mut t, b"\x1b]0;\x07");
    assert_eq!(t.window_title(), "");
}

#[test]
fn osc_7_file_uri_extracts_path() {
    let mut t = term();
    feed_bytes(&mut t, b"\x1b]7;file://hostname/home/user/project\x07");
    assert_eq!(t.cwd(), "/home/user/project");
}

#[test]
fn osc_52_clipboard_set() {
    let mut t = term();
    feed_bytes(&mut t, b"\x1b]52;c;SGVsbG8=\x07");
    let clip = t.take_pending_clipboard();
    assert_eq!(clip, Some(('c', "SGVsbG8=".to_string())));
    // Second take should be None
    assert_eq!(t.take_pending_clipboard(), None);
}

// ── Control character handling ──────────────────────────────

#[test]
fn bell_sets_pending_flag() {
    let mut t = term();
    assert!(!t.take_pending_bell());
    feed_bytes(&mut t, b"\x07");
    assert!(t.take_pending_bell());
    // Consumed
    assert!(!t.take_pending_bell());
}

#[test]
fn backspace_moves_cursor_left() {
    let mut t = term_sized(10, 2);
    feed_bytes(&mut t, b"ABC\x08");
    assert_eq!(t.cursor.col, 2);
}

#[test]
fn backspace_stops_at_column_zero() {
    let mut t = term_sized(10, 2);
    feed_bytes(&mut t, b"\x08\x08\x08");
    assert_eq!(t.cursor.col, 0);
}

#[test]
fn carriage_return_homes_to_column_zero() {
    let mut t = term_sized(10, 2);
    feed_bytes(&mut t, b"Hello\r");
    assert_eq!(t.cursor.col, 0);
    assert_eq!(t.cursor.row, 0);
}

// ── DA1 / DSR response sequences ───────────────────────────

#[test]
fn da1_returns_correct_response() {
    let mut t = term();
    let responses = feed(&mut t, b"\x1b[c");
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0], b"\x1b[?1;2c");
}

#[test]
fn dsr_returns_cursor_position() {
    let mut t = term();
    t.cursor.row = 4;
    t.cursor.col = 9;
    // Need to feed the cursor to position first through CSI
    let responses = feed(&mut t, b"\x1b[6n");
    assert_eq!(responses.len(), 1);
    // 1-based: row=5, col=10
    assert_eq!(responses[0], b"\x1b[5;10R");
}

#[test]
fn device_ok_returns_response() {
    let mut t = term();
    let responses = feed(&mut t, b"\x1b[5n");
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0], b"\x1b[0n");
}

// ── Rapid sequence interleaving ────────────────────────────

#[test]
fn rapid_sgr_mode_cycling() {
    let mut t = term_sized(20, 2);
    // Rapid cycling through SGR attributes
    feed_bytes(&mut t, b"\x1b[1mB\x1b[0m\x1b[3mI\x1b[0m\x1b[4mU\x1b[0mN");
    let cells = t.grid.row_cells(0).unwrap();
    assert!(cells[0].attrs.bold());
    assert!(!cells[0].attrs.italic());
    assert!(cells[1].attrs.italic());
    assert!(!cells[1].attrs.bold());
    assert!(cells[2].attrs.underline());
    assert!(!cells[3].attrs.bold());
    assert!(!cells[3].attrs.italic());
    assert!(!cells[3].attrs.underline());
}

#[test]
fn multiple_sgr_in_one_sequence() {
    let mut t = term_sized(10, 2);
    // SGR 1;3;4m = bold + italic + underline
    feed_bytes(&mut t, b"\x1b[1;3;4mX");
    let cells = t.grid.row_cells(0).unwrap();
    assert!(cells[0].attrs.bold());
    assert!(cells[0].attrs.italic());
    assert!(cells[0].attrs.underline());
}

#[test]
fn sgr_rgb_foreground_and_background() {
    let mut t = term_sized(10, 2);
    use rldyourterm_core::Color;
    // SGR 38;2;255;128;0 (fg orange) + SGR 48;2;0;0;0 (bg black)
    feed_bytes(&mut t, b"\x1b[38;2;255;128;0;48;2;0;0;0mX");
    let cells = t.grid.row_cells(0).unwrap();
    assert_eq!(cells[0].attrs.fg, Color::Rgb(255, 128, 0));
    assert_eq!(cells[0].attrs.bg, Color::Rgb(0, 0, 0));
}

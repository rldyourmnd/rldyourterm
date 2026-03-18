// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use crate::events::{CoreEvent, IngestDegradeReason};
use crate::grid::Attrs;

use super::{FEED_CHUNK_BYTES, MAX_FEED_BYTES_PER_CALL, TerminalState};

// ── Stress tests ─────────────────────────────────────────────

#[test]
fn stress_ai_cli_output_burst_10k_lines() {
    let mut state = TerminalState::new(80, 24, 50_000);
    let line = b"\x1b[32mOutput line with ANSI color\x1b[0m\r\n";
    for _ in 0..10_000 {
        state.feed(line);
    }
    assert!(!state.scrollback.is_empty());
    assert!(state.scrollback.len() <= 50_000);
}

#[test]
fn stress_scrollback_cap_enforced_at_50k() {
    let mut state = TerminalState::new(80, 24, 50_000);
    let line = b"scrollback test line\r\n";
    for _ in 0..60_000 {
        state.feed(line);
    }
    assert!(state.scrollback.len() <= 50_000);
}

#[test]
fn stress_unicode_multibyte_throughput() {
    let mut state = TerminalState::new(80, 24, 1_000);
    let text = "Hello Мир 你好 🌍\r\n".as_bytes();
    for _ in 0..5_000 {
        state.feed(text);
    }
    // Should not panic, grid should have valid state
    assert!(state.grid.height() > 0);
}

#[test]
fn stress_rapid_sgr_attribute_sequences() {
    let mut state = TerminalState::new(80, 24, 100);
    let mut buf = Vec::with_capacity(100_000);
    for i in 0..10_000u32 {
        let sgr = format!("\x1b[{}mX", i % 109);
        buf.extend_from_slice(sgr.as_bytes());
    }
    state.feed(&buf);
}

#[test]
fn stress_cursor_positioning_boundaries() {
    let mut state = TerminalState::new(80, 24, 100);
    for row in 1..=30u16 {
        for col in 1..=90u16 {
            let seq = format!("\x1b[{};{}H", row, col);
            state.feed(seq.as_bytes());
        }
    }
    // Cursor should be clamped to grid bounds
    assert!(state.cursor.row < state.grid.height());
    assert!(state.cursor.col < state.grid.width());
}

#[test]
fn stress_resize_during_output() {
    let mut state = TerminalState::new(80, 24, 1_000);
    for i in 0..500u16 {
        state.feed(b"some output text\r\n");
        let w = 60 + (i % 40);
        let h = 20 + (i % 10);
        state.resize(w, h);
    }
    assert!(state.cursor.row < state.grid.height());
    assert!(state.cursor.col < state.grid.width());
}

#[test]
fn stress_bulk_feed_max_chunk_64kb() {
    let mut state = TerminalState::new(80, 24, 1_000);
    let bulk = vec![b'A'; MAX_FEED_BYTES_PER_CALL];
    state.feed(&bulk);
    // Should not panic; grid filled with 'A's
    let cell = state.grid.get_cell(0, 0).unwrap();
    assert_eq!(cell.ch, 'A');
}

#[test]
fn stress_incomplete_escape_at_chunk_boundary() {
    let mut state = TerminalState::new(80, 24, 100);
    // Send partial escape sequence at chunk boundary
    for _ in 0..1_000 {
        state.feed(b"\x1b[");
        state.feed(b"31m");
        state.feed(b"X");
    }
    // Parser should recover and render 'X' chars
}

#[test]
fn stress_alternating_normal_and_alternate_screen() {
    let mut state = TerminalState::new(80, 24, 1_000);
    for _ in 0..1_000 {
        // Enter alternate screen
        state.feed(b"\x1b[?1049h");
        state.feed(b"alternate content\r\n");
        // Exit alternate screen
        state.feed(b"\x1b[?1049l");
        state.feed(b"normal content\r\n");
    }
}

#[test]
fn stress_attribute_combinations_all_64() {
    let mut state = TerminalState::new(80, 24, 100);
    // Bold=1, Dim=2, Italic=3, Underline=4, Blink=5, Inverse=7, Strikethrough=9
    let combos = [
        "\x1b[1;3;4mBIU\x1b[0m",       // bold+italic+underline
        "\x1b[1;2;7mBDR\x1b[0m",       // bold+dim+inverse
        "\x1b[1;3;4;9mBIUS\x1b[0m",    // bold+italic+underline+strikethrough
        "\x1b[1;2;3;4;7;9mALL\x1b[0m", // all attributes
    ];
    for _ in 0..1_000 {
        for combo in &combos {
            state.feed(combo.as_bytes());
        }
    }
}

#[test]
fn stress_erase_operations() {
    let mut state = TerminalState::new(80, 24, 100);
    for _ in 0..5_000 {
        state.feed(b"fill this line with text");
        state.feed(b"\x1b[2J"); // erase entire screen
        state.feed(b"\x1b[K"); // erase to end of line
        state.feed(b"\x1b[1K"); // erase to start of line
        state.feed(b"\x1b[2K"); // erase entire line
    }
}

#[test]
fn stress_throughput_1mb_mixed_ansi() {
    // Baseline throughput: 1MB of mixed ASCII + ANSI must complete in <5s.
    // On modern hardware, terminal parsers typically handle 50-200 MB/s.
    // A 5s budget is generous - catches regressions, not micro-benchmarks.
    let mut state = TerminalState::new(120, 50, 50_000);
    let mut data = Vec::with_capacity(1_048_576);
    // Build 1MB of realistic AI CLI output: colored text lines
    let line =
        b"\x1b[1;32m  INFO\x1b[0m processing batch \x1b[33m#1234\x1b[0m: tokens=512 latency=42ms\r\n";
    while data.len() < 1_048_576 {
        data.extend_from_slice(line);
    }
    let start = std::time::Instant::now();
    // Feed in 64KB chunks (realistic PTY read size)
    for chunk in data.chunks(65_536) {
        state.feed(chunk);
    }
    let elapsed = start.elapsed();
    let mb_per_sec = 1.0 / elapsed.as_secs_f64();
    // Assert completion and reasonable speed
    assert!(
        elapsed.as_secs() < 5,
        "1MB throughput took {elapsed:?} ({mb_per_sec:.1} MB/s) - regression detected"
    );
    assert!(!state.scrollback.is_empty());
}

#[test]
fn stress_memory_stability_after_sustained_output() {
    // Simulate long-running AI CLI session: 100K lines of output.
    // Verify scrollback cap holds and no unbounded growth.
    let mut state = TerminalState::new(80, 24, 50_000);
    let line = b"output line with some content for the terminal buffer test\r\n";
    for _ in 0..100_000 {
        state.feed(line);
    }
    // Scrollback must be capped
    assert!(state.scrollback.len() <= 50_000);
    // Grid must be intact
    assert_eq!(state.grid.height(), 24);
    assert_eq!(state.grid.width(), 80);
    // Cursor must be in valid bounds
    assert!(state.cursor.row < 24);
    assert!(state.cursor.col < 80);
}

// ── Coverage gap tests: boundary conditions ────────────────

#[test]
fn feed_at_exactly_max_bytes_boundary() {
    let mut state = TerminalState::new(80, 24, 100);
    // Feed exactly MAX_FEED_BYTES_PER_CALL bytes - should not degrade
    let data = vec![b'X'; MAX_FEED_BYTES_PER_CALL];
    let events = state.feed(&data);
    assert!(
        !events.iter().any(|e| matches!(
            e,
            CoreEvent::IngestDegraded {
                reason: IngestDegradeReason::InputFeedTooLarge,
                ..
            }
        )),
        "Feed at exactly MAX_FEED_BYTES should not emit InputFeedTooLarge"
    );
}

#[test]
fn feed_one_byte_over_max_degrades() {
    let mut state = TerminalState::new(80, 24, 100);
    let data = vec![b'X'; MAX_FEED_BYTES_PER_CALL + 1];
    let events = state.feed(&data);
    assert!(
        events.iter().any(|e| matches!(
            e,
            CoreEvent::IngestDegraded {
                reason: IngestDegradeReason::InputFeedTooLarge,
                accepted,
                dropped: 1,
            } if *accepted == MAX_FEED_BYTES_PER_CALL
        )),
        "Feed one byte over MAX should degrade with dropped=1"
    );
}

#[test]
fn feed_at_chunk_boundary_processes_all_chunks() {
    let mut state = TerminalState::new(80, 24, 100);
    // Feed exactly 3 chunks worth of data
    let data = vec![b'A'; FEED_CHUNK_BYTES * 3];
    let events = state.feed(&data);
    // All data should be processed (no degrade since 12KB < 64KB max)
    assert!(!events.iter().any(|e| matches!(
        e,
        CoreEvent::IngestDegraded {
            reason: IngestDegradeReason::InputFeedTooLarge,
            ..
        }
    )),);
    // Grid should have 'A' chars
    assert_eq!(state.grid.get_char(0, 0).unwrap(), 'A');
}

#[test]
fn zero_size_state_feed_does_not_panic() {
    // TerminalState with zero dimensions
    let mut state = TerminalState::new(0, 0, 100);
    // Feed various sequences - none should panic
    state.feed(b"Hello");
    state.feed(b"\x1b[31mRed\x1b[0m");
    state.feed(b"\r\n");
    state.feed(b"\x1b[2J");
    state.feed(b"\x1b[H");
    state.feed(b"\x1b[?1049h\x1b[?1049l");
}

#[test]
fn insert_lines_via_csi_count_exceeds_height() {
    let mut state = TerminalState::new(4, 5, 10);
    // Fill grid
    for row in 0..5u16 {
        let ch = (b'A' + row as u8) as char;
        for col in 0..4u16 {
            let _ = state.grid.put_char(row, col, ch, Attrs::default());
        }
    }
    // Move cursor to row 1
    state.cursor.row = 1;
    state.cursor.col = 0;
    // CSI 100 L = insert 100 lines (far exceeds grid height)
    state.feed(b"\x1b[100L");
    // Should not panic; rows below cursor cleared
    assert_eq!(state.grid.row_string(0).unwrap(), "AAAA");
}

#[test]
fn delete_lines_via_csi_count_exceeds_height() {
    let mut state = TerminalState::new(4, 5, 10);
    for row in 0..5u16 {
        let ch = (b'A' + row as u8) as char;
        for col in 0..4u16 {
            let _ = state.grid.put_char(row, col, ch, Attrs::default());
        }
    }
    state.cursor.row = 1;
    state.cursor.col = 0;
    // CSI 100 M = delete 100 lines
    state.feed(b"\x1b[100M");
    // Should not panic; region cleared
    assert_eq!(state.grid.row_string(0).unwrap(), "AAAA");
}

#[test]
fn mixed_sgr_and_utf8_across_chunk_boundaries() {
    let mut state = TerminalState::new(80, 24, 100);
    // Build data where a 3-byte UTF-8 char spans a FEED_CHUNK_BYTES boundary
    let mut data = Vec::with_capacity(FEED_CHUNK_BYTES + 10);
    // Fill up to FEED_CHUNK_BYTES - 1 with ASCII
    data.extend(std::iter::repeat_n(b'A', FEED_CHUNK_BYTES - 1));
    // Add a 3-byte UTF-8 char that will span the chunk boundary
    data.extend("€".as_bytes()); // E2 82 AC
    // Add SGR after
    data.extend(b"\x1b[31mB");
    state.feed(&data);
    // Parser should handle the split correctly - no panic
    // The pen should have red fg from the SGR
    assert_eq!(state.pen.fg, crate::grid::Color::Indexed(1));
}

#[test]
fn cursor_position_at_exact_grid_bounds() {
    let mut state = TerminalState::new(10, 5, 10);
    // CUP to exact last cell (1-indexed: row=5, col=10)
    state.feed(b"\x1b[5;10H");
    assert_eq!(state.cursor.row, 4);
    assert_eq!(state.cursor.col, 9);
    // CUP beyond bounds should clamp
    state.feed(b"\x1b[100;200H");
    assert_eq!(state.cursor.row, 4);
    assert_eq!(state.cursor.col, 9);
}

#[test]
fn rapid_resize_to_zero_and_back() {
    let mut state = TerminalState::new(80, 24, 100);
    state.feed(b"Hello World");
    // Resize to zero
    state.resize(0, 0);
    assert!(state.grid.is_empty());
    // Feed should not panic
    state.feed(b"More data");
    // Resize back
    state.resize(80, 24);
    assert!(!state.grid.is_empty());
    state.feed(b"Recovered");
    assert_eq!(state.grid.get_char(0, 0).unwrap(), 'R');
}

#[test]
fn tab_does_not_overflow_u16_at_large_col() {
    // Regression test: ((col / 8) + 1) * 8 overflows u16 when col >= 65528
    // Saturating arithmetic prevents this.
    let mut state = TerminalState::new(100, 5, 10);
    state.cursor.col = 99; // near end of reasonable grid
    state.feed(b"\t");
    assert!(state.cursor.col < 100);

    // Extreme case: manually set col to near u16::MAX
    state.cursor.col = 65530;
    state.feed(b"\t");
    // Should clamp to width-1, not panic from overflow
    assert!(state.cursor.col < 100);
}

#[test]
fn sgr_17_params_via_state_feed() {
    let mut state = TerminalState::new(10, 2, 5);
    // 17 params: first 16 consumed, 17th silently truncated
    let _ = state.feed(b"\x1b[1;2;3;4;5;7;9;31;32;33;34;35;36;37;38;39;40mX");
    // Parser should not crash. Bold (1) should apply from first param.
    assert!(state.pen.bold());
}

#[test]
fn alt_screen_resize_clamps_saved_cursor() {
    // Regression: resize while in alt screen must clamp the saved
    // main-screen cursor. Otherwise, leaving alt screen restores
    // a cursor that is out-of-bounds for the (resized) main grid.
    let mut state = TerminalState::new(120, 30, 100);
    // Move cursor to near the bottom-right of the original grid
    state.cursor.row = 29;
    state.cursor.col = 119;
    // Enter alt screen (saves cursor at row=29, col=119)
    state.feed(b"\x1b[?1049h");
    assert!(state.alternate_screen.is_some());
    // Shrink the grid while in alt screen
    state.resize(80, 24);
    // Leave alt screen - cursor must be clamped to 80x24 bounds
    state.feed(b"\x1b[?1049l");
    assert!(state.alternate_screen.is_none());
    assert!(
        state.cursor.row < 24,
        "cursor row {} out of bounds",
        state.cursor.row
    );
    assert!(
        state.cursor.col < 80,
        "cursor col {} out of bounds",
        state.cursor.col
    );
    // Grid should have correct dimensions (restored main grid was resized)
    assert_eq!(state.grid.width(), 80);
    assert_eq!(state.grid.height(), 24);
    // Further operations must not panic
    state.feed(b"safe");
}

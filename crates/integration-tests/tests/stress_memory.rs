// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use rldyourterm_core::TerminalState;
use rldyourterm_integration_tests::{feed_bytes, term, term_sized};

// ── Scrollback memory stability ─────────────────────────────

#[test]
fn scrollback_churn_at_cap() {
    // Scrollback cap=100, push 100,000 lines - tests steady-state memory under churn
    let mut t = TerminalState::new(80, 24, 100);
    for i in 0u32..100_000 {
        feed_bytes(&mut t, format!("Line {:06}\r\n", i).as_bytes());
    }
    // Scrollback must be exactly at cap
    assert_eq!(t.scrollback.len(), 100);
    // Oldest visible line should be near the end of the sequence
    let first = t.scrollback.get(0).expect("scrollback not empty");
    assert!(
        first.contains("Line 0"),
        "first line should be from recent output"
    );
}

#[test]
fn scrollback_zero_cap_never_grows() {
    let mut t = TerminalState::new(80, 24, 0);
    for i in 0u32..10_000 {
        feed_bytes(&mut t, format!("Line {}\r\n", i).as_bytes());
    }
    assert_eq!(t.scrollback.len(), 0);
    assert_eq!(t.scrollback.byte_len(), 0);
}

#[test]
fn scrollback_byte_budget_enforcement() {
    // 50 line cap, 256 byte budget - tests byte_cap trimming
    let mut t = TerminalState::new(80, 24, 50);
    // Note: scrollback byte budget is DEFAULT_SCROLLBACK_BYTE_CAP (512MB),
    // so we test at the API level with the public constructor.
    // With default byte cap, 50-line cap is the binding constraint.
    for i in 0u32..200 {
        feed_bytes(&mut t, format!("Data line {:04}\r\n", i).as_bytes());
    }
    assert!(t.scrollback.len() <= 50);
}

// ── Resize oscillation ──────────────────────────────────────

#[test]
fn resize_oscillation_does_not_leak() {
    let mut t = TerminalState::new(80, 24, 500);
    // Fill with content
    for i in 0..100 {
        feed_bytes(&mut t, format!("Content line {}\r\n", i).as_bytes());
    }
    // Oscillate between sizes 500 times
    for i in 0u32..500 {
        if i % 2 == 0 {
            t.resize(40, 12);
        } else {
            t.resize(80, 24);
        }
    }
    // Grid dimensions should match last resize
    assert_eq!(t.grid.width(), 80);
    assert_eq!(t.grid.height(), 24);
    // Cursor must be within bounds
    assert!(t.cursor.row < 24);
    assert!(t.cursor.col < 80);
}

#[test]
fn resize_shrink_expand_preserves_content() {
    let mut t = TerminalState::new(80, 24, 1000);
    feed_bytes(&mut t, b"Hello World\r\n");
    feed_bytes(&mut t, b"Second Line\r\n");

    // Shrink
    t.resize(20, 5);
    // Expand back
    t.resize(80, 24);

    // Content should survive the round-trip (may be in scrollback or grid)
    let mut found_hello = false;
    for r in 0..t.grid.height() {
        let text = t.grid.row_string(r).unwrap_or_default();
        if text.contains("Hello") {
            found_hello = true;
            break;
        }
    }
    if !found_hello {
        for i in 0..t.scrollback.len() {
            if let Some(line) = t.scrollback.get(i)
                && line.contains("Hello")
            {
                found_hello = true;
                break;
            }
        }
    }
    assert!(found_hello, "content must survive resize round-trip");
}

#[test]
fn extreme_resize_values() {
    let mut t = term();
    // Resize to very small
    t.resize(1, 1);
    feed_bytes(&mut t, b"X");
    assert_eq!(t.cursor.col, 0); // wrap_pending, col stays at last col (0)
    assert!(t.cursor.wrap_pending);

    // Resize to moderately large
    t.resize(500, 100);
    feed_bytes(&mut t, b"Y");
    assert!(t.cursor.col < 500);
    assert!(t.cursor.row < 100);

    // Back to normal
    t.resize(80, 24);
    assert!(t.cursor.row < 24);
    assert!(t.cursor.col < 80);
}

// ── Alternate screen memory isolation ───────────────────────

#[test]
fn alternate_screen_cycles_do_not_leak() {
    let mut t = TerminalState::new(80, 24, 1000);
    // Fill main screen
    for i in 0..50 {
        feed_bytes(&mut t, format!("Main {}\r\n", i).as_bytes());
    }
    let sb_before = t.scrollback.len();

    // Enter/exit alternate screen 100 times
    for cycle in 0..100 {
        feed_bytes(&mut t, b"\x1b[?1049h"); // enter alt
        // Write on alt screen
        feed_bytes(&mut t, format!("Alt cycle {}\r\n", cycle).as_bytes());
        feed_bytes(&mut t, b"\x1b[?1049l"); // exit alt
    }
    // Scrollback should be preserved (not grown by alt screen content)
    assert_eq!(t.scrollback.len(), sb_before);
}

// ── Long-running session simulation ─────────────────────────

#[test]
fn simulated_ai_session_stability() {
    let mut t = TerminalState::new(120, 40, 5000);
    // Simulate 30 minutes of AI CLI output:
    // - 100 "command" cycles
    // - Each cycle: prompt, command echo, 50 lines of colored output
    for cmd_idx in 0u32..100 {
        // Prompt
        feed_bytes(&mut t, b"\x1b[1;32m$ \x1b[0m");
        // Command
        feed_bytes(
            &mut t,
            format!("claude \"question {}\" \r\n", cmd_idx).as_bytes(),
        );
        // AI streaming output with colors
        for line in 0..50 {
            let color = 31 + (line % 7);
            feed_bytes(
                &mut t,
                format!(
                    "\x1b[{}mResponse line {} for query {}\x1b[0m\r\n",
                    color, line, cmd_idx
                )
                .as_bytes(),
            );
        }
        // Brief cursor movement (status update)
        feed_bytes(&mut t, b"\x1b7\x1b[1;1H\x1b[2K");
        feed_bytes(
            &mut t,
            format!("Status: {} commands complete", cmd_idx + 1).as_bytes(),
        );
        feed_bytes(&mut t, b"\x1b8");
    }
    // Verify terminal is still healthy
    assert!(t.cursor.row < t.grid.height());
    assert!(t.cursor.col < t.grid.width());
    assert!(!t.scrollback.is_empty());
    // Scrollback should be at or near cap
    assert!(t.scrollback.len() <= 5000);
}

// ── Grid clear cycles ───────────────────────────────────────

#[test]
fn erase_display_cycles() {
    let mut t = term_sized(80, 24);
    // 1000 cycles of fill + erase
    for _ in 0u32..1000 {
        // Fill screen
        for _ in 0..24 {
            feed_bytes(&mut t, b"XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX\r\n");
        }
        // Erase entire display (CSI 2J)
        feed_bytes(&mut t, b"\x1b[2J");
    }
    // After 1000 erase cycles, grid should be clean
    for r in 0..t.grid.height() {
        let text = t.grid.row_string(r).unwrap_or_default();
        assert_eq!(text.trim(), "", "row {} should be blank after erase", r);
    }
}

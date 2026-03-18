// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use rldyourterm_core::TerminalState;
use rldyourterm_integration_tests::{feed_bytes, row};

// ── Cap enforcement ─────────────────────────────────────────

#[test]
fn scrollback_cap_exact_boundary() {
    let mut t = TerminalState::new(40, 10, 50);
    // Push exactly 50 + grid height lines to fill scrollback to cap
    for i in 0..60 {
        feed_bytes(&mut t, format!("Line {:03}\r\n", i).as_bytes());
    }
    assert!(t.scrollback.len() <= 50);
}

#[test]
fn scrollback_cap_one_never_exceeds() {
    let mut t = TerminalState::new(40, 5, 1);
    for i in 0u32..1000 {
        feed_bytes(&mut t, format!("L{}\r\n", i).as_bytes());
    }
    assert_eq!(t.scrollback.len(), 1);
    // The single retained line should be recent
    let line = t.scrollback.get_text(0).expect("one line retained");
    assert!(line.starts_with("L"), "line should start with L prefix");
}

#[test]
fn scrollback_fifo_ordering() {
    let mut t = TerminalState::new(40, 5, 10);
    for i in 0u32..20 {
        feed_bytes(&mut t, format!("Line {:02}\r\n", i).as_bytes());
    }
    // With cap=10 and 5-row grid, older lines evicted first
    // Verify FIFO ordering: lines should be in ascending order
    let mut prev_num = None;
    for i in 0..t.scrollback.len() {
        let line = t.scrollback.get_text(i).unwrap();
        if let Some(num_str) = line.strip_prefix("Line ")
            && let Ok(num) = num_str.trim().parse::<u32>()
        {
            if let Some(prev) = prev_num {
                assert!(
                    num > prev,
                    "scrollback must be FIFO ordered: {} should be > {}",
                    num,
                    prev
                );
            }
            prev_num = Some(num);
        }
    }
    assert!(
        prev_num.is_some(),
        "scrollback should contain numbered lines"
    );
}

// ── Scrollback content integrity ────────────────────────────

#[test]
fn scrollback_preserves_unicode_content() {
    let mut t = TerminalState::new(40, 3, 100);
    let lines = [
        "Hello World",
        "Привет Мир",
        "你好世界",
        "🚀🌍🎉",
        "Mixed: ABC日本語DEF",
    ];
    for line in &lines {
        feed_bytes(&mut t, format!("{}\r\n", line).as_bytes());
    }
    // First lines should be in scrollback (grid only shows last 3 rows)
    let mut found = Vec::new();
    for i in 0..t.scrollback.len() {
        found.push(t.scrollback.get_text(i).unwrap());
    }
    // At least the first few lines should be in scrollback
    assert!(!found.is_empty(), "scrollback should have Unicode content");
    // Verify no mojibake - check a known Unicode line exists in scrollback or grid
    let all_text: String = found.join("\n");
    let grid_text: String = (0..3).map(|r| row(&t, r)).collect::<Vec<_>>().join("\n");
    let combined = format!("{}\n{}", all_text, grid_text);
    for line in &lines {
        assert!(
            combined.contains(line),
            "must preserve Unicode line: {}",
            line
        );
    }
}

#[test]
fn scrollback_preserves_trimmed_content() {
    let mut t = TerminalState::new(20, 3, 100);
    // Lines with trailing spaces (common from grid row_string)
    feed_bytes(&mut t, b"Hello\r\n");
    feed_bytes(&mut t, b"World\r\n");
    feed_bytes(&mut t, b"Third\r\n");
    feed_bytes(&mut t, b"Fourth\r\n");
    // Scrollback lines should be trimmed
    if !t.scrollback.is_empty() {
        let line = t.scrollback.get_text(0).unwrap();
        assert_eq!(
            line,
            line.trim_end(),
            "scrollback should trim trailing spaces"
        );
    }
}

// ── Rapid push/access interleaving ──────────────────────────

#[test]
fn interleaved_push_and_read() {
    let mut t = TerminalState::new(40, 5, 100);
    for i in 0u32..500 {
        feed_bytes(&mut t, format!("Event {:04}\r\n", i).as_bytes());
        // Read scrollback during push cycle
        if i % 50 == 0 && !t.scrollback.is_empty() {
            let first = t.scrollback.get_text(0).unwrap();
            assert!(
                first.starts_with("Event"),
                "scrollback content must be valid during interleaved access"
            );
            let last_idx = t.scrollback.len() - 1;
            let last = t.scrollback.get_text(last_idx).unwrap();
            assert!(
                last.starts_with("Event"),
                "last scrollback line must be valid"
            );
        }
    }
}

#[test]
fn scrollback_iter_during_churn() {
    let mut t = TerminalState::new(40, 5, 50);
    for i in 0u32..200 {
        feed_bytes(&mut t, format!("Item {}\r\n", i).as_bytes());
    }
    // Iterate entire scrollback
    let count = t.scrollback.iter().count();
    assert_eq!(count, t.scrollback.len());
    // All items should be valid strings
    for i in 0..t.scrollback.len() {
        let line = t.scrollback.get_text(i).unwrap();
        assert!(line.starts_with("Item"), "each line should be valid");
    }
}

// ── Scrollback + alternate screen ───────────────────────────

#[test]
fn scrollback_frozen_during_alternate_screen() {
    let mut t = TerminalState::new(40, 5, 100);
    // Fill scrollback
    for i in 0..20 {
        feed_bytes(&mut t, format!("Main {}\r\n", i).as_bytes());
    }
    let sb_len_before = t.scrollback.len();
    assert!(sb_len_before > 0);

    // Enter alternate screen (main scrollback saved internally, t.scrollback = alt cap=0)
    feed_bytes(&mut t, b"\x1b[?1049h");
    // Write lots of content on alt screen
    for i in 0..100 {
        feed_bytes(&mut t, format!("Alt {}\r\n", i).as_bytes());
    }

    // Exit alternate screen - main scrollback restored
    feed_bytes(&mut t, b"\x1b[?1049l");
    assert_eq!(t.scrollback.len(), sb_len_before);
}

// ── Edge: empty and whitespace lines ────────────────────────

#[test]
fn scrollback_handles_empty_lines() {
    let mut t = TerminalState::new(40, 3, 100);
    // Mix of empty and content lines
    feed_bytes(&mut t, b"\r\n\r\n\r\n");
    feed_bytes(&mut t, b"Content\r\n");
    feed_bytes(&mut t, b"\r\n\r\n");
    feed_bytes(&mut t, b"More\r\n");

    // Should handle empty lines without issues
    for i in 0..t.scrollback.len() {
        let _line = t.scrollback.get_text(i).unwrap();
        // No panic = success
    }
}

#[test]
fn scrollback_with_long_lines() {
    let mut t = TerminalState::new(20, 3, 100);
    // Line that wraps across multiple rows
    let long_text = "A".repeat(100);
    feed_bytes(&mut t, long_text.as_bytes());
    feed_bytes(&mut t, b"\r\n");
    // Force some scrolling
    for _ in 0..10 {
        feed_bytes(&mut t, b"Short\r\n");
    }
    // Scrollback should contain wrapped content
    assert!(!t.scrollback.is_empty());
}

// ── Scrollback clear ────────────────────────────────────────

#[test]
fn scrollback_clear_via_erase_display_3() {
    let mut t = TerminalState::new(40, 5, 100);
    for i in 0..50 {
        feed_bytes(&mut t, format!("Line {}\r\n", i).as_bytes());
    }
    assert!(!t.scrollback.is_empty());
    // CSI 3J = erase scrollback (xterm extension)
    feed_bytes(&mut t, b"\x1b[3J");
    assert_eq!(t.scrollback.len(), 0);
    assert_eq!(t.scrollback.byte_len(), 0);
}

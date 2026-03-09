use rldyourterm_core::TerminalState;
use rldyourterm_integration_tests::{feed_bytes, row, term, term_sized};

// ── High-volume ASCII ingest ────────────────────────────────

#[test]
fn ingest_10mb_ascii_stream() {
    let mut t = term();
    // 10 MB of printable ASCII with periodic newlines (simulates AI CLI streaming)
    let chunk_size = 64 * 1024; // 64 KB per feed (matches MAX_FEED_BYTES_PER_CALL)
    let total_bytes = 10 * 1024 * 1024;
    let mut chunk = Vec::with_capacity(chunk_size);
    for i in 0..chunk_size {
        if i % 80 == 79 {
            chunk.push(b'\n');
        } else {
            chunk.push(b'A' + (i % 26) as u8);
        }
    }
    let feeds = total_bytes / chunk_size;
    for _ in 0..feeds {
        feed_bytes(&mut t, &chunk);
    }
    // Terminal must remain functional after 10 MB
    feed_bytes(&mut t, b"ALIVE\r\n");
    assert!(t.grid.height() > 0);
    assert!(t.scrollback.len() > 0);
}

#[test]
fn ingest_1mb_mixed_csi_and_text() {
    let mut t = term();
    // Interleaved SGR color changes and text (common in AI CLI colored output)
    let mut payload = Vec::with_capacity(1024 * 1024);
    for i in 0..50_000 {
        // SGR set foreground color
        let color = (i % 256) as u8;
        payload.extend_from_slice(b"\x1b[38;5;");
        payload.extend_from_slice(color.to_string().as_bytes());
        payload.push(b'm');
        // Short text
        payload.extend_from_slice(b"text ");
        if i % 16 == 15 {
            payload.extend_from_slice(b"\r\n");
        }
    }
    // Feed in 64 KB chunks
    for chunk in payload.chunks(64 * 1024) {
        feed_bytes(&mut t, chunk);
    }
    // SGR reset
    feed_bytes(&mut t, b"\x1b[0mDONE\r\n");
    // Verify terminal is still responsive
    let found = (0..t.grid.height()).any(|r| row(&t, r).contains("DONE"));
    assert!(found, "terminal must display DONE after 1MB mixed CSI+text");
}

#[test]
fn rapid_small_feeds() {
    let mut t = term();
    // 100,000 single-byte feeds (worst case for per-feed overhead)
    for i in 0u32..100_000 {
        let byte = b'A' + (i % 26) as u8;
        feed_bytes(&mut t, &[byte]);
    }
    // Terminal must not crash or corrupt state
    assert!(t.cursor.col < t.grid.width());
    assert!(t.cursor.row < t.grid.height());
}

#[test]
fn rapid_newline_flood() {
    let mut t = TerminalState::new(80, 24, 1000);
    // 50,000 newlines - tests scroll performance and scrollback push
    let newlines = "\n".repeat(50_000);
    for chunk in newlines.as_bytes().chunks(64 * 1024) {
        feed_bytes(&mut t, chunk);
    }
    // Scrollback should be at cap
    assert_eq!(t.scrollback.len(), t.scrollback.cap());
    // Cursor should be at bottom
    assert_eq!(t.cursor.row, t.grid.height() - 1);
}

// ── Full-screen redraw stress ───────────────────────────────

#[test]
fn repeated_full_screen_redraws() {
    let mut t = term_sized(80, 24);
    // Simulate 1000 full-screen redraws (common in TUI apps)
    for cycle in 0u32..1000 {
        // Home cursor
        feed_bytes(&mut t, b"\x1b[H");
        // Fill every row
        for row_idx in 0..24 {
            let line = format!(
                "Row {:02} Cycle {:04}{}\r\n",
                row_idx,
                cycle,
                " ".repeat(80 - 17)
            );
            feed_bytes(&mut t, line.as_bytes());
        }
    }
    // After 1000 redraws, last cycle content should be visible
    assert!(row(&t, 0).contains("Cycle 0999"));
}

#[test]
fn scroll_region_stress() {
    let mut t = term_sized(40, 10);
    // Set scroll region rows 3-8 (1-indexed: 4-9)
    feed_bytes(&mut t, b"\x1b[4;9r");
    // Write outside region
    feed_bytes(&mut t, b"\x1b[1;1HHeader");
    feed_bytes(&mut t, b"\x1b[10;1HFooter");
    // Flood the scroll region with 500 lines
    feed_bytes(&mut t, b"\x1b[4;1H");
    for i in 0u32..500 {
        feed_bytes(&mut t, format!("Line {:03}\r\n", i).as_bytes());
    }
    // Header and Footer should be untouched (outside scroll region)
    assert_eq!(row(&t, 0), "Header");
    assert_eq!(row(&t, 9), "Footer");
}

// ── CSI sequence throughput ─────────────────────────────────

#[test]
fn cursor_movement_storm() {
    let mut t = term_sized(80, 24);
    // 10,000 random cursor movements
    for i in 0u32..10_000 {
        let r = (i % 24) + 1;
        let c = (i % 80) + 1;
        let seq = format!("\x1b[{};{}H", r, c);
        feed_bytes(&mut t, seq.as_bytes());
    }
    // Cursor should be valid
    assert!(t.cursor.row < t.grid.height());
    assert!(t.cursor.col < t.grid.width());
}

#[test]
fn sgr_attribute_cycling_throughput() {
    let mut t = term_sized(80, 24);
    // Cycle through all SGR attributes rapidly
    let sgr_codes = [
        "1", "2", "3", "4", "5", "7", "8", "9", "21", "53", // on
        "22", "23", "24", "25", "27", "28", "29", "24", "55", // off
        "0",  // reset
    ];
    for _ in 0..5_000 {
        for code in &sgr_codes {
            let seq = format!("\x1b[{}mX", code);
            feed_bytes(&mut t, seq.as_bytes());
        }
    }
    // Final reset
    feed_bytes(&mut t, b"\x1b[0m");
    // Grid should contain data
    assert!(t.grid.has_dirty_rows());
}

// ── Wide character throughput ───────────────────────────────

#[test]
fn cjk_flood_10k_characters() {
    let mut t = term_sized(80, 24);
    // Feed 10,000 CJK characters (each occupies 2 columns)
    let cjk_text: String = (0..10_000)
        .map(|i| {
            // Cycle through common CJK range U+4E00..U+9FFF
            char::from_u32(0x4E00 + (i % 0x5200) as u32).unwrap_or('\u{4E00}')
        })
        .collect();
    for chunk in cjk_text.as_bytes().chunks(64 * 1024) {
        feed_bytes(&mut t, chunk);
    }
    // Terminal must still function
    assert!(t.cursor.row < t.grid.height());
    // Scrollback should have content from CJK scrolling
    assert!(t.scrollback.len() > 0);
}

#[test]
fn mixed_width_character_flood() {
    let mut t = term_sized(40, 10);
    // Mix ASCII (width 1) and CJK (width 2) in rapid succession
    for i in 0u32..5000 {
        if i % 3 == 0 {
            feed_bytes(&mut t, "漢".as_bytes());
        } else if i % 3 == 1 {
            feed_bytes(&mut t, b"A");
        } else {
            feed_bytes(&mut t, b"\r\n");
        }
    }
    assert!(t.cursor.row < t.grid.height());
    assert!(t.cursor.col < t.grid.width());
}

// ── Escape sequence burst ───────────────────────────────────

#[test]
fn osc_title_update_burst() {
    let mut t = term();
    // 10,000 title updates (OSC 0)
    for i in 0u32..10_000 {
        let seq = format!("\x1b]0;Title {}\x07", i);
        feed_bytes(&mut t, seq.as_bytes());
    }
    assert_eq!(t.window_title(), "Title 9999");
}

#[test]
fn da1_response_burst() {
    use rldyourterm_integration_tests::feed;
    let mut t = term();
    // 1000 DA1 queries - each should produce a response
    for _ in 0..1000 {
        let responses = feed(&mut t, b"\x1b[c");
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0], b"\x1b[?1;2c");
    }
}

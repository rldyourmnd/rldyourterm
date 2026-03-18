// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use std::time::Instant;

use rldyourterm_core::TerminalState;
use rldyourterm_integration_tests::{feed_bytes, row, term_full, term_sized};

// -- CJK / Wide Characters ---------------------------------------------------

#[test]
fn cjk_flood_fills_grid_correctly() {
    let mut t = TerminalState::new(80, 24, 1000);
    // 10K CJK chars; each occupies 2 columns -> 40 per 80-col row -> 250 rows needed
    let cjk_text: String = (0..10_000)
        .map(|i| char::from_u32(0x4E00 + (i % 0x5200) as u32).unwrap_or('\u{4E00}'))
        .collect();
    feed_bytes(&mut t, cjk_text.as_bytes());

    // Terminal must remain functional
    assert!(t.cursor.row < t.grid.height());
    assert!(t.cursor.col < t.grid.width());

    // Scrollback must have received overflow lines (250 rows needed, 24 visible)
    assert!(
        !t.scrollback.is_empty(),
        "CJK flood must push to scrollback"
    );

    // Verify a visible row contains CJK content
    let last_row = row(&t, t.cursor.row);
    assert!(
        last_row
            .chars()
            .any(|c| (0x4E00..=0x9FFF).contains(&(c as u32))),
        "visible grid must contain CJK characters"
    );
}

#[test]
fn cjk_at_last_column_wraps() {
    // 5-col terminal: fill 4 cols with ASCII, then a wide char (needs 2 cols, only 1 left)
    let mut t = TerminalState::new(5, 3, 100);
    feed_bytes(&mut t, b"ABCD");
    feed_bytes(&mut t, "\u{6F22}".as_bytes()); // U+6F22 (width 2)

    // Row 0: "ABCD" (5th col stays blank because wide char cannot fit)
    assert_eq!(row(&t, 0), "ABCD");
    // Row 1: wide char starts at col 0
    assert_eq!(row(&t, 1), "\u{6F22}");
}

#[test]
fn cjk_mixed_with_ascii_alignment() {
    let mut t = term_sized(20, 5);
    // Pattern: CJK (2 cols) + ASCII (1 col) repeated -> 3 cols per unit
    // 6 units = 18 cols, fits in 20-col row with 2 cols spare
    let mut payload = String::new();
    for i in 0..6 {
        let cjk = char::from_u32(0x4E00 + i as u32).unwrap();
        payload.push(cjk);
        payload.push((b'A' + (i % 26) as u8) as char);
    }
    feed_bytes(&mut t, payload.as_bytes());

    let row_text = row(&t, 0);
    assert_eq!(row_text, payload, "mixed CJK+ASCII must align correctly");

    // Verify cell widths are correct
    let cells = t.grid.row_cells(0).unwrap();
    // First cell: CJK (width 2)
    assert_eq!(cells[0].width, 2);
    // Second cell: continuation (width 0)
    assert_eq!(cells[1].width, 0);
    // Third cell: ASCII (width 1)
    assert_eq!(cells[2].width, 1);
}

#[test]
fn cjk_overwrite_by_ascii_clears_continuation() {
    let mut t = term_sized(10, 2);
    // Place a wide character at col 0-1
    feed_bytes(&mut t, "\u{6F22}".as_bytes());
    assert_eq!(t.grid.row_cells(0).unwrap()[0].width, 2);
    assert_eq!(t.grid.row_cells(0).unwrap()[1].width, 0);

    // Move cursor back to col 0 and overwrite with ASCII
    feed_bytes(&mut t, b"\x1b[1;1H");
    feed_bytes(&mut t, b"X");

    let cells = t.grid.row_cells(0).unwrap();
    assert_eq!(cells[0].ch, 'X');
    assert_eq!(cells[0].width, 1);
    // Continuation cell must be cleared (not left as dangling width-0)
    assert_eq!(cells[1].ch, ' ');
    assert_eq!(cells[1].width, 1);
}

#[test]
fn wide_char_erase_in_line_preserves_neighbors() {
    let mut t = term_sized(10, 2);
    // "A漢B" = A(1) + 漢(2) + B(1) = 4 cols, placed at cols 0-3
    feed_bytes(&mut t, "A\u{6F22}B".as_bytes());

    // Move cursor to col 1 (inside the wide char's owning cell area)
    // CSI 0K erases from cursor to end of line
    feed_bytes(&mut t, b"\x1b[1;2H\x1b[0K");

    let cells = t.grid.row_cells(0).unwrap();
    // Col 0 ('A') must be preserved
    assert_eq!(cells[0].ch, 'A');
    assert_eq!(cells[0].width, 1);
}

// -- Emoji --------------------------------------------------------------------

#[test]
fn emoji_presentation_sequences() {
    let mut t = term_sized(40, 5);
    // Basic emoji (typically width 2 in terminals)
    let emojis = [
        "\u{1F680}", // rocket
        "\u{1F30D}", // globe
        "\u{1F389}", // party popper
        "\u{2764}",  // red heart
    ];
    let text: String = emojis.join("");
    feed_bytes(&mut t, text.as_bytes());

    // Verify all emoji are stored in the grid
    let row_text = row(&t, 0);
    for emoji in &emojis {
        assert!(
            row_text.contains(emoji),
            "grid must contain emoji: {}",
            emoji
        );
    }
}

#[test]
fn emoji_at_line_boundary() {
    // 5-col terminal; fill 4 with ASCII, then emoji (width 2)
    let mut t = TerminalState::new(5, 3, 100);
    feed_bytes(&mut t, b"ABCD");
    feed_bytes(&mut t, "\u{1F680}".as_bytes()); // rocket emoji, width 2

    // Should wrap just like CJK wide chars
    let r0 = row(&t, 0);
    let r1 = row(&t, 1);
    assert!(!r0.contains('\u{1F680}'), "emoji should not fit on row 0");
    assert!(
        r1.contains('\u{1F680}'),
        "emoji must wrap to row 1: got '{}'",
        r1
    );
}

// -- Combining Characters -----------------------------------------------------

#[test]
fn combining_diacritical_marks() {
    let mut t = term_sized(20, 3);
    // 'e' + combining acute accent (U+0301) → 'é' via NFC
    let text = "e\u{0301}";
    feed_bytes(&mut t, text.as_bytes());

    let row_text = row(&t, 0);
    // NFC composition: 'e' + U+0301 → 'é' (U+00E9)
    assert!(
        row_text.contains('\u{00E9}'),
        "NFC composed char must be present, got: '{}'",
        row_text
    );
    // Cursor should advance by the display width of the base character (1 column)
    assert_eq!(
        t.cursor.col, 1,
        "combining mark should not advance cursor, got col={}",
        t.cursor.col
    );
}

#[test]
fn multiple_combining_marks_on_single_base() {
    let mut t = term_sized(20, 3);
    // 'a' + combining tilde (U+0303) + combining dot below (U+0323) + combining ring above (U+030A)
    let text = "a\u{0303}\u{0323}\u{030A}";
    feed_bytes(&mut t, text.as_bytes());

    let row_text = row(&t, 0);
    // NFC: 'a' + U+0303 → 'ã' (U+00E3). Subsequent combiners that don't compose
    // to a single codepoint are dropped (full grapheme support tracked in #76).
    assert!(
        row_text.contains('\u{00E3}'),
        "NFC composed char must be present, got: '{}'",
        row_text
    );
    // Cursor should advance by the display width of the base character (1 column)
    assert_eq!(
        t.cursor.col, 1,
        "stacked combiners should not advance cursor, got col={}",
        t.cursor.col
    );
}

// -- RTL / Bidirectional Text -------------------------------------------------

#[test]
fn arabic_text_stores_correctly() {
    let mut t = term_sized(40, 3);
    let arabic = "\u{0645}\u{0631}\u{062D}\u{0628}\u{0627}"; // "mrhba" in Arabic
    feed_bytes(&mut t, arabic.as_bytes());

    let row_text = row(&t, 0);
    // Terminal should store the characters (bidi rendering is a display concern,
    // but storage must not corrupt or drop them)
    for ch in arabic.chars() {
        assert!(
            row_text.contains(ch),
            "Arabic char U+{:04X} must be stored in grid",
            ch as u32
        );
    }
}

#[test]
fn hebrew_mixed_with_latin() {
    let mut t = term_sized(40, 3);
    let mixed = "Hello \u{05E9}\u{05DC}\u{05D5}\u{05DD} World";
    feed_bytes(&mut t, mixed.as_bytes());

    let row_text = row(&t, 0);
    assert!(
        row_text.contains("Hello"),
        "Latin portion must be preserved"
    );
    assert!(
        row_text.contains("World"),
        "Latin portion must be preserved"
    );
    assert!(
        row_text.contains('\u{05E9}'),
        "Hebrew chars must be preserved"
    );
}

// -- Multi-byte UTF-8 Stress --------------------------------------------------

#[test]
fn full_utf8_range_1_to_4_bytes() {
    let mut t = TerminalState::new(80, 24, 1000);
    let test_chars: Vec<char> = vec![
        // 1-byte UTF-8 (ASCII)
        'A',
        'Z',
        '0',
        '~',
        // 2-byte UTF-8 (Latin Extended, Cyrillic)
        '\u{00E9}', // e-acute
        '\u{00F1}', // n-tilde
        '\u{0410}', // Cyrillic A
        '\u{03B1}', // Greek alpha
        // 3-byte UTF-8 (CJK, general BMP)
        '\u{4E00}', // CJK first
        '\u{9FFF}', // CJK last
        '\u{20AC}', // Euro sign
        '\u{2603}', // Snowman
        // 4-byte UTF-8 (supplementary plane - emoji, rare CJK)
        '\u{1F600}', // grinning face
        '\u{1F4A9}', // pile of poo
        '\u{20000}', // CJK Extension B first char
    ];

    for ch in &test_chars {
        let mut buf = [0u8; 4];
        let encoded = ch.encode_utf8(&mut buf);
        feed_bytes(&mut t, encoded.as_bytes());
    }

    // Terminal must remain functional after all byte-width categories
    assert!(t.cursor.row < t.grid.height());
    assert!(t.cursor.col < t.grid.width());

    // Verify at least some characters are in the grid
    let mut found_count = 0;
    for r in 0..t.grid.height() {
        let text = row(&t, r);
        for ch in &test_chars {
            if text.contains(*ch) {
                found_count += 1;
            }
        }
    }
    assert!(
        found_count > 0,
        "at least some test characters must appear in the grid"
    );
}

#[test]
fn utf8_boundary_split_stress() {
    // Feed multi-byte UTF-8 sequences split across chunk boundaries at every possible
    // split point. The parser must reassemble them correctly.
    let test_chars: Vec<&str> = vec![
        "\u{00E9}",  // 2-byte: C3 A9
        "\u{20AC}",  // 3-byte: E2 82 AC
        "\u{1F600}", // 4-byte: F0 9F 98 80
    ];

    for text in &test_chars {
        let bytes = text.as_bytes();
        // Split at every possible byte boundary
        for split_at in 1..bytes.len() {
            let mut t = term_sized(20, 3);
            feed_bytes(&mut t, &bytes[..split_at]);
            feed_bytes(&mut t, &bytes[split_at..]);

            let row_text = row(&t, 0);
            assert!(
                row_text.contains(text),
                "split at byte {} of {:?} must reassemble correctly, got: '{}'",
                split_at,
                bytes,
                row_text
            );
        }
    }
}

#[test]
fn utf8_overlong_sequences_rejected() {
    let mut t = term_sized(20, 3);
    // Overlong encoding of '/' (U+002F): should be 0x2F but encoded as C0 AF (2-byte)
    // This is invalid UTF-8 and should produce replacement characters
    let overlong: &[u8] = &[0xC0, 0xAF];
    feed_bytes(&mut t, overlong);
    feed_bytes(&mut t, b"OK");

    let row_text = row(&t, 0);
    // "OK" must survive after the invalid sequence
    assert!(
        row_text.contains("OK"),
        "text after overlong sequence must be preserved, got: '{}'",
        row_text
    );
    // The overlong bytes should produce replacement characters (U+FFFD)
    assert!(
        row_text.contains('\u{FFFD}'),
        "overlong encoding must produce replacement character, got: '{}'",
        row_text
    );
}

#[test]
fn utf8_10k_mixed_scripts() {
    let mut t = TerminalState::new(80, 24, 1000);
    let mut payload = String::with_capacity(40_000);
    for i in 0u32..10_000 {
        let ch = match i % 4 {
            // Latin
            0 => char::from_u32(0x0041 + (i % 26)).unwrap_or('A'),
            // CJK
            1 => char::from_u32(0x4E00 + (i % 0x5200)).unwrap_or('\u{4E00}'),
            // Cyrillic
            2 => char::from_u32(0x0410 + (i % 64)).unwrap_or('\u{0410}'),
            // Arabic
            _ => char::from_u32(0x0621 + (i % 40)).unwrap_or('\u{0621}'),
        };
        payload.push(ch);
        if i % 80 == 79 {
            payload.push('\n');
        }
    }
    for chunk in payload.as_bytes().chunks(64 * 1024) {
        feed_bytes(&mut t, chunk);
    }

    assert!(t.cursor.row < t.grid.height());
    assert!(t.cursor.col < t.grid.width());
    assert!(
        !t.scrollback.is_empty(),
        "10K mixed script chars must push to scrollback"
    );
}

// -- Scrollback with Unicode --------------------------------------------------

#[test]
fn scrollback_preserves_unicode_content() {
    let mut t = TerminalState::new(40, 3, 100);
    let lines = [
        "\u{041F}\u{0440}\u{0438}\u{0432}\u{0435}\u{0442}", // Cyrillic "Privet"
        "\u{4F60}\u{597D}\u{4E16}\u{754C}",                 // CJK "NiHaoShiJie"
        "\u{1F680}\u{1F30D}\u{1F389}",                      // Emoji
        "Mixed: ABC\u{65E5}\u{672C}\u{8A9E}DEF",            // Mixed ASCII + CJK
        "Latin \u{00E9}\u{00F1}\u{00FC}",                   // Latin extended
    ];
    for line in &lines {
        feed_bytes(&mut t, format!("{}\r\n", line).as_bytes());
    }

    // Collect all text from scrollback and grid
    let mut all_text = String::new();
    for i in 0..t.scrollback.len() {
        all_text.push_str(t.scrollback.get(i).unwrap());
        all_text.push('\n');
    }
    for r in 0..t.grid.height() {
        all_text.push_str(&row(&t, r));
        all_text.push('\n');
    }

    for line in &lines {
        assert!(
            all_text.contains(line),
            "must preserve Unicode line: {}",
            line
        );
    }
}

#[test]
fn scrollback_cjk_line_length() {
    let mut t = TerminalState::new(20, 3, 100);
    // 10 CJK chars = 20 columns = exactly one full row
    let cjk_line: String = (0..10)
        .map(|i| char::from_u32(0x4E00 + i as u32).unwrap())
        .collect();
    feed_bytes(&mut t, cjk_line.as_bytes());
    feed_bytes(&mut t, b"\r\n");
    // Push more lines to force the CJK line into scrollback
    for _ in 0..5 {
        feed_bytes(&mut t, b"filler\r\n");
    }

    // Find the CJK line in scrollback
    let mut found = false;
    for i in 0..t.scrollback.len() {
        let sb_line = t.scrollback.get(i).unwrap();
        if sb_line.contains('\u{4E00}') {
            // Scrollback stores characters, not display columns, so length
            // should be 10 characters (not 20 columns)
            assert_eq!(
                sb_line.chars().count(),
                10,
                "scrollback CJK line should have 10 chars, got: '{}'",
                sb_line
            );
            found = true;
            break;
        }
    }
    assert!(found, "CJK line must be found in scrollback");
}

#[test]
fn scrollback_mixed_width_lines() {
    let mut t = TerminalState::new(20, 3, 100);
    let lines = [
        "A\u{6F22}B\u{5B57}C",              // ASCII + CJK interleaved
        "\u{6F22}\u{5B57}\u{6F22}\u{5B57}", // all CJK
        "ABCDEFGHIJ",                       // all ASCII
    ];
    for line in &lines {
        feed_bytes(&mut t, format!("{}\r\n", line).as_bytes());
    }
    // Force all into scrollback
    for _ in 0..5 {
        feed_bytes(&mut t, b"pad\r\n");
    }

    let mut all_sb = String::new();
    for i in 0..t.scrollback.len() {
        all_sb.push_str(t.scrollback.get(i).unwrap());
        all_sb.push('\n');
    }
    for line in &lines {
        assert!(
            all_sb.contains(line),
            "scrollback must preserve mixed-width line: '{}'",
            line
        );
    }
}

// -- Reflow with Unicode ------------------------------------------------------

#[test]
fn reflow_cjk_text_preserves_pairs() {
    let mut t = TerminalState::new(10, 5, 100);
    // "AAAA漢字BB" = 4 + 2 + 2 + 2 = 10 columns, exactly fills row
    feed_bytes(&mut t, "AAAA\u{6F22}\u{5B57}BB".as_bytes());
    assert_eq!(row(&t, 0), "AAAA\u{6F22}\u{5B57}BB");

    // Shrink to 5 columns: wide chars must not be split across rows
    t.resize(5, 5);

    // Collect all visible content
    let mut visible = String::new();
    for r in 0..5 {
        visible.push_str(&row(&t, r));
    }

    // Wide characters must remain intact (not split)
    assert!(visible.contains('\u{6F22}'), "CJK char must survive reflow");
    assert!(visible.contains('\u{5B57}'), "CJK char must survive reflow");

    // Verify no continuation cell is left as the first cell of any row
    for r in 0..5 {
        let cells = t.grid.row_cells(r).unwrap();
        assert_ne!(
            cells[0].width, 0,
            "row {} must not start with a continuation cell",
            r
        );
    }
}

#[test]
fn reflow_mixed_width_cursor_tracking() {
    let mut t = TerminalState::new(10, 5, 100);
    // "ABC漢DE" = 3 + 2 + 2 = 7 columns
    feed_bytes(&mut t, "ABC\u{6F22}DE".as_bytes());
    // Cursor is after 'E' at col 7 (or wrap_pending if at col 6)
    let col_before = t.cursor.col;

    // Shrink to 4 columns
    t.resize(4, 5);

    // Cursor must be within bounds
    assert!(
        t.cursor.col < 4,
        "cursor col {} must be < new width 4",
        t.cursor.col
    );
    assert!(
        t.cursor.row < 5,
        "cursor row {} must be < height 5",
        t.cursor.row
    );

    // Content must be preserved across reflow
    let mut all_text = String::new();
    for r in 0..5 {
        all_text.push_str(&row(&t, r));
    }
    assert!(
        all_text.contains("ABC"),
        "ASCII portion must survive reflow, col was {}, got: '{}'",
        col_before,
        all_text
    );
}

// -- Performance / Throughput -------------------------------------------------

#[test]
fn unicode_throughput_100k_mixed() {
    let mut t = TerminalState::new(80, 24, 1000);

    let mut payload = String::with_capacity(400_000);
    for i in 0u32..100_000 {
        let ch = match i % 5 {
            0 => char::from_u32(0x0041 + (i % 26)).unwrap_or('A'),
            1 => char::from_u32(0x4E00 + (i % 0x5200)).unwrap_or('\u{4E00}'),
            2 => char::from_u32(0x0410 + (i % 64)).unwrap_or('\u{0410}'),
            3 => char::from_u32(0x0621 + (i % 40)).unwrap_or('\u{0621}'),
            _ => char::from_u32(0x1F600 + (i % 80)).unwrap_or('\u{1F600}'),
        };
        payload.push(ch);
        if i % 80 == 79 {
            payload.push('\n');
        }
    }

    let start = Instant::now();
    for chunk in payload.as_bytes().chunks(64 * 1024) {
        feed_bytes(&mut t, chunk);
    }
    let elapsed = start.elapsed();

    // Verify terminal is functional
    feed_bytes(&mut t, b"ALIVE\r\n");
    assert!(t.cursor.row < t.grid.height());
    assert!(!t.scrollback.is_empty());

    // Sanity: should complete in under 30 seconds on any reasonable hardware
    assert!(
        elapsed.as_secs() < 30,
        "100K mixed Unicode ingest took {:?}, expected < 30s",
        elapsed
    );
}

#[test]
fn cjk_grid_churn_50k_lines() {
    let mut t = term_full(80, 24, 1000);

    // Build a CJK line that fills exactly one 80-col row (40 CJK chars = 80 cols)
    let cjk_line: String = (0..40)
        .map(|i| char::from_u32(0x4E00 + (i % 0x5200) as u32).unwrap_or('\u{4E00}'))
        .collect();
    let line_with_crlf = format!("{}\r\n", cjk_line);
    let line_bytes = line_with_crlf.as_bytes();

    let start = Instant::now();
    for _ in 0..50_000 {
        feed_bytes(&mut t, line_bytes);
    }
    let elapsed = start.elapsed();

    // Scrollback must be at cap
    assert_eq!(t.scrollback.len(), t.scrollback.cap());

    // Grid must still be functional
    assert!(t.cursor.row < t.grid.height());

    // Sanity: should complete in under 60 seconds
    assert!(
        elapsed.as_secs() < 60,
        "50K CJK line churn took {:?}, expected < 60s",
        elapsed
    );
}

// -- Additional edge cases ----------------------------------------------------

#[test]
fn null_byte_inside_unicode_stream() {
    let mut t = term_sized(20, 3);
    // NUL (0x00) is a C0 control; it should not break the parser mid-stream
    feed_bytes(&mut t, "AB\x00CD".as_bytes());
    let row_text = row(&t, 0);
    assert!(row_text.contains("AB"), "text before NUL must be preserved");
    assert!(row_text.contains("CD"), "text after NUL must be preserved");
}

#[test]
fn bmp_private_use_area() {
    let mut t = term_sized(20, 3);
    // Private Use Area U+E000..U+F8FF (commonly used by Nerd Fonts)
    let pua_chars: String = (0xE000..0xE010)
        .map(|cp| char::from_u32(cp).unwrap())
        .collect();
    feed_bytes(&mut t, pua_chars.as_bytes());

    // Characters should be stored without panic
    let row_text = row(&t, 0);
    assert!(
        !row_text.is_empty(),
        "PUA characters must be stored in grid"
    );
}

#[test]
fn supplementary_private_use_area() {
    let mut t = term_sized(20, 3);
    // Supplementary Private Use Area-A: U+F0000..U+FFFFF (4-byte UTF-8)
    let spua_chars: String = (0xF0000..0xF0010)
        .map(|cp| char::from_u32(cp).unwrap())
        .collect();
    feed_bytes(&mut t, spua_chars.as_bytes());

    let row_text = row(&t, 0);
    assert!(
        !row_text.is_empty(),
        "Supplementary PUA characters must be stored"
    );
}

#[test]
fn zero_width_joiner_sequences() {
    let mut t = term_sized(40, 3);
    // Family emoji: person + ZWJ + person + ZWJ + child
    // Terminals vary on this, but it must not panic or corrupt state
    let zwj_seq = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
    feed_bytes(&mut t, zwj_seq.as_bytes());

    // Terminal must remain functional
    assert!(t.cursor.row < t.grid.height());
    assert!(t.cursor.col < t.grid.width());

    // Content must be stored (exact display width varies by implementation)
    let row_text = row(&t, 0);
    assert!(
        !row_text.is_empty(),
        "ZWJ sequence must produce grid content"
    );
}

#[test]
fn variation_selector_16() {
    let mut t = term_sized(20, 3);
    // Text character + VS16 (U+FE0F) to request emoji presentation
    // Star: U+2B50 + U+FE0F
    let star_emoji = "\u{2B50}\u{FE0F}";
    feed_bytes(&mut t, star_emoji.as_bytes());
    feed_bytes(&mut t, b"OK");

    let row_text = row(&t, 0);
    assert!(
        row_text.contains("OK"),
        "text after VS16 sequence must be preserved"
    );
}

#[test]
fn surrogate_halves_rejected() {
    let mut t = term_sized(20, 3);
    // Manually construct bytes that would represent surrogate half U+D800 in a
    // naive encoding: ED A0 80. This is invalid UTF-8.
    let invalid: &[u8] = &[0xED, 0xA0, 0x80];
    feed_bytes(&mut t, invalid);
    feed_bytes(&mut t, b"OK");

    let row_text = row(&t, 0);
    assert!(
        row_text.contains("OK"),
        "text after invalid surrogate bytes must be preserved"
    );
    assert!(
        row_text.contains('\u{FFFD}'),
        "surrogate halves must produce replacement character"
    );
}

#[test]
fn rapid_script_switching() {
    // Rapidly switch between scripts, simulating multilingual AI output
    let mut t = TerminalState::new(80, 24, 1000);
    let scripts: Vec<&str> = vec![
        "Hello",
        "\u{4F60}\u{597D}",                                 // Chinese
        "\u{041F}\u{0440}\u{0438}\u{0432}\u{0435}\u{0442}", // Russian
        "\u{3053}\u{3093}\u{306B}\u{3061}\u{306F}",         // Japanese hiragana
        "\u{C548}\u{B155}",                                 // Korean
        "\u{0645}\u{0631}\u{062D}\u{0628}\u{0627}",         // Arabic
        "\u{05E9}\u{05DC}\u{05D5}\u{05DD}",                 // Hebrew
        "World",
    ];

    for _ in 0..500 {
        for script in &scripts {
            feed_bytes(&mut t, script.as_bytes());
            feed_bytes(&mut t, b" ");
        }
        feed_bytes(&mut t, b"\r\n");
    }

    // Terminal must remain functional after 4000 script switches
    assert!(t.cursor.row < t.grid.height());
    assert!(!t.scrollback.is_empty());
}

#[test]
fn max_codepoint_boundary() {
    let mut t = term_sized(20, 3);
    // U+10FFFF is the maximum valid Unicode code point (4-byte UTF-8: F4 8F BF BF)
    let max_cp = char::from_u32(0x10FFFF).unwrap();
    let mut buf = [0u8; 4];
    let encoded = max_cp.encode_utf8(&mut buf);
    feed_bytes(&mut t, encoded.as_bytes());
    feed_bytes(&mut t, b"X");

    // Must not panic; X must be stored
    let row_text = row(&t, 0);
    assert!(
        row_text.contains('X'),
        "char after U+10FFFF must be preserved"
    );
}

#[test]
fn invalid_utf8_continuation_byte_alone() {
    let mut t = term_sized(20, 3);
    // Lone continuation byte 0x80 is invalid
    feed_bytes(&mut t, &[0x80]);
    feed_bytes(&mut t, b"OK");

    let row_text = row(&t, 0);
    assert!(
        row_text.contains("OK"),
        "text after lone continuation byte must be preserved"
    );
}

#[test]
fn fe_ff_bytes_handled() {
    let mut t = term_sized(20, 3);
    // 0xFE and 0xFF are never valid in UTF-8
    feed_bytes(&mut t, &[0xFE, 0xFF]);
    feed_bytes(&mut t, b"OK");

    let row_text = row(&t, 0);
    assert!(
        row_text.contains("OK"),
        "text after 0xFE/0xFF bytes must be preserved"
    );
}

#[test]
fn thai_complex_script() {
    let mut t = term_sized(40, 3);
    // Thai text with base consonants and combining vowel/tone marks.
    // Combining marks (Mn category) have unicode width 0, so they are placed at the
    // current cursor position but do not advance the cursor. The next base character
    // overwrites the mark's cell. This is standard terminal behavior (no combining
    // character stacking). We verify the terminal does not panic and that base
    // consonants survive.
    let thai = "\u{0E2A}\u{0E27}\u{0E31}\u{0E2A}\u{0E14}\u{0E35}\u{0E04}\u{0E23}\u{0E31}\u{0E1A}";
    feed_bytes(&mut t, thai.as_bytes());

    let row_text = row(&t, 0);
    assert!(!row_text.is_empty(), "Thai text must produce grid content");

    // Base consonants that are NOT immediately preceded by a zero-width mark should
    // survive in the grid. The first char (U+0E2A) and the ones that follow marks
    // (which overwrite the marks) should be present.
    // In practice: ส(0E2A) ว(0E27) [ั(0E31) overwritten by] ส(0E2A) ...
    // At least some base characters must appear
    let base_count = row_text
        .chars()
        .filter(|c| ('\u{0E00}'..='\u{0E7F}').contains(c))
        .count();
    assert!(
        base_count >= 3,
        "at least 3 Thai base consonants must survive, got {}",
        base_count
    );
}

#[test]
fn devanagari_conjuncts() {
    let mut t = term_sized(40, 3);
    // Hindi text with virama (halant) for conjunct formation.
    // Virama (U+094D) and dependent vowel (U+0947) have unicode width 0 and are placed
    // at the cursor without advancing it. The next base character overwrites them.
    // This is standard terminal behavior (no combining character support).
    let hindi = "\u{0928}\u{092E}\u{0938}\u{094D}\u{0924}\u{0947}"; // "namaste"
    feed_bytes(&mut t, hindi.as_bytes());

    let row_text = row(&t, 0);
    assert!(!row_text.is_empty(), "Devanagari text must be stored");

    // Some base consonants must survive in the grid. Zero-width marks (virama, vowel
    // signs) are overwritten by the next character, so we only verify base chars.
    let devanagari_count = row_text
        .chars()
        .filter(|c| ('\u{0900}'..='\u{097F}').contains(c))
        .count();
    assert!(
        devanagari_count >= 2,
        "at least 2 Devanagari base characters must survive, got {}",
        devanagari_count
    );
}

// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use super::{Parser, ParserAction};

// ── CSI Parameter Parsing ──────────────────────────────────

#[test]
fn csi_with_256_parameters() {
    // CSI with far more params than MAX_CSI_PARAMS (32) - should handle gracefully.
    // parse_params returns Err when count exceeds MAX_CSI_PARAMS, causing
    // the sequence to be emitted as UnsupportedSequence rather than panicking.
    let mut parser = Parser::default();
    let params_str = vec!["1"; 256].join(";");
    let seq = format!("\x1b[{params_str}m");
    let actions = parser.feed(seq.as_bytes());

    // Must not panic. The sequence may be accepted (truncated SGR), degraded
    // (CSI too long), or unsupported - any of these is valid handling.
    assert!(
        !actions.is_empty(),
        "256-param CSI should produce at least one action"
    );

    // Parser must return to ground state and handle subsequent input
    let after = parser.feed(b"A");
    assert_eq!(after, vec![ParserAction::Print('A')]);
}

#[test]
fn csi_with_max_value_params() {
    // CSI with u16::MAX values in parameters
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[65535;65535;65535m");
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, ParserAction::SetGraphicsRendition(sgr)
                if sgr.as_slice().iter().all(|p| *p == Some(65535))
            )),
        "SGR with u16::MAX values should be preserved"
    );
}

#[test]
fn csi_with_zero_params() {
    // CSI 0;0;0m - all explicit zeros
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[0;0;0m");
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, ParserAction::SetGraphicsRendition(sgr)
                if sgr.as_slice() == [Some(0), Some(0), Some(0)]
            )),
        "SGR with explicit zeros should preserve them"
    );
}

#[test]
fn csi_empty_params_treated_as_default() {
    // CSI ;; m - empty params should be represented as None (default)
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[;;m");
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, ParserAction::SetGraphicsRendition(sgr)
                if sgr.as_slice() == [None, None, None]
            )),
        "Empty CSI params should be None (default), got: {actions:?}"
    );
}

// ── UTF-8 Boundary Stress ──────────────────────────────────

#[test]
fn utf8_every_possible_split_2byte() {
    // 2-byte UTF-8 char: U+00E9 (e-acute) = [0xC3, 0xA9]
    let bytes = [0xC3, 0xA9];
    let expected_char = '\u{00E9}';

    for split in 0..=bytes.len() {
        let mut parser = Parser::default();
        let part1 = &bytes[..split];
        let part2 = &bytes[split..];

        let actions1 = parser.feed(part1);
        let actions2 = parser.feed(part2);

        let all_actions: Vec<_> = actions1.into_iter().chain(actions2).collect();
        assert!(
            all_actions.contains(&ParserAction::Print(expected_char)),
            "2-byte char split at {split} should produce '{expected_char}', got: {all_actions:?}"
        );
    }
}

#[test]
fn utf8_every_possible_split_3byte() {
    // 3-byte UTF-8 char: U+20AC (euro sign) = [0xE2, 0x82, 0xAC]
    let bytes = [0xE2, 0x82, 0xAC];
    let expected_char = '\u{20AC}';

    for split in 0..=bytes.len() {
        let mut parser = Parser::default();
        let part1 = &bytes[..split];
        let part2 = &bytes[split..];

        let actions1 = parser.feed(part1);
        let actions2 = parser.feed(part2);

        let all_actions: Vec<_> = actions1.into_iter().chain(actions2).collect();
        assert!(
            all_actions.contains(&ParserAction::Print(expected_char)),
            "3-byte char split at {split} should produce '{expected_char}', got: {all_actions:?}"
        );
    }
}

#[test]
fn utf8_every_possible_split_4byte() {
    // 4-byte UTF-8 char: U+1F600 (grinning face) = [0xF0, 0x9F, 0x98, 0x80]
    let bytes = [0xF0, 0x9F, 0x98, 0x80];
    let expected_char = '\u{1F600}';

    for split in 0..=bytes.len() {
        let mut parser = Parser::default();
        let part1 = &bytes[..split];
        let part2 = &bytes[split..];

        let actions1 = parser.feed(part1);
        let actions2 = parser.feed(part2);

        let all_actions: Vec<_> = actions1.into_iter().chain(actions2).collect();
        assert!(
            all_actions.contains(&ParserAction::Print(expected_char)),
            "4-byte char split at {split} should produce '{expected_char}', got: {all_actions:?}"
        );
    }
}

#[test]
fn utf8_1000_random_split_points() {
    // Build a buffer with 1000 multi-byte characters and feed at every possible
    // split point within a sliding window to verify no panic and correct output.
    let test_chars = [
        '\u{00E9}',  // 2-byte
        '\u{20AC}',  // 3-byte
        '\u{1F600}', // 4-byte
        '\u{4E16}',  // 3-byte CJK
    ];
    let mut full_bytes = Vec::new();
    let mut expected_chars = Vec::new();
    for i in 0..1000 {
        let ch = test_chars[i % test_chars.len()];
        let mut buf = [0u8; 4];
        let encoded = ch.encode_utf8(&mut buf);
        full_bytes.extend_from_slice(encoded.as_bytes());
        expected_chars.push(ch);
    }

    // Test splits at every position within the first 200 bytes.
    // Both part1 and part2 are restricted to the same test_range window.
    // Snap test_range to a character boundary so we feed only complete
    // characters, making the expected count deterministic.
    let raw_range = full_bytes.len().min(200);
    let test_range = {
        let s = std::str::from_utf8(&full_bytes[..]).expect("full_bytes is valid UTF-8");
        // Find the largest char boundary <= raw_range.
        let mut bound = raw_range;
        while bound > 0 && !s.is_char_boundary(bound) {
            bound -= 1;
        }
        bound
    };

    // Count expected characters within [0..test_range].
    let expected_in_range = std::str::from_utf8(&full_bytes[..test_range])
        .expect("snapped to char boundary")
        .chars()
        .count();

    for split in 0..=test_range {
        let mut parser = Parser::default();
        let part1 = &full_bytes[..split];
        let part2 = &full_bytes[split..test_range];

        let actions1 = parser.feed(part1);
        let actions2 = parser.feed(part2);

        // Must not panic - that is the primary assertion
        let total_count: usize = actions1
            .iter()
            .chain(actions2.iter())
            .map(|a| match a {
                ParserAction::Print(_) => 1,
                ParserAction::PrintText(t) => t.chars().count(),
                _ => 0,
            })
            .sum();

        // Characters emitted should equal the decoded chars in [0..test_range].
        assert_eq!(
            total_count, expected_in_range,
            "split at {split}: expected {expected_in_range} chars, got {total_count}",
        );
    }
}

// ── Sequence Stress ────────────────────────────────────────

#[test]
fn rapid_mode_switching_1000_cycles() {
    let mut parser = Parser::default();
    let mut total_actions = 0usize;

    for _ in 0..1000 {
        // Alternate between multiple private modes in rapid succession
        let actions = parser.feed(
            b"\x1b[?1h\x1b[?25l\x1b[?2004h\x1b[?1049h\
              \x1b[?1l\x1b[?25h\x1b[?2004l\x1b[?1049l",
        );
        total_actions += actions.len();
        // Each cycle: 8 mode switches
        assert_eq!(
            actions.len(),
            8,
            "each cycle should produce exactly 8 mode actions"
        );
    }
    assert_eq!(total_actions, 8000);

    // Parser must be in ground state and ready for normal input
    let after = parser.feed(b"OK");
    assert!(
        after
            .iter()
            .any(|a| matches!(a, ParserAction::PrintText(t) if t == "OK")),
    );
}

#[test]
fn interleaved_csi_and_text_1000() {
    let mut parser = Parser::default();
    let mut sgr_count = 0usize;
    let mut char_count = 0usize;

    for i in 0..1000u32 {
        // CSI command then text, 1000 repetitions
        let color = (i % 8) as u8 + 30; // 30-37
        let seq = format!("\x1b[{color}m{i}");
        let actions = parser.feed(seq.as_bytes());

        for action in &actions {
            match action {
                ParserAction::SetGraphicsRendition(_) => sgr_count += 1,
                ParserAction::Print(_) => char_count += 1,
                ParserAction::PrintText(t) => char_count += t.len(),
                _ => {}
            }
        }
    }

    assert_eq!(sgr_count, 1000, "should have 1000 SGR commands");
    // Each iteration prints the decimal representation of i (0..1000).
    // Total digit count = sum of digit_count(i) for i in 0..1000.
    let expected_chars: usize = (0..1000u32).map(|i| i.to_string().len()).sum();
    assert_eq!(
        char_count, expected_chars,
        "should have {expected_chars} text characters across all iterations"
    );
}

#[test]
fn nested_osc_inside_csi_rejected() {
    // ESC appearing during CSI is treated as another CSI parameter byte.
    // The ']' byte (0x5D) is a valid CSI final byte (0x40-0x7E range),
    // so the CSI is completed with the accumulated buffer.
    // The subsequent "0;Title\x07After" is then processed in Ground state:
    // "0;Title" as text, BEL as Bell, "After" as text.
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[31\x1b]0;Title\x07After");

    // The CSI is interrupted by ESC, which in CSI state gets appended to the
    // csi_buffer. Then ']' completes the CSI (it's a final byte). The result
    // is an UnsupportedSequence for the malformed CSI.
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, ParserAction::UnsupportedSequence(_))),
        "interrupted CSI should produce UnsupportedSequence, got: {actions:?}"
    );

    // "After" should be printed as normal text
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, ParserAction::PrintText(t) if t == "After")),
        "text after interrupted CSI should print normally"
    );

    // Parser must be in ground state and functional
    let after = parser.feed(b"OK");
    assert!(
        after
            .iter()
            .any(|a| matches!(a, ParserAction::PrintText(t) if t == "OK")),
    );
}

#[test]
fn escape_flood_10000() {
    // 10K bare ESC characters. The parser alternates between Ground and Escape:
    // - ESC in Ground: transitions to Escape, no output
    // - ESC in Escape: falls through to default handler, emits UnsupportedSequence,
    //   returns to Ground
    // With 10K (even) ESCs, parser ends in Ground state.
    let mut parser = Parser::default();
    let input = vec![0x1B; 10_000];
    let actions = parser.feed(&input);

    // No panic is the primary assertion.
    let unsupported_count = actions
        .iter()
        .filter(|a| matches!(a, ParserAction::UnsupportedSequence(_)))
        .count();
    assert!(
        unsupported_count > 0,
        "bare ESC pairs should produce unsupported events"
    );

    // Verify the parser recovers correctly: a subsequent SGR must parse properly.
    let recovery = parser.feed(b"\x1b[31mX");
    assert!(
        recovery
            .iter()
            .any(|a| matches!(a, ParserAction::SetGraphicsRendition(_))),
        "parser must recover and parse SGR after ESC flood"
    );
    assert!(
        recovery
            .iter()
            .any(|a| matches!(a, ParserAction::Print('X'))),
        "parser must print text after SGR recovery"
    );

    // With odd count (10001), parser ends in Escape state
    let mut parser2 = Parser::default();
    let input_odd = vec![0x1B; 10_001];
    let actions_odd = parser2.feed(&input_odd);
    let unsupported_odd = actions_odd
        .iter()
        .filter(|a| matches!(a, ParserAction::UnsupportedSequence(_)))
        .count();
    assert!(
        unsupported_odd > 0,
        "odd ESC flood should also produce unsupported events"
    );

    // Now ESC M should work (parser in Escape state)
    let after2 = parser2.feed(b"MZ");
    assert!(
        after2.contains(&ParserAction::ReverseIndex),
        "ESC M should produce ReverseIndex when parser is in Escape state"
    );
    assert!(
        after2.contains(&ParserAction::Print('Z')),
        "Z should print after ReverseIndex"
    );
}

#[test]
fn control_char_flood() {
    // 10K control characters (BEL, BS, TAB, CR, LF) in rapid sequence
    let mut parser = Parser::default();
    let mut input = Vec::with_capacity(10_000);
    let controls = [0x07u8, 0x08, 0x09, 0x0D, 0x0A]; // BEL, BS, TAB, CR, LF
    for i in 0..10_000 {
        input.push(controls[i % controls.len()]);
    }
    let actions = parser.feed(&input);

    // Should produce exactly 10K actions (one per control char)
    assert_eq!(
        actions.len(),
        10_000,
        "each control char should produce one action"
    );

    // Verify distribution
    let bell_count = actions.iter().filter(|a| **a == ParserAction::Bell).count();
    let bs_count = actions
        .iter()
        .filter(|a| **a == ParserAction::Backspace)
        .count();
    let tab_count = actions.iter().filter(|a| **a == ParserAction::Tab).count();
    let cr_count = actions
        .iter()
        .filter(|a| **a == ParserAction::CarriageReturn)
        .count();
    let lf_count = actions
        .iter()
        .filter(|a| **a == ParserAction::LineFeed)
        .count();

    assert_eq!(bell_count, 2000);
    assert_eq!(bs_count, 2000);
    assert_eq!(tab_count, 2000);
    assert_eq!(cr_count, 2000);
    assert_eq!(lf_count, 2000);
}

#[test]
fn max_osc_payload_length() {
    // Test OSC with a very large payload (just under MAX_OSC_LEN)
    let mut parser = Parser::default();
    let prefix = b"0;";
    // MAX_OSC_LEN is 4096; fill exactly to the limit
    let fill_len = 4096 - prefix.len();
    let mut payload = vec![0x1B, b']'];
    payload.extend_from_slice(prefix);
    payload.extend(std::iter::repeat_n(b'X', fill_len));
    payload.push(0x07); // BEL terminator
    let actions = parser.feed(&payload);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, ParserAction::SetWindowTitle(_))),
        "OSC at exactly MAX_OSC_LEN should be accepted"
    );

    // Now test one byte over - should be discarded
    let mut parser = Parser::default();
    let mut payload_over = vec![0x1B, b']'];
    payload_over.extend_from_slice(prefix);
    payload_over.extend(std::iter::repeat_n(b'X', fill_len + 1));
    payload_over.push(0x07);
    let actions_over = parser.feed(&payload_over);
    assert!(
        !actions_over
            .iter()
            .any(|a| matches!(a, ParserAction::SetWindowTitle(_))),
        "OSC one byte over MAX_OSC_LEN should be discarded"
    );

    // Parser should recover and handle normal input after oversized OSC
    let recovery = parser.feed(b"Hello");
    assert!(
        recovery
            .iter()
            .any(|a| matches!(a, ParserAction::PrintText(t) if t == "Hello")),
        "parser should recover after oversized OSC"
    );
}

#[test]
fn alternating_dcs_and_ground() {
    // DCS enter/exit cycling - rapid DCS payload followed by ST terminator
    let mut parser = Parser::default();

    for i in 0..500u32 {
        // Enter DCS, send some payload, terminate with ST (ESC \)
        let payload = format!("\x1bP+q{i:04x}\x1b\\");
        let actions = parser.feed(payload.as_bytes());
        // DCS is silently absorbed, no actions expected
        assert!(
            actions.is_empty(),
            "DCS cycle {i} should produce no actions, got: {actions:?}"
        );
    }

    // Verify parser returns to ground state and handles normal text
    let after = parser.feed(b"Done");
    assert!(
        after
            .iter()
            .any(|a| matches!(a, ParserAction::PrintText(t) if t == "Done")),
        "parser should be in ground state after DCS cycling"
    );
}

// ── Additional stress scenarios ────────────────────────────

#[test]
fn csi_parameter_overflow_saturates_to_u16_max() {
    // Test that numeric parameters larger than u16::MAX are clamped
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[99999A");
    // CursorUp param should be clamped to u16::MAX (65535)
    assert_eq!(actions, vec![ParserAction::CursorUp(65535)]);
}

#[test]
fn mixed_control_and_multibyte_stress() {
    // Interleave control characters with multi-byte UTF-8
    let mut parser = Parser::default();
    let euro = "\u{20AC}"; // 3 bytes
    let mut input = Vec::new();
    for _ in 0..500 {
        input.extend_from_slice(euro.as_bytes());
        input.push(0x0A); // LF
    }

    let actions = parser.feed(&input);
    let print_count: usize = actions
        .iter()
        .map(|a| match a {
            ParserAction::Print(_) => 1,
            ParserAction::PrintText(t) => t.chars().count(),
            _ => 0,
        })
        .sum();
    let lf_count = actions
        .iter()
        .filter(|a| **a == ParserAction::LineFeed)
        .count();

    assert_eq!(print_count, 500, "should print 500 euro signs");
    assert_eq!(lf_count, 500, "should emit 500 line feeds");
}

#[test]
fn sgr_rapid_color_cycling_stress() {
    // Rapid SGR color changes followed by text - common in colored terminal output
    let mut parser = Parser::default();
    let mut input = Vec::new();
    for i in 0..500u16 {
        // 256-color foreground
        let seq = format!("\x1b[38;5;{i}m.");
        input.extend_from_slice(seq.as_bytes());
    }
    // Reset at end
    input.extend_from_slice(b"\x1b[0m");

    let actions = parser.feed(&input);
    let sgr_count = actions
        .iter()
        .filter(|a| matches!(a, ParserAction::SetGraphicsRendition(_)))
        .count();
    // 500 color sets + 1 reset = 501
    assert_eq!(sgr_count, 501);
}

#[test]
fn incomplete_sequences_across_multiple_feeds() {
    // Feed sequences byte-by-byte and verify they complete correctly
    let full_seq = b"\x1b[38;2;255;128;0mHello\x1b[0m";
    let mut parser = Parser::default();
    let mut all_actions = Vec::new();

    for &byte in full_seq.iter() {
        let actions = parser.feed(&[byte]);
        all_actions.extend(actions);
    }

    // Should have: SGR(38;2;255;128;0), "Hello" chars, SGR(0)
    let sgr_count = all_actions
        .iter()
        .filter(|a| matches!(a, ParserAction::SetGraphicsRendition(_)))
        .count();
    assert_eq!(sgr_count, 2, "should have 2 SGR actions");

    let text_count: usize = all_actions
        .iter()
        .map(|a| match a {
            ParserAction::Print(_) => 1,
            ParserAction::PrintText(t) => t.chars().count(),
            _ => 0,
        })
        .sum();
    assert_eq!(text_count, 5, "should print 5 chars of 'Hello'");
}

#[test]
fn csi_all_final_bytes_no_panic() {
    // All valid CSI final bytes (0x40-0x7E) should be handled without panic
    let mut parser = Parser::default();
    for final_byte in 0x40u8..=0x7E {
        let seq = [0x1B, b'[', b'1', final_byte];
        let _actions = parser.feed(&seq);
        // No panic is the assertion
    }

    // Parser should be in ground state
    let after = parser.feed(b"OK");
    assert!(
        after
            .iter()
            .any(|a| matches!(a, ParserAction::PrintText(t) if t == "OK")),
    );
}

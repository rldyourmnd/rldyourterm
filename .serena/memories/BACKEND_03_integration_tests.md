<!-- Memory Metadata
Last updated: 2026-03-10
Last commit: a6c5c87 fix(ci): clippy warnings in integration-tests, VSA layer mapping
Scope: crates/integration-tests/
Area: BACKEND
-->

# Integration Test Layer

## Overview
Separate VSA layer (`crates/integration-tests/`) providing black-box testing of terminal emulation via public API only. Depends exclusively on `rldyourterm-core`, uses `TerminalState::feed_terminal_responses_into()` (pub) rather than internal `feed()` (pub(crate)). Total: 138 tests across 8 scenario files.

## Architecture
- **VSA layer**: Independent test crate, no access to internal core implementation
- **Entry point**: `feed_terminal_responses_into()` from `TerminalState` (public API boundary)
- **Test harness utilities**: `term()`, `term_sized()`, `term_full()`, `feed()`, `feed_bytes()`, `grid_content()`, `row()`
- **Publish status**: `publish = false` in Cargo.toml

## Key Files
- `crates/integration-tests/Cargo.toml` - depends only on `rldyourterm-core.workspace = true`
- `crates/integration-tests/src/lib.rs` - 59 lines, test harness utilities
- `crates/integration-tests/tests/ai_cli_compatibility.rs` - 16 tests, AI CLI scenarios
- `crates/integration-tests/tests/parser_edge_cases.rs` - 24 tests, parser edge cases
- `crates/integration-tests/tests/grid_boundary.rs` - 22 tests, grid boundary conditions
- `crates/integration-tests/tests/terminal_modes.rs` - 27 tests, terminal mode handling
- `crates/integration-tests/tests/reflow_resize.rs` - 17 tests, resize and reflow
- `crates/integration-tests/tests/scrollback_pressure.rs` - 11 tests, scrollback cap enforcement
- `crates/integration-tests/tests/stress_throughput.rs` - 12 tests, high-throughput scenarios
- `crates/integration-tests/tests/stress_memory.rs` - 9 tests, memory stability

## Test Scenarios (8 categories, 138 tests)

### ai_cli_compatibility (16 tests)
Target: fish, vim, Claude Code, tmux, starship compatibility patterns
- `fish_da1_query_response` - DA1 terminal query handling
- `fish_osc_7_cwd_tracking` - OSC 7 working directory tracking
- `fish_osc_133_shell_markers` - OSC 133 shell integration markers
- `fish_right_prompt_pattern` - fish right prompt rendering
- `starship_complex_prompt_rendering` - starship prompt parsing
- `vim_alternate_screen_lifecycle` - vim alt screen enter/exit
- `vim_cursor_shape_changes` - DECSCUSR cursor shapes
- `vim_mouse_mode_enable_disable` - mouse mode 1000/1003
- `vim_scroll_region_editing` - DECSTBM scroll regions
- `bracketed_paste_mode_toggle` - mode 2004 toggle
- `focus_reporting_toggle` - mode 1004 focus events
- `synchronized_output_mode` - mode 2026 BSU/ESU
- `alternate_screen_preserves_scrollback` - alt screen isolation
- `long_streaming_text_with_sgr_colors` - AI output streaming
- `streaming_text_mixed_with_cursor_movement` - mixed cursor/text
- `rapid_line_output_with_scrollback` - fast line output

### parser_edge_cases (24 tests)
Target: CSI truncation, UTF-8 boundaries, OSC, control chars, DA1/DSR
- `truncated_csi_discarded_cleanly` - incomplete CSI handling
- `incomplete_utf8_produces_replacement` - UTF-8 decode error
- `utf8_split_across_feeds` - multi-feed UTF-8 continuation
- `mixed_ascii_and_cjk` - mixed width character handling
- `empty_csi_params_use_defaults` - default param resolution
- `csi_with_only_semicolons_uses_defaults` - semicolon-only params
- `csi_with_too_many_params_is_discarded` - param overflow
- `csi_with_large_numeric_param_clamped` - param clamping
- `escape_within_csi_aborts_csi` - CSI abort sequences
- `osc_terminated_by_bel` / `osc_terminated_by_st` - OSC terminators
- `osc_with_empty_payload` - empty OSC handling
- `osc_7_file_uri_extracts_path` - OSC 7 URI parsing
- `osc_52_clipboard_set` - OSC 52 clipboard operations
- `da1_returns_correct_response` - DA1 response format
- `dsr_returns_cursor_position` - DSR cursor report
- `device_ok_returns_response` - DeviceOk response
- `sgr_rgb_foreground_and_background` - SGR RGB colors
- `multiple_sgr_in_one_sequence` - batch SGR processing
- `rapid_sgr_mode_cycling` - fast mode toggling
- `backspace_moves_cursor_left` / `backspace_stops_at_column_zero`
- `carriage_return_homes_to_column_zero`
- `bell_sets_pending_flag` - bell side-channel

### grid_boundary (22 tests)
Target: 1x1 grid, wide chars at edges, max dimensions, erase/insert/delete
- `grid_1x1_basic_operations` / `grid_1x1_cursor_movement_clamped`
- `grid_1x1_erase_operations` / `grid_1x1_wrap_and_scroll`
- `grid_1x2_single_column` / `grid_2x1_narrow_terminal`
- `large_grid_dimensions` - max size handling
- `wide_char_at_last_column_wraps` - wide char wrap
- `wide_char_exactly_fills_row` / `wide_char_overwrites_at_boundary`
- `wide_char_on_1_column_grid_skips` - narrow grid wide char
- `cursor_movement_clamps_to_grid` - boundary clamping
- `erase_in_display_from_cursor_at_origin` / `erase_in_display_to_cursor_at_end`
- `erase_line_variants` - EL0/EL1/EL2
- `delete_chars_at_first_column` / `insert_chars_at_last_column`
- `delete_lines_at_grid_top` / `insert_lines_at_grid_bottom`
- `scroll_region_full_grid` / `scroll_region_single_row`
- `scroll_region_reset_by_decstbm_without_params` - DECSTBM reset

### terminal_modes (27 tests)
Target: mode stacking, alt screen, tab stops, cursor shape, REP
- `alternate_screen_clears_on_enter` / `alternate_screen_modes_persist`
- `alternate_screen_saves_restores_cursor_position`
- `alternate_screen_scroll_region_reset_on_exit`
- `auto_wrap_enabled_wraps_to_next_line` / `auto_wrap_disabled_stays_at_last_column`
- `cursor_save_restore_via_esc_7_8` / `cursor_save_restore_preserves_pen_attributes`
- `cursor_shape_all_decscusr_values` - block/underline/bar
- `cursor_shape_preserved_through_text_output`
- `default_tab_stops_every_8_columns`
- `custom_tab_stop_via_hts` - HTS (ESC H)
- `tab_clear_current_position` / `tab_clear_all` - TBC (CSI g)
- `tab_stops_survive_resize_width_expansion`
- `rep_repeats_last_printed_character` / `rep_after_csi_uses_last_printed_char`
- `rep_with_no_prior_char_is_noop` - CSI b (REP)
- `mouse_mode_independent_of_mouse_format` - 1000/1002/1003
- `mouse_mode_upgrade_path`
- `focus_reporting_toggle` - mode 1004
- `bracketed_paste_survives_sgr_reset` - mode 2004
- `synchronized_output_toggle` - mode 2026
- `da1_response_is_consistent`
- `dsr_cursor_position_at_boundaries`
- `window_title_osc_0_and_osc_2` / `window_title_with_special_characters`

### reflow_resize (17 tests)
Target: shrink/expand, wide char reflow, cursor tracking, scrollback overflow
- `resize_empty_grid` / `resize_same_dimensions_is_noop`
- `shrink_wraps_long_line` / `shrink_preserves_multiple_logical_lines`
- `shrink_pushes_overflow_to_scrollback`
- `expand_rejoins_wrapped_lines` / `expand_does_not_rejoin_hard_newlines`
- `shrink_expand_roundtrip_preserves_content`
- `cursor_tracks_through_shrink` / `cursor_tracks_through_expand`
- `cursor_at_origin_stays_at_origin`
- `cursor_not_lost_in_scrollback_overflow`
- `reflow_overflow_populates_scrollback`
- `wide_char_at_reflow_boundary_wraps_correctly`
- `wide_char_reflow_on_shrink` / `wide_char_expand_rejoins`
- `multiple_resize_cycles`

### scrollback_pressure (11 tests)
Target: cap enforcement, FIFO, Unicode, alt screen isolation
- `scrollback_cap_exact_boundary` / `scrollback_cap_one_never_exceeds`
- `scrollback_fifo_ordering`
- `scrollback_preserves_trimmed_content` / `scrollback_preserves_unicode_content`
- `scrollback_handles_empty_lines` / `scrollback_with_long_lines`
- `scrollback_frozen_during_alternate_screen`
- `scrollback_clear_via_erase_display_3` - ED3 clears scrollback
- `interleaved_push_and_read` / `scrollback_iter_during_churn`

### stress_throughput (12 tests)
Target: 10MB ASCII, 1MB CSI+text, 10K CJK, rapid feeds, DA1 burst
- `ingest_10mb_ascii_stream`
- `ingest_1mb_mixed_csi_and_text`
- `cjk_flood_10k_characters`
- `mixed_width_character_flood`
- `rapid_newline_flood` / `rapid_small_feeds`
- `cursor_movement_storm`
- `repeated_full_screen_redraws`
- `scroll_region_stress`
- `sgr_attribute_cycling_throughput`
- `osc_title_update_burst`
- `da1_response_burst`

### stress_memory (9 tests)
Target: scrollback churn, resize oscillation, erase cycles, AI session sim
- `scrollback_churn_at_cap` / `scrollback_zero_cap_never_grows`
- `scrollback_byte_budget_enforcement`
- `resize_shrink_expand_preserves_content`
- `resize_oscillation_does_not_leak`
- `extreme_resize_values`
- `erase_display_cycles`
- `alternate_screen_cycles_do_not_leak`
- `simulated_ai_session_stability`

## Patterns and Conventions

### Entry Point Pattern
```rust
// CORRECT: Use public API
state.feed_terminal_responses_into(bytes, &mut responses);

// WRONG: pub(crate) not accessible from integration layer
state.feed(bytes);  // NOT AVAILABLE
```

### Alt Screen Isolation Pattern
```rust
// Alt screen replaces scrollback with cap=0 (check after exit, not during)
feed_bytes(&mut t, b"\x1b[?1049h");  // enter alt screen
// t.scrollback.cap() == 0 during alt screen
feed_bytes(&mut t, b"\x1b[?1049l");  // exit alt screen
// t.scrollback.cap() restored after exit
```

### Reflow Cursor Clamp Pattern
```rust
// Reflow clamps cursor to last content position (trimmed logical line)
// When reflow trims trailing whitespace, cursor adjusts to new line end
```

### N Lines with \r\n on H-row Grid Pattern
```rust
// When feeding N lines with \r\n to an H-row grid:
// Last \n triggers scroll -> visible content starts at row N-H+1
// Use grid_content() to verify visible portion
```

## Dependencies
- `rldyourterm-core.workspace = true` (only dependency)

## Current State
- 138 integration tests, 0 failures
- CI: runs via `cargo test -p rldyourterm-integration-tests`
- All tests use public API boundary exclusively
- CI gated on integration test pass (ci-gate job requires all test jobs)

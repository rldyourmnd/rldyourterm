<!-- Memory Metadata
last updated: 2026-03-10
a6c5c87 fix(ci): clippy warnings in integration-tests, VSA layer mapping
Scope: entire codebase,/
Area: BACKEND
-->

# Core Domain Model (crates/core)

## Module Structure (decomposed 2026-03-09)
Core crate modules are now directories with submodules for better organization:

### `grid/` directory
- `mod.rs` - Grid struct, Cell type, dirty-row tracking, resize, clear operations
- `operations.rs` - Grid mutation methods: put_char, scroll_up/down, insert/delete chars/lines, erase
- `tests.rs` - Unit tests for grid operations
- `tests_stress.rs` - Stress tests for grid under load

### `parser/` directory
- `mod.rs` - Parser state machine, ParseState enum, feed logic, CsiParams struct
- `csi.rs` - CSI sequence handling and parameter parsing
- `handlers.rs` - Sequence dispatch handlers (private CSI, SGR, etc.)
- `tests_basic.rs` - Basic parsing tests
- `tests_advanced.rs` - Advanced sequence handling tests

### `state/` directory
- `mod.rs` - TerminalState struct, feed() entry point, public API
- `actions.rs` - ParserAction enum, apply_action_into implementation
- `dispatch.rs` - Action dispatch routing to specific apply_* methods
- `tests_feed.rs` - Feed behavior tests
- `tests_modes.rs` - Terminal mode tests (alt screen, cursor keys, etc.)
- `tests_stress.rs` - Stress tests for terminal state

### Single-file modules
- `cursor.rs` - Cursor position tracking
- `scrollback.rs` - Scrollback buffer with configurable cap (50,000 lines default)
- `events.rs` - CoreEvent enum variants
- `error.rs` - CoreError enum
- `render_mode.rs` - RenderMode enum (Cpu, Gpu, Auto); domain concept, re-exported by services

## Key Types
- `Grid` - 2D cell array with dirty-row tracking for incremental rendering; holds `dirty_rows: Vec<bool>` initialized all-true on construction
- `RenderMode` - enum (Cpu, Gpu, Auto); domain concept in core, re-exported from services
- `Cell` - single character cell (field: ch)
- `CsiParams` - stack-allocated CSI parameter array: `[Option<u16>; MAX_CSI_PARAMS]` with `len: u8` (32 slots; ECMA-48 limits CSI to 16 params); zero heap allocation and O(1) access
- `Parser` - state machine: ParseState (Ground/Escape/Csi/CsiDiscard), produces ParserAction
- `TerminalState` - composes Grid + Cursor + Scrollback + Parser, processes byte feeds
- `ParserAction` - enum: Print(char), PrintText(String), LineFeed, CarriageReturn, Backspace, Tab, Bell, cursor movement/position variants, ClearDisplay, ClearLine, SetGraphicsRendition, CursorSavePosition, CursorRestorePosition, SetCursorVisible, AlternateScreenEnter/Leave, InsertLines, DeleteLines, ScrollUp/Down, EraseCharacters, InsertCharacters, DeleteCharacters, SetScrollRegion, SetWindowTitle, BracketedPasteMode, ApplicationCursorKeys, AutoWrapMode, SendPrimaryDA, SendDeviceStatusReport, SendDeviceOk, UnsupportedSequence, IngestDegraded
  - `PrintText(String)`: emitted for contiguous text >= 2 characters, replacing per-character `Print(char)` emission; reduces Vec pushes from ~4000 to ~1 for a 4 KB ASCII block; single characters still use `Print(char)` to avoid allocation

## Parser CSI Multi-Param Dispatch
`dispatch_private_csi` handles multi-mode private CSI sequences like `\x1b[?1;25h` which sets both `ApplicationCursorKeys` AND `SetCursorVisible` in a single escape. Previously only the first parameter was processed. Tests: `multi_param_private_csi_dispatches_all_modes`, `multi_param_private_csi_reset`.

## Grid Dirty-Row Tracking API
All mutating `Grid` methods call `mark_row_dirty(row)` (private) or `mark_all_dirty()` internally. The caller never needs to mark rows manually.

| Method | Signature | Notes |
|--------|-----------|-------|
| `has_dirty_rows` | `(&self) -> bool` | True if any row is dirty |
| `dirty_rows` | `(&self) -> &[bool]` | Borrow the dirty flags (read by renderer, cleared separately) |
| `take_dirty_rows` | `(&mut self) -> Vec<u16>` | Returns row indices that are dirty AND clears all flags |
| `clear_dirty_rows` | `(&mut self)` | Clears dirty flags in-place without allocation (3a4685e); use when renderer has already consumed `dirty_rows()` by reference |
| `mark_all_dirty` | `(&mut self)` | Sets every row dirty (called on clear, scroll, resize) |

Mutating methods that mark individual rows: `put_char`, `clear_row*`, `erase_chars`, `insert_chars`, `delete_chars`.
Mutating methods that mark all rows: `clear`, `scroll_up`, `scroll_up_region`, `scroll_down_region`, `resize`.

Callers: `gui_runtime` reads `grid.dirty_rows()` before GPU render. After successful GPU present, calls `clear_dirty_rows()` (3ba0d8b, allocation-free) instead of `take_dirty_rows()` since indices were already consumed.

## Data Flow
```
bytes -> TerminalState.feed() -> Parser.feed() -> [ParserAction] -> apply_action_into() -> Grid mutations + CoreEvents
```

`Parser.feed()` pre-allocates the actions Vec with `bytes.len() / 2` capacity. The private `emit_text` helper determines whether to emit `Print(char)` (single character) or `PrintText(String)` (multi-character run). `apply_action_into` handles `PrintText` by iterating chars and calling `apply_print` for each.

## Terminal Query-Response System
The core domain handles query-response sequences from the host application (e.g., DA1, DSR). Parser emits `SendPrimaryDA`, `SendDeviceStatusReport`, or `SendDeviceOk` actions; `apply_action_into` generates `CoreEvent::TerminalResponse { data: Vec<u8> }` with the response bytes. The caller (gui_runtime) is responsible for writing these bytes back to the PTY writer.

- `SendPrimaryDA` (CSI `c` / `0c`) -> response: `\x1b[?1;2c`
- `SendDeviceStatusReport` (CSI `6n`) -> response: `\x1b[{row};{col}R` (1-based cursor position)
- `SendDeviceOk` (CSI `5n`) -> response: `\x1b[0n`

## CoreEvent Variants
- `CellUpdated`, `CursorMoved`, `LineWrapped`, `GridScrolled`, `ScrollbackTrimmed` - display state changes (defined in events.rs; `CellUpdated` and `CursorMoved` are no longer emitted in the hot path as of 7a5f24c - only emitted by `apply_set_cursor_position` and explicit CSI cursor-move sequences)
- `DisplayCleared`, `LineCleared`, `Bell`, `CursorVisibilityChanged` - terminal control events
- `AlternateScreenEntered`, `AlternateScreenLeft`, `WindowTitleChanged` - mode/metadata events
- `TerminalResponse { data: Vec<u8> }` - response bytes that must be written back to PTY
- `UnsupportedSequenceIgnored`, `IngestDegraded` - diagnostic events

## Safety Boundaries
- MAX_FEED_BYTES_PER_CALL / FEED_CHUNK_BYTES - bounds input per feed call
- MAX_CSI_LEN - oversized CSI sequences are discarded with Degrade action
- MAX_OSC_LEN - oversized OSC sequences are discarded
- Parser resync after truncation via resync_after_truncation()
- Malformed UTF-8 produces REPLACEMENT_CHAR, never panics
- Tab arithmetic: `apply_tab` uses `saturating_add`/`saturating_mul` to prevent u16 overflow when cursor.col >= 65528 (d127d5b)
- Alt screen resize: `resize()` clamps saved main-screen cursor in `alt.cursor`; `apply_alternate_screen_leave()` adds defensive clamp; prevents `InvalidGridPosition` on leave after resize (0dd2fa2)

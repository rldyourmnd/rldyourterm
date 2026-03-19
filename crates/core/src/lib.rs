// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

mod cursor;
pub mod error;
mod events;
pub use events::{CoreEvent, DisplayClearMode, IngestDegradeReason, LineClearMode};

mod grid;
mod input;
mod parser;
pub use parser::ShellMarkerKind;
mod render_mode;
mod scrollback;
mod search;
mod state;

pub use cursor::Cursor;
pub use grid::{
    ANSI_PALETTE, Attrs, CELL_HEIGHT, CELL_WIDTH, Cell, CellText, Color, DEFAULT_BG, DEFAULT_FG,
    Grid, TerminalTheme, UnderlineStyle, color_to_u32,
};
pub use input::{
    RuntimeKey, RuntimeKeyEvent, RuntimeKeyEventKind, RuntimeKeyModifiers, TerminalModeFlags,
    encode_runtime_key_event,
};
// Keep parser module private while preserving a narrow tooling surface for direct fuzz harnesses.
pub use parser::Parser;
pub use render_mode::RenderMode;
pub use scrollback::{MAX_SCROLLBACK_CAP, Scrollback};
pub use search::{SearchError, SearchMatch};
pub use state::{MAX_FEED_BYTES_PER_CALL, MouseFormat, MouseMode, TerminalState};

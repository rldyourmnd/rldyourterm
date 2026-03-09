mod cursor;
pub mod error;
mod events;
mod grid;
mod parser;
mod render_mode;
mod scrollback;
mod state;

pub use cursor::Cursor;
pub use grid::{
    ANSI_PALETTE, Attrs, CELL_HEIGHT, CELL_WIDTH, Cell, Color, DEFAULT_BG, DEFAULT_FG, Grid,
    color_to_u32,
};
// Keep parser module private while preserving a narrow tooling surface for direct fuzz harnesses.
pub use parser::Parser;
pub use render_mode::RenderMode;
pub use scrollback::{MAX_SCROLLBACK_CAP, Scrollback};
pub use state::{MAX_FEED_BYTES_PER_CALL, MouseFormat, MouseMode, TerminalState};

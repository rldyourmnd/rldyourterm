use crate::cursor::Cursor;
use crate::grid::Attrs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestDegradeReason {
    InputFeedTooLarge,
    CsiSequenceTooLong,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayClearMode {
    Below,
    Above,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineClearMode {
    Right,
    Left,
    All,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreEvent {
    CellUpdated {
        row: u16,
        col: u16,
        ch: char,
        attrs: Attrs,
    },
    CursorMoved {
        from: Cursor,
        to: Cursor,
    },
    LineWrapped {
        row: u16,
    },
    GridScrolled {
        lines: u16,
    },
    ScrollbackTrimmed {
        dropped: usize,
    },
    DisplayCleared {
        mode: DisplayClearMode,
    },
    LineCleared {
        row: u16,
        mode: LineClearMode,
    },
    Bell,
    CursorVisibilityChanged {
        visible: bool,
    },
    AlternateScreenEntered,
    AlternateScreenLeft,
    WindowTitleChanged {
        title: String,
    },
    TerminalResponse {
        data: Vec<u8>,
    },
    UnsupportedSequenceIgnored {
        sequence: String,
    },
    IngestDegraded {
        reason: IngestDegradeReason,
        accepted: usize,
        dropped: usize,
    },
}

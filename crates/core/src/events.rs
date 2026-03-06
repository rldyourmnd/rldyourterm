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
    Scrollback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineClearMode {
    Right,
    Left,
    All,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreEvent {
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

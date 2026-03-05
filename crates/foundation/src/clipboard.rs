#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardHealth {
    Available,
    Degraded,
    Unavailable,
}

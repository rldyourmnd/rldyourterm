use crate::api::common::ContractResult;

pub trait ClipboardAdapter: Send + Sync {
    fn set_text(&self, text: &str) -> ContractResult<()>;
    fn get_text(&self) -> ContractResult<Option<String>>;
    fn clear(&self) -> ContractResult<()>;
}

#[deprecated(note = "Use ClipboardAdapter::set_text returning ContractResult<()>")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardSetOutcome {
    Stored,
    ReplacedExisting,
}

#[deprecated(note = "Use ClipboardAdapter::get_text returning ContractResult<Option<String>>")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardGetOutcome {
    Text(String),
    Empty,
}

#[deprecated(note = "Use ClipboardAdapter::clear returning ContractResult<()>")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardClearOutcome {
    Cleared,
    AlreadyEmpty,
}

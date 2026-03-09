use std::io::{Read, Write};
use std::path::PathBuf;

use crate::api::common::ContractResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtySize {
    pub cols: u16,
    pub rows: u16,
    pub pixel_width: u16,
    pub pixel_height: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PtySpawnConfig {
    pub shell_command: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: Vec<(String, String)>,
    pub size: PtySize,
}

pub trait PtyIo: Send + Sync {
    // Reader contract: at most one successful acquisition for a PTY lifecycle.
    // Runtime callers that lose the reader must terminate or recreate the PTY session;
    // this adapter surface does not expose reader cloning/reacquisition.
    fn take_reader(&self) -> ContractResult<Box<dyn Read + Send>>;

    // Single-writer contract: at most one successful acquisition for a PTY lifecycle.
    fn take_writer(&self) -> ContractResult<Box<dyn Write + Send>>;

    fn resize(&self, size: PtySize) -> ContractResult<()>;
    fn kill(&self) -> ContractResult<()>;
    fn wait(&self) -> ContractResult<i32>;
    fn try_wait(&self) -> ContractResult<Option<i32>>;
    fn close(&self) -> ContractResult<()>;
}

pub trait PtyFactory: Send + Sync {
    fn spawn(&self, config: PtySpawnConfig) -> ContractResult<Box<dyn PtyIo>>;
}

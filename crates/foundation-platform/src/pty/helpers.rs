use std::io::{self, ErrorKind};
use std::thread;
use std::time::Duration;

use portable_pty::{Child, ChildKiller};
use rldyourterm_foundation::error::{
    FoundationError, PtyFailureCode, PtyOperation, Recoverability,
};

pub const POST_KILL_REAP_ATTEMPTS: usize = 5;
pub const POST_KILL_REAP_BACKOFF: Duration = Duration::from_millis(25);

pub(super) fn normalize_exit_code(code: u32) -> i32 {
    i32::try_from(code).unwrap_or(i32::MAX)
}

pub(super) fn bounded_spawn_cleanup(
    killer: &mut Box<dyn ChildKiller + Send + Sync>,
    child: &mut Box<dyn Child + Send + Sync>,
) {
    let _ = killer.kill();
    for attempt in 0..POST_KILL_REAP_ATTEMPTS {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => {}
            Err(error) if is_child_lifecycle_race(&error) => return,
            Err(_) => return,
        }
        if attempt + 1 < POST_KILL_REAP_ATTEMPTS {
            thread::sleep(POST_KILL_REAP_BACKOFF);
        }
    }
}

pub(super) fn io_failure(
    operation: PtyOperation,
    error: impl std::fmt::Display,
) -> FoundationError {
    FoundationError::pty(
        operation,
        PtyFailureCode::IoFailure,
        Recoverability::Degrade,
        format!("{error}"),
        None,
    )
}

pub(super) fn child_failure(operation: PtyOperation, error: io::Error) -> FoundationError {
    if is_child_lifecycle_race(&error) {
        lifecycle_boundary_fault(operation, &error)
    } else {
        io_failure(operation, error)
    }
}

pub(super) fn lifecycle_boundary_fault(
    operation: PtyOperation,
    error: &io::Error,
) -> FoundationError {
    FoundationError::pty(
        operation,
        PtyFailureCode::BoundaryFault,
        Recoverability::Degrade,
        format!("pty lifecycle boundary race: {error}"),
        None,
    )
}

pub(super) fn is_child_lifecycle_race(error: &io::Error) -> bool {
    is_child_gone(error) || is_os_missing_child(error)
}

fn is_child_gone(error: &io::Error) -> bool {
    matches!(error.kind(), ErrorKind::NotFound) || is_pty_eof(error)
}

#[cfg(unix)]
fn is_os_missing_child(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(3) | Some(10))
}

#[cfg(not(unix))]
fn is_os_missing_child(_error: &io::Error) -> bool {
    false
}

fn is_pty_eof(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::UnexpectedEof | ErrorKind::BrokenPipe | ErrorKind::NotConnected
    ) || error.raw_os_error() == Some(5)
}

pub(super) fn single_writer_violation(operation: PtyOperation) -> FoundationError {
    FoundationError::pty(
        operation,
        PtyFailureCode::SingleWriterInvariantViolation,
        Recoverability::Degrade,
        "pty writer is already acquired",
        None,
    )
}

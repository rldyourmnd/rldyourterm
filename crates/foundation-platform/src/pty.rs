use std::io::{self, ErrorKind, Read, Write};
use std::sync::{Mutex, MutexGuard};

use portable_pty::{CommandBuilder, MasterPty, PtySize as PortablePtySize, native_pty_system};
use rldyourterm_foundation::api::pty::{PtyFactory, PtyIo, PtySize, PtySpawnConfig};
use rldyourterm_foundation::error::{
    FoundationError, FoundationResult, PtyFailureCode, PtyOperation, Recoverability,
};

// Reserved sentinel when the child is known to be gone but no exit status can be retrieved.
const UNKNOWN_EXIT_CODE: i32 = i32::MIN;

#[derive(Debug, Default)]
pub struct PlatformPtyFactory;

struct PtyInner {
    master: Box<dyn MasterPty + Send>,
    reader: Option<Box<dyn Read + Send>>,
    writer: Option<Box<dyn Write + Send>>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    exit_code: Option<i32>,
    last_size: PtySize,
    closed: bool,
}

pub struct PlatformPtyIo {
    inner: Mutex<PtyInner>,
}

impl PlatformPtyIo {
    fn lock_inner(&self, operation: PtyOperation) -> FoundationResult<MutexGuard<'_, PtyInner>> {
        self.inner.lock().map_err(|_| {
            FoundationError::pty(
                operation,
                PtyFailureCode::BoundaryFault,
                Recoverability::Fatal,
                "pty state lock poisoned",
                None,
            )
        })
    }

    fn close_handles(inner: &mut PtyInner) {
        inner.closed = true;
        inner.reader = None;
        inner.writer = None;
    }

    fn cache_exit_code(inner: &mut PtyInner, code: i32) -> i32 {
        inner.exit_code = Some(code);
        Self::close_handles(inner);
        code
    }

    fn cache_unknown_exit(inner: &mut PtyInner, operation: PtyOperation, error: &io::Error) -> i32 {
        let mapped = lifecycle_boundary_fault(operation, error);
        tracing::warn!(
            error = %mapped,
            "pty child lifecycle race detected; using unknown exit code"
        );
        Self::cache_exit_code(inner, UNKNOWN_EXIT_CODE)
    }

    fn refresh_exit_state(inner: &mut PtyInner) -> FoundationResult<Option<i32>> {
        if let Some(code) = inner.exit_code {
            return Ok(Some(code));
        }

        match inner.child.try_wait() {
            Ok(Some(status)) => {
                let code = Self::cache_exit_code(inner, normalize_exit_code(status.exit_code()));
                Ok(Some(code))
            }
            Ok(None) => Ok(None),
            Err(error) if is_child_lifecycle_race(&error) => {
                let code = Self::cache_unknown_exit(inner, PtyOperation::TryWait, &error);
                Ok(Some(code))
            }
            Err(error) => Err(child_failure(PtyOperation::TryWait, error)),
        }
    }

    fn terminate_child(inner: &mut PtyInner, operation: PtyOperation) -> FoundationResult<()> {
        Self::close_handles(inner);
        if inner.exit_code.is_some() {
            return Ok(());
        }

        match inner.child.kill() {
            Ok(()) => {}
            Err(error) if is_child_lifecycle_race(&error) => {
                let _ = Self::cache_unknown_exit(inner, operation, &error);
                return Ok(());
            }
            Err(error) => return Err(child_failure(operation, error)),
        }

        let _ = Self::refresh_exit_state(inner)?;
        Ok(())
    }

    fn wait_for_exit(inner: &mut PtyInner, operation: PtyOperation) -> FoundationResult<i32> {
        if let Some(code) = Self::refresh_exit_state(inner)? {
            return Ok(code);
        }

        match inner.child.wait() {
            Ok(status) => {
                let code = Self::cache_exit_code(inner, normalize_exit_code(status.exit_code()));
                Ok(code)
            }
            Err(error) if is_child_lifecycle_race(&error) => {
                let code = Self::cache_unknown_exit(inner, operation, &error);
                Ok(code)
            }
            Err(error) => Err(child_failure(operation, error)),
        }
    }

    fn ensure_open(inner: &PtyInner, operation: PtyOperation) -> FoundationResult<()> {
        if inner.closed || inner.exit_code.is_some() {
            return Err(FoundationError::pty(
                operation,
                PtyFailureCode::SessionClosed,
                Recoverability::Degrade,
                "pty session is closed",
                None,
            ));
        }
        Ok(())
    }
}

impl PtyIo for PlatformPtyIo {
    fn take_reader(&self) -> FoundationResult<Box<dyn Read + Send>> {
        let mut inner = self.lock_inner(PtyOperation::Read)?;
        let _ = Self::refresh_exit_state(&mut inner)?;
        Self::ensure_open(&inner, PtyOperation::Read)?;

        inner.reader.take().ok_or_else(|| {
            FoundationError::pty(
                PtyOperation::Read,
                PtyFailureCode::BoundaryFault,
                Recoverability::Degrade,
                "pty reader is already acquired",
                None,
            )
        })
    }

    fn take_writer(&self) -> FoundationResult<Box<dyn Write + Send>> {
        let mut inner = self.lock_inner(PtyOperation::AcquireWriterLease)?;
        let _ = Self::refresh_exit_state(&mut inner)?;
        Self::ensure_open(&inner, PtyOperation::AcquireWriterLease)?;

        inner.writer.take().ok_or_else(|| {
            FoundationError::pty(
                PtyOperation::AcquireWriterLease,
                PtyFailureCode::SingleWriterInvariantViolation,
                Recoverability::Degrade,
                "pty writer is already acquired",
                None,
            )
        })
    }

    fn resize(&self, size: PtySize) -> FoundationResult<()> {
        let mut inner = self.lock_inner(PtyOperation::Resize)?;
        let _ = Self::refresh_exit_state(&mut inner)?;
        Self::ensure_open(&inner, PtyOperation::Resize)?;

        let normalized = normalize_size(size);
        if normalized == inner.last_size {
            return Ok(());
        }

        inner
            .master
            .resize(to_portable_size(normalized))
            .map_err(|error| io_failure(PtyOperation::Resize, error))?;
        inner.last_size = normalized;
        Ok(())
    }

    fn kill(&self) -> FoundationResult<()> {
        let mut inner = self.lock_inner(PtyOperation::Kill)?;
        if Self::refresh_exit_state(&mut inner)?.is_some() {
            return Ok(());
        }

        Self::terminate_child(&mut inner, PtyOperation::Kill)
    }

    fn wait(&self) -> FoundationResult<i32> {
        let mut inner = self.lock_inner(PtyOperation::TryWait)?;
        Self::wait_for_exit(&mut inner, PtyOperation::TryWait)
    }

    fn try_wait(&self) -> FoundationResult<Option<i32>> {
        let mut inner = self.lock_inner(PtyOperation::TryWait)?;
        Self::refresh_exit_state(&mut inner)
    }

    fn close(&self) -> FoundationResult<()> {
        let mut inner = self.lock_inner(PtyOperation::Kill)?;
        if inner.exit_code.is_none() {
            Self::terminate_child(&mut inner, PtyOperation::Kill)?;
        } else {
            Self::close_handles(&mut inner);
        }

        Ok(())
    }
}

impl Drop for PlatformPtyIo {
    fn drop(&mut self) {
        if let Ok(mut inner) = self.inner.lock() {
            if inner.exit_code.is_none() {
                let _ = Self::terminate_child(&mut inner, PtyOperation::Kill);
            } else {
                Self::close_handles(&mut inner);
            }
        }
    }
}

impl PtyFactory for PlatformPtyFactory {
    fn spawn(&self, config: PtySpawnConfig) -> FoundationResult<Box<dyn PtyIo>> {
        if config.shell_command.trim().is_empty() {
            return Err(FoundationError::pty(
                PtyOperation::SpawnShell,
                PtyFailureCode::InvalidSpawnRequest,
                Recoverability::Fatal,
                "spawn config contains an empty shell command",
                None,
            ));
        }

        let normalized_size = normalize_size(config.size);
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(to_portable_size(normalized_size))
            .map_err(|error| io_failure(PtyOperation::SpawnShell, error))?;

        let mut command = CommandBuilder::new(&config.shell_command);
        command.args(&config.args);
        if let Some(cwd) = config.cwd.as_ref() {
            command.cwd(cwd.as_os_str());
        }
        for (key, value) in &config.env {
            command.env(key, value);
        }

        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| io_failure(PtyOperation::SpawnShell, error))?;

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| io_failure(PtyOperation::SpawnShell, error))?;

        let writer = pair
            .master
            .take_writer()
            .map_err(|error| io_failure(PtyOperation::SpawnShell, error))?;

        let inner = PtyInner {
            master: pair.master,
            reader: Some(reader),
            writer: Some(writer),
            child,
            exit_code: None,
            last_size: normalized_size,
            closed: false,
        };

        Ok(Box::new(PlatformPtyIo {
            inner: Mutex::new(inner),
        }))
    }
}

fn normalize_size(size: PtySize) -> PtySize {
    PtySize {
        rows: size.rows.max(1),
        cols: size.cols.max(1),
        pixel_width: size.pixel_width,
        pixel_height: size.pixel_height,
    }
}

fn to_portable_size(size: PtySize) -> PortablePtySize {
    PortablePtySize {
        rows: size.rows,
        cols: size.cols,
        pixel_width: size.pixel_width,
        pixel_height: size.pixel_height,
    }
}

fn normalize_exit_code(code: u32) -> i32 {
    i32::try_from(code).unwrap_or(i32::MAX)
}

fn io_failure(operation: PtyOperation, error: impl std::fmt::Display) -> FoundationError {
    FoundationError::pty(
        operation,
        PtyFailureCode::IoFailure,
        Recoverability::Degrade,
        format!("{error}"),
        None,
    )
}

fn child_failure(operation: PtyOperation, error: io::Error) -> FoundationError {
    if is_child_lifecycle_race(&error) {
        lifecycle_boundary_fault(operation, &error)
    } else {
        io_failure(operation, error)
    }
}

fn lifecycle_boundary_fault(operation: PtyOperation, error: &io::Error) -> FoundationError {
    FoundationError::pty(
        operation,
        PtyFailureCode::BoundaryFault,
        Recoverability::Degrade,
        format!("pty lifecycle boundary race: {error}"),
        None,
    )
}

fn is_child_gone(error: &io::Error) -> bool {
    matches!(error.kind(), ErrorKind::NotFound) || is_pty_eof(error)
}

fn is_child_lifecycle_race(error: &io::Error) -> bool {
    is_child_gone(error) || is_os_missing_child(error)
}

#[cfg(unix)]
fn is_os_missing_child(error: &io::Error) -> bool {
    // ESRCH (3): no such process, ECHILD (10): no child process.
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

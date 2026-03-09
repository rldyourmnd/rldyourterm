// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

mod helpers;

use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};
use std::thread;

use portable_pty::{
    Child, ChildKiller, CommandBuilder, MasterPty, PtySize as PortablePtySize, native_pty_system,
};
use rldyourterm_foundation::api::pty::{PtyFactory, PtyIo, PtySize, PtySpawnConfig};
use rldyourterm_foundation::error::{
    FoundationError, FoundationResult, PtyFailureCode, PtyOperation, Recoverability,
};

use helpers::{
    POST_KILL_REAP_ATTEMPTS, POST_KILL_REAP_BACKOFF, bounded_spawn_cleanup, child_failure,
    io_failure, is_child_lifecycle_race, lifecycle_boundary_fault, normalize_exit_code,
    single_writer_violation,
};

const UNKNOWN_EXIT_CODE: i32 = i32::MIN;

#[derive(Debug, Default)]
pub struct PlatformPtyFactory;

struct PtyProcess {
    child: Mutex<Box<dyn Child + Send + Sync>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
}

struct PtyInner {
    master: Box<dyn MasterPty + Send>,
    reader: Option<Box<dyn Read + Send>>,
    writer: Option<Box<dyn Write + Send>>,
    process: Arc<PtyProcess>,
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

    fn has_final_exit(inner: &PtyInner) -> bool {
        matches!(inner.exit_code, Some(code) if code != UNKNOWN_EXIT_CODE)
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

    fn lock_child<'a>(
        process: &'a Arc<PtyProcess>,
        operation: PtyOperation,
    ) -> FoundationResult<MutexGuard<'a, Box<dyn Child + Send + Sync>>> {
        process.child.lock().map_err(|_| {
            FoundationError::pty(
                operation,
                PtyFailureCode::BoundaryFault,
                Recoverability::Fatal,
                "pty child lock poisoned",
                None,
            )
        })
    }

    fn lock_killer<'a>(
        process: &'a Arc<PtyProcess>,
        operation: PtyOperation,
    ) -> FoundationResult<MutexGuard<'a, Box<dyn ChildKiller + Send + Sync>>> {
        process.killer.lock().map_err(|_| {
            FoundationError::pty(
                operation,
                PtyFailureCode::BoundaryFault,
                Recoverability::Fatal,
                "pty killer lock poisoned",
                None,
            )
        })
    }

    fn refresh_exit_state(
        inner: &mut PtyInner,
        operation: PtyOperation,
    ) -> FoundationResult<Option<i32>> {
        if let Some(code) = inner.exit_code
            && code != UNKNOWN_EXIT_CODE
        {
            return Ok(Some(code));
        }

        let wait_result = {
            let mut child = Self::lock_child(&inner.process, operation)?;
            child.try_wait()
        };

        match wait_result {
            Ok(Some(status)) => {
                let code = Self::cache_exit_code(inner, normalize_exit_code(status.exit_code()));
                Ok(Some(code))
            }
            Ok(None) => Ok(None),
            Err(error) if is_child_lifecycle_race(&error) => {
                let code = Self::cache_unknown_exit(inner, operation, &error);
                Ok(Some(code))
            }
            Err(error) => Err(child_failure(operation, error)),
        }
    }

    fn terminate_child(inner: &mut PtyInner, operation: PtyOperation) -> FoundationResult<()> {
        Self::close_handles(inner);
        if Self::has_final_exit(inner) {
            return Ok(());
        }

        let kill_result = {
            let mut killer = Self::lock_killer(&inner.process, operation)?;
            killer.kill()
        };
        match kill_result {
            Ok(()) => {}
            Err(error) if is_child_lifecycle_race(&error) => {
                let _ = Self::cache_unknown_exit(inner, operation, &error);
                return Ok(());
            }
            Err(error) => return Err(child_failure(operation, error)),
        }

        for attempt in 0..POST_KILL_REAP_ATTEMPTS {
            let wait_result = match inner.process.child.try_lock() {
                Ok(mut child) => child.try_wait(),
                Err(TryLockError::WouldBlock) => {
                    if attempt + 1 < POST_KILL_REAP_ATTEMPTS {
                        thread::sleep(POST_KILL_REAP_BACKOFF);
                        continue;
                    }
                    break;
                }
                Err(TryLockError::Poisoned(_)) => {
                    return Err(FoundationError::pty(
                        PtyOperation::TryWait,
                        PtyFailureCode::BoundaryFault,
                        Recoverability::Fatal,
                        "pty child lock poisoned",
                        None,
                    ));
                }
            };
            match wait_result {
                Ok(Some(status)) => {
                    let _ = Self::cache_exit_code(inner, normalize_exit_code(status.exit_code()));
                    return Ok(());
                }
                Ok(None) => {}
                Err(error) if is_child_lifecycle_race(&error) => {
                    let _ = Self::cache_unknown_exit(inner, operation, &error);
                    return Ok(());
                }
                Err(error) => return Err(child_failure(PtyOperation::TryWait, error)),
            }
            if attempt + 1 < POST_KILL_REAP_ATTEMPTS {
                thread::sleep(POST_KILL_REAP_BACKOFF);
            }
        }

        tracing::warn!(
            operation = %operation.as_str(),
            attempts = POST_KILL_REAP_ATTEMPTS,
            "pty child did not report exit after kill; exit remains unknown"
        );
        inner.exit_code = Some(UNKNOWN_EXIT_CODE);
        Ok(())
    }

    fn wait_for_exit(process: Arc<PtyProcess>, operation: PtyOperation) -> FoundationResult<i32> {
        let mut child = Self::lock_child(&process, operation)?;
        match child.wait() {
            Ok(status) => Ok(normalize_exit_code(status.exit_code())),
            Err(error) if is_child_lifecycle_race(&error) => Ok(UNKNOWN_EXIT_CODE),
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
        let _ = Self::refresh_exit_state(&mut inner, PtyOperation::TryWait)?;
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
        let _ = Self::refresh_exit_state(&mut inner, PtyOperation::TryWait)?;
        Self::ensure_open(&inner, PtyOperation::AcquireWriterLease)?;

        inner
            .writer
            .take()
            .ok_or_else(|| single_writer_violation(PtyOperation::AcquireWriterLease))
    }

    fn resize(&self, size: PtySize) -> FoundationResult<()> {
        let mut inner = self.lock_inner(PtyOperation::Resize)?;
        // Skip refresh_exit_state here — it acquires process.child mutex which
        // the wait_pump thread may hold indefinitely in wait_for_exit (deadlock).
        // ensure_open checks cached exit_code; if the child exited but the code
        // is not cached yet, master.resize() will return an IO error instead.
        Self::ensure_open(&inner, PtyOperation::Resize)?;

        let normalized = normalize_size(size);
        if normalized == inner.last_size {
            return Ok(());
        }

        if let Err(error) = inner.master.resize(to_portable_size(normalized)) {
            if let Some(io_error) = error.downcast_ref::<io::Error>()
                && is_child_lifecycle_race(io_error)
            {
                let _ = Self::cache_unknown_exit(&mut inner, PtyOperation::Resize, io_error);
                Self::close_handles(&mut inner);
                return Ok(());
            }
            return Err(io_failure(PtyOperation::Resize, error));
        }
        inner.last_size = normalized;
        Ok(())
    }

    fn kill(&self) -> FoundationResult<()> {
        let mut inner = self.lock_inner(PtyOperation::Kill)?;
        if Self::has_final_exit(&inner) {
            Self::close_handles(&mut inner);
            return Ok(());
        }

        Self::terminate_child(&mut inner, PtyOperation::Kill)
    }

    fn wait(&self) -> FoundationResult<i32> {
        let process = {
            let mut inner = self.lock_inner(PtyOperation::TryWait)?;
            if let Some(code) = Self::refresh_exit_state(&mut inner, PtyOperation::TryWait)? {
                return Ok(code);
            }
            Arc::clone(&inner.process)
        };

        match Self::wait_for_exit(process, PtyOperation::TryWait) {
            Ok(code) => {
                let mut inner = self.lock_inner(PtyOperation::TryWait)?;
                if !Self::has_final_exit(&inner) {
                    let _ = Self::cache_exit_code(&mut inner, code);
                }
                Ok(inner.exit_code.unwrap_or(code))
            }
            Err(error) => Err(error),
        }
    }

    fn try_wait(&self) -> FoundationResult<Option<i32>> {
        let mut inner = self.lock_inner(PtyOperation::TryWait)?;
        Self::refresh_exit_state(&mut inner, PtyOperation::TryWait)
    }

    fn close(&self) -> FoundationResult<()> {
        let mut inner = self.lock_inner(PtyOperation::Kill)?;
        if !Self::has_final_exit(&inner) {
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
            if !Self::has_final_exit(&inner) {
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

        let mut child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| io_failure(PtyOperation::SpawnShell, error))?;
        let mut killer = child.clone_killer();

        let reader = pair.master.try_clone_reader().map_err(|error| {
            bounded_spawn_cleanup(&mut killer, &mut child);
            io_failure(PtyOperation::SpawnShell, error)
        })?;

        let writer = pair.master.take_writer().map_err(|error| {
            bounded_spawn_cleanup(&mut killer, &mut child);
            io_failure(PtyOperation::SpawnShell, error)
        })?;

        let inner = PtyInner {
            master: pair.master,
            reader: Some(reader),
            writer: Some(writer),
            process: Arc::new(PtyProcess {
                child: Mutex::new(child),
                killer: Mutex::new(killer),
            }),
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

#[cfg(test)]
mod tests {
    use super::helpers::{child_failure, single_writer_violation};
    use rldyourterm_foundation::error::{
        FoundationError, PtyFailureCode, PtyOperation, Recoverability,
    };
    use std::io;

    #[test]
    fn single_writer_violation_contract_is_degrade_recoverable() {
        let error = single_writer_violation(PtyOperation::AcquireWriterLease);

        assert_eq!(
            error,
            FoundationError::Pty {
                operation: PtyOperation::AcquireWriterLease,
                code: PtyFailureCode::SingleWriterInvariantViolation,
                recoverability: Recoverability::Degrade,
                message: "pty writer is already acquired".to_string(),
                correlation_id: None,
            }
        );
        assert!(error.recoverability().is_recoverable());
    }

    #[test]
    fn resize_lifecycle_race_is_degrade_recoverable() {
        let error = child_failure(PtyOperation::Resize, io::Error::from_raw_os_error(3));

        assert_eq!(
            error,
            FoundationError::Pty {
                operation: PtyOperation::Resize,
                code: PtyFailureCode::BoundaryFault,
                recoverability: Recoverability::Degrade,
                message: "pty lifecycle boundary race: No such process (os error 3)".to_string(),
                correlation_id: None,
            }
        );
        assert!(error.recoverability().is_recoverable());
    }
}

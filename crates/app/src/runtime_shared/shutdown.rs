// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use std::{
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use tracing::warn;

pub(crate) const SHUTDOWN_JOIN_TIMEOUT: Duration = Duration::from_millis(750);
pub(crate) const SHUTDOWN_JOIN_POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JoinThreadOutcome {
    Joined,
    Panicked,
    TimedOut,
}

pub(crate) fn join_thread_with_timeout(
    handle: JoinHandle<()>,
    timeout: Duration,
    poll_interval: Duration,
    thread_label: &'static str,
) -> JoinThreadOutcome {
    let poll_interval = poll_interval.max(Duration::from_millis(1));
    let deadline = Instant::now() + timeout;
    while !handle.is_finished() && Instant::now() < deadline {
        thread::sleep(poll_interval);
    }

    if handle.is_finished() {
        return match handle.join() {
            Ok(()) => JoinThreadOutcome::Joined,
            Err(join_error) => {
                warn!(?join_error, thread_label, "shutdown thread join failed");
                JoinThreadOutcome::Panicked
            }
        };
    }

    JoinThreadOutcome::TimedOut
}

pub(crate) fn child_exit_drain_timed_out(
    started_at: Instant,
    now: Instant,
    max_wait: Duration,
) -> bool {
    now.saturating_duration_since(started_at) >= max_wait
}

#[cfg(test)]
mod tests {
    use super::{JoinThreadOutcome, child_exit_drain_timed_out, join_thread_with_timeout};
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn join_thread_with_timeout_returns_joined_for_finished_thread() {
        let handle = thread::spawn(|| {});
        let outcome = join_thread_with_timeout(
            handle,
            Duration::from_millis(100),
            Duration::from_millis(1),
            "test_joined",
        );
        assert_eq!(outcome, JoinThreadOutcome::Joined);
    }

    #[test]
    fn join_thread_with_timeout_returns_timed_out_for_busy_thread() {
        let handle = thread::spawn(|| thread::sleep(Duration::from_millis(50)));
        let outcome = join_thread_with_timeout(
            handle,
            Duration::from_millis(1),
            Duration::from_millis(1),
            "test_timeout",
        );
        assert_eq!(outcome, JoinThreadOutcome::TimedOut);
        thread::sleep(Duration::from_millis(60));
    }

    #[test]
    fn join_thread_with_timeout_detects_panicking_thread() {
        let handle = thread::spawn(|| panic!("panic for test"));
        let outcome = join_thread_with_timeout(
            handle,
            Duration::from_millis(100),
            Duration::from_millis(1),
            "test_panic",
        );
        assert_eq!(outcome, JoinThreadOutcome::Panicked);
    }

    #[test]
    fn child_exit_drain_timed_out_detects_expiry() {
        let started = Instant::now();
        let max_wait = Duration::from_millis(10);
        assert!(!child_exit_drain_timed_out(started, started, max_wait));
        assert!(child_exit_drain_timed_out(
            started,
            started + max_wait,
            max_wait
        ));
    }
}

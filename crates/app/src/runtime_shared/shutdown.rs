use std::{
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use tracing::warn;

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

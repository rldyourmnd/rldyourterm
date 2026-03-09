// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use std::io::{ErrorKind, Read};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rldyourterm_foundation::api::pty::PtyIo;
use rldyourterm_services::session::SessionBoundary;
use winit::event_loop::EventLoopProxy;

use super::{
    GuiEvent, OUTPUT_BATCH_MAX_BYTES, OUTPUT_DRAIN_CRITICAL_MAX_BYTES_PER_TICK,
    OUTPUT_DRAIN_CRITICAL_MAX_LATENCY, OUTPUT_DRAIN_CRITICAL_QUEUE_BYTES,
    OUTPUT_DRAIN_CRITICAL_QUEUE_CHUNKS, OUTPUT_DRAIN_ELEVATED_MAX_BYTES_PER_TICK,
    OUTPUT_DRAIN_ELEVATED_MAX_LATENCY, OUTPUT_DRAIN_ELEVATED_QUEUE_BYTES,
    OUTPUT_DRAIN_ELEVATED_QUEUE_CHUNKS, OUTPUT_DRAIN_MAX_BYTES_PER_TICK, OUTPUT_DRAIN_MAX_LATENCY,
    PTY_OUTPUT_CHUNK_BYTES, PTY_OUTPUT_RECYCLE_POOL_WARMUP,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct OutputQueueSnapshot {
    pub(super) queued_bytes: usize,
    pub(super) queued_chunks: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OutputDrainPressure {
    Normal,
    Elevated,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct OutputDrainBudget {
    pub(super) pressure: OutputDrainPressure,
    pub(super) max_bytes_per_tick: usize,
    pub(super) max_latency: Duration,
}

#[derive(Debug, Default)]
pub(super) struct OutputQueueBackpressure {
    queued_bytes: AtomicUsize,
    queued_chunks: AtomicUsize,
}

#[derive(Debug)]
pub(super) struct OutputChunk {
    buffer: Vec<u8>,
    len: usize,
}

impl OutputChunk {
    pub(super) fn new(buffer: Vec<u8>, len: usize) -> Self {
        Self { buffer, len }
    }

    pub(super) fn len(&self) -> usize {
        self.len
    }

    pub(super) fn as_bytes(&self) -> &[u8] {
        &self.buffer[..self.len]
    }

    pub(super) fn into_buffer(self) -> Vec<u8> {
        self.buffer
    }
}

impl OutputQueueBackpressure {
    pub(super) fn note_enqueue(&self, bytes: usize) {
        self.queued_bytes.fetch_add(bytes, Ordering::AcqRel);
        self.queued_chunks.fetch_add(1, Ordering::AcqRel);
    }

    pub(super) fn note_dequeue(&self, bytes: usize) {
        let _ = self
            .queued_bytes
            .fetch_update(Ordering::AcqRel, Ordering::Relaxed, |current| {
                Some(current.saturating_sub(bytes))
            });
        let _ = self
            .queued_chunks
            .fetch_update(Ordering::AcqRel, Ordering::Relaxed, |current| {
                Some(current.saturating_sub(1))
            });
    }

    pub(super) fn snapshot(&self) -> OutputQueueSnapshot {
        OutputQueueSnapshot {
            queued_bytes: self.queued_bytes.load(Ordering::Acquire),
            queued_chunks: self.queued_chunks.load(Ordering::Acquire),
        }
    }
}

pub(super) fn spawn_reader_pump(
    mut reader: Box<dyn Read + Send>,
    proxy: EventLoopProxy<GuiEvent>,
    output_tx: SyncSender<OutputChunk>,
    output_recycle_rx: Receiver<Vec<u8>>,
    output_event_pending: Arc<AtomicBool>,
    output_backpressure: Arc<OutputQueueBackpressure>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut buffer = take_output_chunk_buffer(&output_recycle_rx);

        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read_bytes) => {
                    output_backpressure.note_enqueue(read_bytes);
                    match output_tx.send(OutputChunk::new(buffer, read_bytes)) {
                        Ok(()) => {
                            if !output_event_pending.swap(true, Ordering::AcqRel)
                                && proxy.send_event(GuiEvent::OutputReady).is_err()
                            {
                                break;
                            }
                            buffer = take_output_chunk_buffer(&output_recycle_rx);
                        }
                        Err(send_error) => {
                            output_backpressure.note_dequeue(read_bytes);
                            drop(send_error.0.into_buffer());
                            break;
                        }
                    }
                }
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(error) => {
                    let _ = proxy.send_event(GuiEvent::PtyFailure {
                        boundary: SessionBoundary::PtyRead,
                        message: format!("PTY reader pump failed: {error}"),
                    });
                    break;
                }
            }
        }
    })
}

pub(super) fn spawn_wait_pump(
    pty: Arc<dyn PtyIo>,
    proxy: EventLoopProxy<GuiEvent>,
) -> JoinHandle<()> {
    thread::spawn(move || match pty.wait() {
        Ok(code) => {
            let _ = proxy.send_event(GuiEvent::Exited(code));
        }
        Err(error) => {
            let _ = proxy.send_event(GuiEvent::PtyFailure {
                boundary: SessionBoundary::PtyWait,
                message: format!("PTY wait failed: {error}"),
            });
        }
    })
}

pub(super) fn warm_output_chunk_pool(recycle_tx: &SyncSender<Vec<u8>>) {
    for _ in 0..PTY_OUTPUT_RECYCLE_POOL_WARMUP {
        if recycle_tx
            .try_send(vec![0_u8; PTY_OUTPUT_CHUNK_BYTES])
            .is_err()
        {
            break;
        }
    }
}

pub(super) fn take_output_chunk_buffer(recycle_rx: &Receiver<Vec<u8>>) -> Vec<u8> {
    let mut chunk = match recycle_rx.try_recv() {
        Ok(buffer) => buffer,
        Err(TryRecvError::Empty | TryRecvError::Disconnected) => vec![0_u8; PTY_OUTPUT_CHUNK_BYTES],
    };
    if chunk.len() != PTY_OUTPUT_CHUNK_BYTES {
        chunk.resize(PTY_OUTPUT_CHUNK_BYTES, 0);
    }
    chunk
}

pub(super) fn recycle_output_chunk_buffer(recycle_tx: &SyncSender<Vec<u8>>, mut chunk: Vec<u8>) {
    if chunk.len() != PTY_OUTPUT_CHUNK_BYTES {
        chunk.resize(PTY_OUTPUT_CHUNK_BYTES, 0);
    }
    let _ = recycle_tx.try_send(chunk);
}

pub(super) fn should_flush_output_batch(
    current_batch_len: usize,
    incoming_chunk_len: usize,
) -> bool {
    current_batch_len > 0
        && current_batch_len.saturating_add(incoming_chunk_len) > OUTPUT_BATCH_MAX_BYTES
}

pub(super) fn output_drain_budget(snapshot: OutputQueueSnapshot) -> OutputDrainBudget {
    if snapshot.queued_bytes >= OUTPUT_DRAIN_CRITICAL_QUEUE_BYTES
        || snapshot.queued_chunks >= OUTPUT_DRAIN_CRITICAL_QUEUE_CHUNKS
    {
        return OutputDrainBudget {
            pressure: OutputDrainPressure::Critical,
            max_bytes_per_tick: OUTPUT_DRAIN_CRITICAL_MAX_BYTES_PER_TICK,
            max_latency: OUTPUT_DRAIN_CRITICAL_MAX_LATENCY,
        };
    }

    if snapshot.queued_bytes >= OUTPUT_DRAIN_ELEVATED_QUEUE_BYTES
        || snapshot.queued_chunks >= OUTPUT_DRAIN_ELEVATED_QUEUE_CHUNKS
    {
        return OutputDrainBudget {
            pressure: OutputDrainPressure::Elevated,
            max_bytes_per_tick: OUTPUT_DRAIN_ELEVATED_MAX_BYTES_PER_TICK,
            max_latency: OUTPUT_DRAIN_ELEVATED_MAX_LATENCY,
        };
    }

    OutputDrainBudget {
        pressure: OutputDrainPressure::Normal,
        max_bytes_per_tick: OUTPUT_DRAIN_MAX_BYTES_PER_TICK,
        max_latency: OUTPUT_DRAIN_MAX_LATENCY,
    }
}

pub(super) fn output_drain_budget_exhausted(
    drained_bytes: usize,
    elapsed: Duration,
    budget: OutputDrainBudget,
) -> bool {
    drained_bytes >= budget.max_bytes_per_tick || elapsed >= budget.max_latency
}

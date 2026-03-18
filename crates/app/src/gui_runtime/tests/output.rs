// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use super::super::{
    CHILD_EXIT_DRAIN_MAX_WAIT, MAX_FEED_BYTES_PER_CALL, OUTPUT_BATCH_MAX_BYTES,
    OUTPUT_DRAIN_CRITICAL_MAX_BYTES_PER_TICK, OUTPUT_DRAIN_ELEVATED_MAX_BYTES_PER_TICK,
    OUTPUT_DRAIN_MAX_BYTES_PER_TICK, OUTPUT_DRAIN_MAX_LATENCY, OutputDrainBudget,
    OutputDrainPressure, OutputQueueSnapshot, PTY_OUTPUT_CHUNK_BYTES,
    PTY_OUTPUT_RECYCLE_POOL_WARMUP, child_exit_drain_timed_out, output_drain_budget,
    output_drain_budget_exhausted, recycle_output_chunk_buffer, should_flush_output_batch,
    take_output_chunk_buffer, terminal_feed_chunks, warm_output_chunk_pool,
};
use std::sync::mpsc::sync_channel;
use std::time::{Duration, Instant};

#[test]
fn output_batch_flush_policy_only_triggers_on_overflow_with_existing_batch() {
    assert!(!should_flush_output_batch(0, 32));
    assert!(!should_flush_output_batch(128, 256));
    assert!(should_flush_output_batch(OUTPUT_BATCH_MAX_BYTES - 64, 128));
}

#[test]
fn output_chunk_pool_warmup_stops_at_channel_capacity() {
    let (recycle_tx, recycle_rx) = sync_channel::<Vec<u8>>(2);
    warm_output_chunk_pool(&recycle_tx);
    let chunk_count = recycle_rx.try_iter().count();
    assert_eq!(chunk_count, PTY_OUTPUT_RECYCLE_POOL_WARMUP.min(2));
}

#[test]
fn output_chunk_take_reuses_preallocated_buffer_when_available() {
    let (recycle_tx, recycle_rx) = sync_channel::<Vec<u8>>(1);
    let mut seeded = vec![0_u8; PTY_OUTPUT_CHUNK_BYTES];
    let seeded_ptr = seeded.as_ptr();
    seeded.truncate(32);
    recycle_tx.send(seeded).expect("seed recycle buffer");

    let reused = take_output_chunk_buffer(&recycle_rx);
    assert_eq!(reused.len(), PTY_OUTPUT_CHUNK_BYTES);
    assert_eq!(reused.as_ptr(), seeded_ptr);
}

#[test]
fn output_chunk_recycle_roundtrip_preserves_allocation() {
    let (recycle_tx, recycle_rx) = sync_channel::<Vec<u8>>(1);
    let mut chunk = vec![0_u8; PTY_OUTPUT_CHUNK_BYTES];
    let ptr = chunk.as_ptr();
    chunk.truncate(777);
    recycle_output_chunk_buffer(&recycle_tx, chunk);

    let roundtrip = take_output_chunk_buffer(&recycle_rx);
    assert_eq!(roundtrip.len(), PTY_OUTPUT_CHUNK_BYTES);
    assert_eq!(roundtrip.as_ptr(), ptr);
}

#[test]
fn output_drain_budget_triggers_on_byte_limit() {
    let budget = OutputDrainBudget {
        pressure: OutputDrainPressure::Normal,
        max_bytes_per_tick: OUTPUT_DRAIN_MAX_BYTES_PER_TICK,
        max_latency: OUTPUT_DRAIN_MAX_LATENCY,
    };
    assert!(!output_drain_budget_exhausted(
        OUTPUT_DRAIN_MAX_BYTES_PER_TICK - 1,
        Duration::ZERO,
        budget,
    ));
    assert!(output_drain_budget_exhausted(
        OUTPUT_DRAIN_MAX_BYTES_PER_TICK,
        Duration::ZERO,
        budget,
    ));
}

#[test]
fn output_drain_budget_triggers_on_elapsed_limit() {
    let budget = OutputDrainBudget {
        pressure: OutputDrainPressure::Normal,
        max_bytes_per_tick: OUTPUT_DRAIN_MAX_BYTES_PER_TICK,
        max_latency: OUTPUT_DRAIN_MAX_LATENCY,
    };
    assert!(!output_drain_budget_exhausted(
        0,
        OUTPUT_DRAIN_MAX_LATENCY.saturating_sub(Duration::from_millis(1)),
        budget,
    ));
    assert!(output_drain_budget_exhausted(
        0,
        OUTPUT_DRAIN_MAX_LATENCY,
        budget
    ));
}

#[test]
fn output_drain_budget_escalates_with_queue_pressure() {
    let normal = output_drain_budget(OutputQueueSnapshot {
        queued_bytes: 0,
        queued_chunks: 0,
    });
    assert_eq!(normal.pressure, OutputDrainPressure::Normal);
    assert_eq!(normal.max_bytes_per_tick, OUTPUT_DRAIN_MAX_BYTES_PER_TICK);

    let elevated = output_drain_budget(OutputQueueSnapshot {
        queued_bytes: 3 * 1024 * 1024,
        queued_chunks: 4,
    });
    assert_eq!(elevated.pressure, OutputDrainPressure::Elevated);
    assert_eq!(
        elevated.max_bytes_per_tick,
        OUTPUT_DRAIN_ELEVATED_MAX_BYTES_PER_TICK
    );

    let critical = output_drain_budget(OutputQueueSnapshot {
        queued_bytes: 10 * 1024 * 1024,
        queued_chunks: 220,
    });
    assert_eq!(critical.pressure, OutputDrainPressure::Critical);
    assert_eq!(
        critical.max_bytes_per_tick,
        OUTPUT_DRAIN_CRITICAL_MAX_BYTES_PER_TICK
    );
}

#[test]
fn child_exit_drain_timeout_boundary_is_deterministic() {
    let started_at = Instant::now();
    assert!(!child_exit_drain_timed_out(
        started_at,
        started_at + CHILD_EXIT_DRAIN_MAX_WAIT.saturating_sub(Duration::from_millis(1))
    ));
    assert!(child_exit_drain_timed_out(
        started_at,
        started_at + CHILD_EXIT_DRAIN_MAX_WAIT
    ));
}

#[test]
fn terminal_feed_chunking_respects_core_per_call_limit() {
    let payload = vec![b'x'; MAX_FEED_BYTES_PER_CALL * 2 + 17];
    let chunk_sizes: Vec<usize> = terminal_feed_chunks(&payload)
        .map(|chunk| chunk.len())
        .collect();
    assert_eq!(
        chunk_sizes,
        vec![MAX_FEED_BYTES_PER_CALL, MAX_FEED_BYTES_PER_CALL, 17]
    );
}

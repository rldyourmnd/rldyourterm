// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use super::*;

impl GuiRuntimeApp {
    fn dispatch_terminal_responses(
        &mut self,
        responses: &TerminalResponseBuffer,
        event_loop: &ActiveEventLoop,
    ) -> bool {
        let mut emitted_terminal_response = false;
        let mut saw_write_error = false;
        let mut should_exit = false;
        responses.for_each_terminal_response(|data| {
            if should_exit {
                return;
            }
            emitted_terminal_response = true;
            trace!(bytes = data.len(), "sending terminal response to PTY");
            match self.write_pty_chunk(data, event_loop, "failed to write terminal response to PTY")
            {
                PtyWriteOutcome::Written => {}
                PtyWriteOutcome::RecoverableFailure => {
                    saw_write_error = true;
                }
                PtyWriteOutcome::ExitLoop => {
                    should_exit = true;
                }
            }
        });
        if should_exit {
            return false;
        }
        !emitted_terminal_response || saw_write_error || self.finish_pty_write(event_loop)
    }

    fn reader_pump_finished(&self) -> bool {
        match self.reader_pump.as_ref() {
            Some(handle) => handle.is_finished(),
            None => true,
        }
    }

    pub(super) fn child_exit_drain_complete(&self) -> bool {
        self.reader_pump_finished()
            && !self.output_event_pending.load(Ordering::Acquire)
            && self.output_batch.is_empty()
    }

    pub(super) fn begin_child_exit_drain(&mut self, event_loop: &ActiveEventLoop) {
        self.child_exit_pending = true;
        self.child_exit_drain_started_at
            .get_or_insert_with(Instant::now);
        self.drain_output_queue(event_loop);
        if self.child_exit_drain_complete() {
            self.child_exit_pending = false;
            self.child_exit_drain_started_at = None;
            event_loop.exit();
        }
    }

    pub(super) fn shutdown(&mut self) {
        debug!(
            exit_code = ?self.exit_code,
            has_fatal_error = self.fatal_error.is_some(),
            "shutdown: beginning teardown"
        );

        self.persist_gpu_pipeline_cache();

        let child_exited = self.exit_code.is_some() || self.pty.try_wait().ok().flatten().is_some();
        if child_exited {
            if let Some(handle) = self.reader_pump.take() {
                join_pump_thread_with_timeout(handle, "reader_pump");
            }
            if let Some(handle) = self.wait_pump.take() {
                join_pump_thread_with_timeout(handle, "wait_pump");
            }
            if let Err(error) = self.pty.close() {
                warn!(error = %error, "failed to close PTY during GUI shutdown");
                if self.fatal_error.is_none() {
                    self.fatal_error = Some(anyhow!("failed to close PTY: {error}"));
                }
            }
            return;
        }

        if let Err(error) = self.pty.close() {
            warn!(error = %error, "failed to close PTY during GUI shutdown");
            if self.fatal_error.is_none() {
                self.fatal_error = Some(anyhow!("failed to close PTY: {error}"));
            }
        }

        if let Some(handle) = self.reader_pump.take() {
            join_pump_thread_with_timeout(handle, "reader_pump");
        }

        if let Some(handle) = self.wait_pump.take() {
            join_pump_thread_with_timeout(handle, "wait_pump");
        }
    }

    pub(super) fn emit_runtime_notice(&mut self, message: &str) {
        let mut line = String::from("\r\n");
        line.push_str(message);
        line.push_str("\r\n");
        self.response_buffer_scratch
            .feed_terminal(&mut self.terminal, line.as_bytes());
        self.queue_redraw();
    }

    pub(super) fn request_redraw_if_needed(&mut self) {
        if !self.frame.redraw_pending || self.frame.redraw_in_flight {
            return;
        }
        if (self.window.window_control.is_some() || self.window.window.is_some())
            && self.request_window_redraw()
        {
            self.frame.redraw_pending = false;
            self.frame.redraw_in_flight = true;
        }
    }

    fn apply_output_bytes(&mut self, data: &[u8], event_loop: &ActiveEventLoop) -> bool {
        trace!(bytes = data.len(), "pty output received");
        self.interaction.viewport_offset = 0;
        let mut response_buffer = std::mem::take(&mut self.response_buffer_scratch);
        for chunk in terminal_feed_chunks(data) {
            response_buffer.feed_terminal(&mut self.terminal, chunk);
            if !self.dispatch_terminal_responses(&response_buffer, event_loop) {
                self.response_buffer_scratch = response_buffer;
                return false;
            }
        }
        self.response_buffer_scratch = response_buffer;
        self.dispatch_pending_clipboard();
        self.dispatch_pending_bell();
        true
    }

    fn dispatch_pending_clipboard(&mut self) {
        if let Some((_selection, base64_data)) = self.terminal.take_pending_clipboard() {
            use base64::Engine;
            match base64::engine::general_purpose::STANDARD.decode(&base64_data) {
                Ok(decoded) => {
                    if let Ok(text) = String::from_utf8(decoded) {
                        if let Err(err) = self.clipboard.set_text(&text) {
                            debug!(%err, "failed to set clipboard via OSC 52");
                        } else {
                            debug!(bytes = text.len(), "clipboard set via OSC 52");
                        }
                    } else {
                        debug!("OSC 52 clipboard payload is not valid UTF-8");
                    }
                }
                Err(err) => {
                    debug!(%err, "OSC 52 clipboard payload is not valid base64");
                }
            }
        }
    }

    fn dispatch_pending_bell(&mut self) {
        if self.terminal.take_pending_bell()
            && let Some(window) = self.window.window.as_ref()
        {
            window.request_user_attention(Some(winit::window::UserAttentionType::Informational));
            trace!("bell: requested window attention");
        }
    }

    fn flush_output_batch(&mut self, batch: &mut Vec<u8>, event_loop: &ActiveEventLoop) -> bool {
        if batch.is_empty() {
            return true;
        }
        if !self.apply_output_bytes(batch.as_slice(), event_loop) {
            return false;
        }
        batch.clear();
        true
    }

    fn append_output_chunk_to_batch(
        &mut self,
        batch: &mut Vec<u8>,
        data: &[u8],
        event_loop: &ActiveEventLoop,
    ) -> bool {
        if data.len() >= OUTPUT_BATCH_MAX_BYTES {
            if !self.flush_output_batch(batch, event_loop) {
                return false;
            }
            return self.apply_output_bytes(data, event_loop);
        }
        if should_flush_output_batch(batch.len(), data.len())
            && !self.flush_output_batch(batch, event_loop)
        {
            return false;
        }
        batch.extend_from_slice(data);
        true
    }

    fn recycle_output_chunk(&self, chunk: OutputChunk) {
        recycle_output_chunk_buffer(&self.output_recycle_tx, chunk.into_buffer());
    }

    pub(super) fn drain_output_queue(&mut self, event_loop: &ActiveEventLoop) {
        let mut drained_any = false;
        let mut batch = std::mem::take(&mut self.output_batch);
        let drain_started = Instant::now();
        let mut drained_bytes = 0usize;
        let mut budget_exhausted = false;
        let mut active_budget = output_drain_budget(self.output_backpressure.snapshot());

        'drain: loop {
            while let Ok(chunk) = self.output_rx.try_recv() {
                let chunk_len = chunk.len();
                self.output_backpressure.note_dequeue(chunk_len);
                drained_any = true;
                drained_bytes = drained_bytes.saturating_add(chunk_len);
                if !self.append_output_chunk_to_batch(&mut batch, chunk.as_bytes(), event_loop) {
                    self.recycle_output_chunk(chunk);
                    self.output_batch = batch;
                    return;
                }
                self.recycle_output_chunk(chunk);
                active_budget = output_drain_budget(self.output_backpressure.snapshot());
                if output_drain_budget_exhausted(
                    drained_bytes,
                    drain_started.elapsed(),
                    active_budget,
                ) {
                    budget_exhausted = true;
                    break 'drain;
                }
            }

            // Release the pending flag only after we observed queue empty.
            self.output_event_pending.store(false, Ordering::Release);

            // Handle producer race: data may arrive between empty check and flag reset.
            match self.output_rx.try_recv() {
                Ok(chunk) => {
                    let chunk_len = chunk.len();
                    self.output_backpressure.note_dequeue(chunk_len);
                    self.output_event_pending.store(true, Ordering::Release);
                    drained_any = true;
                    drained_bytes = drained_bytes.saturating_add(chunk_len);
                    if !self.append_output_chunk_to_batch(&mut batch, chunk.as_bytes(), event_loop)
                    {
                        self.recycle_output_chunk(chunk);
                        self.output_batch = batch;
                        return;
                    }
                    self.recycle_output_chunk(chunk);
                    active_budget = output_drain_budget(self.output_backpressure.snapshot());
                    if output_drain_budget_exhausted(
                        drained_bytes,
                        drain_started.elapsed(),
                        active_budget,
                    ) {
                        budget_exhausted = true;
                        break;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }

        if !self.flush_output_batch(&mut batch, event_loop) {
            self.output_batch = batch;
            return;
        }
        if batch.capacity() > OUTPUT_BATCH_MAX_BYTES * 2 {
            batch.shrink_to(OUTPUT_BATCH_INITIAL_CAPACITY);
        }
        self.output_batch = batch;

        if !drained_any {
            return;
        }

        if budget_exhausted {
            let queue_snapshot = self.output_backpressure.snapshot();
            debug!(
                drained_bytes,
                elapsed_ms = drain_started.elapsed().as_millis(),
                queue_pressure = ?active_budget.pressure,
                queue_bytes = queue_snapshot.queued_bytes,
                queue_chunks = queue_snapshot.queued_chunks,
                drain_byte_budget = active_budget.max_bytes_per_tick,
                drain_latency_budget_ms = active_budget.max_latency.as_millis(),
                "output drain budget exhausted; scheduling continuation"
            );
            let _ = self.event_proxy.send_event(GuiEvent::OutputReady);
        }

        let title = self.terminal.window_title();
        if !title.is_empty() && title != self.window.last_window_title {
            self.set_window_title(title);
            self.window.last_window_title.clear();
            self.window.last_window_title.push_str(title);
        }

        if let Err(error) = self.mark_pty_boundary_recovered(SessionBoundary::PtyRead) {
            self.fatal_error = Some(error);
            event_loop.exit();
            return;
        }
        self.queue_redraw();
    }
}

fn join_pump_thread_with_timeout(handle: JoinHandle<()>, thread_label: &'static str) {
    if matches!(
        shared_join_thread_with_timeout(
            handle,
            SHUTDOWN_JOIN_TIMEOUT,
            SHUTDOWN_JOIN_POLL_INTERVAL,
            thread_label,
        ),
        JoinThreadOutcome::TimedOut
    ) {
        warn!(
            thread_label,
            timeout_ms = SHUTDOWN_JOIN_TIMEOUT.as_millis(),
            "GUI shutdown thread join timed out; detaching thread to avoid shutdown hang"
        );
    }
}

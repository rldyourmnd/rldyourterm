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
        if !self.redraw_pending || self.redraw_in_flight {
            return;
        }
        if (self.window_control.is_some() || self.window.is_some()) && self.request_window_redraw()
        {
            self.redraw_pending = false;
            self.redraw_in_flight = true;
        }
    }

    fn apply_output_bytes(&mut self, data: &[u8], event_loop: &ActiveEventLoop) -> bool {
        trace!(bytes = data.len(), "pty output received");
        let mut response_buffer = std::mem::take(&mut self.response_buffer_scratch);
        for chunk in terminal_feed_chunks(data) {
            response_buffer.feed_terminal(&mut self.terminal, chunk);
            if !self.dispatch_terminal_responses(&response_buffer, event_loop) {
                self.response_buffer_scratch = response_buffer;
                return false;
            }
        }
        self.response_buffer_scratch = response_buffer;
        true
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

    fn recycle_output_chunk(&self, chunk: Vec<u8>) {
        recycle_output_chunk_buffer(&self.output_recycle_tx, chunk);
    }

    pub(super) fn drain_output_queue(&mut self, event_loop: &ActiveEventLoop) {
        let mut drained_any = false;
        let mut batch = std::mem::take(&mut self.output_batch);
        let drain_started = Instant::now();
        let mut drained_bytes = 0usize;
        let mut budget_exhausted = false;
        let mut active_budget = output_drain_budget(self.output_backpressure.snapshot());

        'drain: loop {
            while let Ok(data) = self.output_rx.try_recv() {
                self.output_backpressure.note_dequeue(data.len());
                drained_any = true;
                drained_bytes = drained_bytes.saturating_add(data.len());
                if !self.append_output_chunk_to_batch(&mut batch, &data, event_loop) {
                    self.recycle_output_chunk(data);
                    self.output_batch = batch;
                    return;
                }
                self.recycle_output_chunk(data);
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
                Ok(data) => {
                    self.output_backpressure.note_dequeue(data.len());
                    self.output_event_pending.store(true, Ordering::Release);
                    drained_any = true;
                    drained_bytes = drained_bytes.saturating_add(data.len());
                    if !self.append_output_chunk_to_batch(&mut batch, &data, event_loop) {
                        self.recycle_output_chunk(data);
                        self.output_batch = batch;
                        return;
                    }
                    self.recycle_output_chunk(data);
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
        if !title.is_empty() {
            self.set_window_title(title);
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

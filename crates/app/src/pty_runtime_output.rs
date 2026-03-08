use super::*;

pub(super) fn is_stdout_disconnect_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::BrokenPipe | ErrorKind::NotConnected
    )
}

pub(super) fn spawn_read_pump(
    mut reader: Box<dyn Read + Send>,
) -> (JoinHandle<()>, Receiver<String>) {
    let (failure_tx, failure_rx) = mpsc::channel::<String>();
    let handle = thread::spawn(move || {
        let mut stdout = BufWriter::with_capacity(READ_PUMP_FLUSH_MAX_BYTES * 2, io::stdout());
        let mut buffer = [0_u8; 65536];
        let mut buffered_bytes = 0usize;
        let mut last_flush = Instant::now();

        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read_bytes) => {
                    let chunk = &buffer[..read_bytes];
                    if let Err(error) = stdout.write_all(chunk) {
                        if is_stdout_disconnect_error(&error) {
                            let _ =
                                failure_tx.send(READ_PUMP_SIGNAL_STDOUT_DISCONNECTED.to_owned());
                            break;
                        }
                        let _ = failure_tx.send("failed to write PTY chunk to stdout".to_owned());
                        break;
                    }
                    buffered_bytes = buffered_bytes.saturating_add(read_bytes);

                    let should_flush =
                        should_flush_read_pump(chunk, buffered_bytes, last_flush.elapsed());
                    if should_flush && let Err(error) = stdout.flush() {
                        if is_stdout_disconnect_error(&error) {
                            let _ =
                                failure_tx.send(READ_PUMP_SIGNAL_STDOUT_DISCONNECTED.to_owned());
                            break;
                        }
                        let _ = failure_tx.send("failed to flush PTY chunk to stdout".to_owned());
                        break;
                    }
                    if should_flush {
                        buffered_bytes = 0;
                        last_flush = Instant::now();
                    }
                }
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(error) => {
                    let _ = failure_tx.send(format!("PTY read error: {error}"));
                    break;
                }
            }
        }

        let _ = stdout.flush();
    });
    (handle, failure_rx)
}

pub(super) fn join_thread_with_timeout(
    handle: JoinHandle<()>,
    timeout: Duration,
    poll_interval: Duration,
    thread_label: &'static str,
) -> JoinThreadOutcome {
    shared_join_thread_with_timeout(handle, timeout, poll_interval, thread_label)
}

pub(super) fn should_flush_read_pump(
    chunk: &[u8],
    buffered_bytes: usize,
    elapsed: Duration,
) -> bool {
    chunk.contains(&b'\n')
        || chunk.contains(&b'\r')
        || buffered_bytes >= READ_PUMP_FLUSH_MAX_BYTES
        || elapsed >= READ_PUMP_FLUSH_INTERVAL
}

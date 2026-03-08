use rldyourterm_services::terminal::{MAX_FEED_BYTES_PER_CALL, TerminalState};

pub(crate) const fn terminal_feed_max_bytes_per_call() -> usize {
    MAX_FEED_BYTES_PER_CALL
}

pub(crate) fn terminal_feed_chunks(data: &[u8]) -> impl Iterator<Item = &[u8]> {
    data.chunks(terminal_feed_max_bytes_per_call())
}

#[derive(Debug, Default)]
pub(crate) struct TerminalResponseBuffer {
    responses: Vec<Vec<u8>>,
}

impl TerminalResponseBuffer {
    #[must_use]
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            responses: Vec::with_capacity(capacity),
        }
    }

    pub(crate) fn feed_terminal(&mut self, terminal: &mut TerminalState, bytes: &[u8]) {
        terminal.feed_terminal_responses_into(bytes, &mut self.responses);
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn is_empty(&self) -> bool {
        self.responses.is_empty()
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn capacity(&self) -> usize {
        self.responses.capacity()
    }

    #[cfg(test)]
    pub(crate) fn clear(&mut self) {
        self.responses.clear();
    }

    pub(crate) fn for_each_terminal_response(&self, mut visitor: impl FnMut(&[u8])) {
        for response in &self.responses {
            visitor(response.as_slice());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{TerminalResponseBuffer, terminal_feed_chunks, terminal_feed_max_bytes_per_call};
    use rldyourterm_services::terminal::{DEFAULT_SCROLLBACK_CAP, TerminalState};

    #[test]
    fn response_buffer_reuses_capacity_and_exposes_terminal_responses() {
        let mut terminal = TerminalState::new(80, 24, DEFAULT_SCROLLBACK_CAP);
        let mut responses = TerminalResponseBuffer::with_capacity(4);

        let initial_capacity = responses.capacity();
        responses.feed_terminal(&mut terminal, b"\x1b[5n");
        let mut exposed = Vec::new();
        responses.for_each_terminal_response(|data| exposed.push(data.to_vec()));

        assert_eq!(exposed, vec![b"\x1b[0n".to_vec()]);
        assert!(!responses.is_empty());
        assert!(responses.capacity() >= 4 || initial_capacity == 0);

        let capacity_after_first_feed = responses.capacity();
        responses.feed_terminal(&mut terminal, b"plain text");
        assert_eq!(responses.capacity(), capacity_after_first_feed);
    }

    #[test]
    fn response_buffer_clear_drops_exposed_events_only() {
        let mut terminal = TerminalState::new(80, 24, DEFAULT_SCROLLBACK_CAP);
        let mut responses = TerminalResponseBuffer::with_capacity(2);

        responses.feed_terminal(&mut terminal, b"\x1b[6n");
        let mut exposed_count = 0usize;
        responses.for_each_terminal_response(|_| exposed_count += 1);
        assert_eq!(exposed_count, 1);

        responses.clear();
        assert!(responses.is_empty());
        exposed_count = 0;
        responses.for_each_terminal_response(|_| exposed_count += 1);
        assert_eq!(exposed_count, 0);
    }

    #[test]
    fn terminal_feed_chunking_uses_core_chunk_boundary() {
        let payload = vec![b'x'; terminal_feed_max_bytes_per_call() * 2 + 7];
        let chunk_sizes: Vec<usize> = terminal_feed_chunks(&payload)
            .map(|chunk| chunk.len())
            .collect();

        assert_eq!(
            chunk_sizes,
            vec![
                terminal_feed_max_bytes_per_call(),
                terminal_feed_max_bytes_per_call(),
                7,
            ]
        );
    }
}

use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scrollback {
    cap: usize,
    lines: VecDeque<String>,
}

impl Scrollback {
    pub fn new(cap: usize) -> Self {
        Self {
            cap,
            lines: VecDeque::new(),
        }
    }

    pub const fn cap(&self) -> usize {
        self.cap
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn clear(&mut self) {
        self.lines.clear();
    }

    pub fn push(&mut self, line: String) -> usize {
        if self.cap == 0 {
            return 1;
        }

        self.lines.push_back(line);
        let mut dropped = 0usize;
        while self.lines.len() > self.cap {
            let _ = self.lines.pop_front();
            dropped += 1;
        }
        dropped
    }

    pub fn get(&self, index: usize) -> Option<&str> {
        self.lines.get(index).map(String::as_str)
    }

    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.lines.iter().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::Scrollback;

    #[test]
    fn push_trims_oldest_lines_when_cap_exceeded() {
        let mut scrollback = Scrollback::new(2);

        assert_eq!(scrollback.push("l1".to_string()), 0);
        assert_eq!(scrollback.push("l2".to_string()), 0);
        assert_eq!(scrollback.push("l3".to_string()), 1);

        assert_eq!(scrollback.len(), 2);
        assert_eq!(scrollback.get(0), Some("l2"));
        assert_eq!(scrollback.get(1), Some("l3"));
    }

    #[test]
    fn cap_zero_discards_every_line() {
        let mut scrollback = Scrollback::new(0);

        assert_eq!(scrollback.push("l1".to_string()), 1);
        assert_eq!(scrollback.push("l2".to_string()), 1);
        assert!(scrollback.is_empty());
    }
}

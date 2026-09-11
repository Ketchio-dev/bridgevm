pub(super) const MAX_COMMAND_BYTES: usize = 256;

#[derive(Default)]
pub(super) struct InputLines {
    partial: Vec<u8>,
    discarding: bool,
}

impl InputLines {
    pub(super) fn clear(&mut self) {
        self.partial.clear();
        self.discarding = false;
    }

    pub(super) fn is_empty(&self) -> bool {
        self.partial.is_empty() && !self.discarding
    }

    pub(super) fn consume(&mut self, bytes: &[u8]) -> Vec<String> {
        let mut lines = Vec::new();
        for &byte in bytes {
            if byte == b'\n' {
                if !self.discarding {
                    let line = String::from_utf8_lossy(&self.partial).trim().to_string();
                    if !line.is_empty() {
                        lines.push(line);
                    }
                }
                self.clear();
            } else if !self.discarding {
                if self.partial.len() == MAX_COMMAND_BYTES {
                    self.partial.clear();
                    self.discarding = true;
                    eprintln!("live input rejected: command_too_long");
                } else {
                    self.partial.push(byte);
                }
            }
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_line_suffix_never_becomes_a_new_command() {
        let mut input = InputLines::default();
        assert!(input.consume(&vec![b'x'; MAX_COMMAND_BYTES + 1]).is_empty());
        assert!(!input.is_empty());
        assert!(input.partial.len() <= MAX_COMMAND_BYTES);
        assert!(input.consume(b"KEY enter").is_empty());
        assert_eq!(input.consume(b"\nKEY tab\n"), vec!["KEY tab"]);
        assert!(input.is_empty());
    }

    #[test]
    fn accepts_exact_limit_and_rejects_one_extra_byte_across_reads() {
        let mut input = InputLines::default();
        let exact = vec![b'x'; MAX_COMMAND_BYTES];
        assert!(input.consume(&exact).is_empty());
        assert_eq!(input.consume(b"\n"), vec!["x".repeat(MAX_COMMAND_BYTES)]);
        assert!(input.consume(&exact).is_empty());
        assert!(input.consume(b"x\n").is_empty());
        assert!(input.is_empty());
    }

    #[test]
    fn complete_line_decoding_preserves_split_utf8_and_crlf() {
        let mut input = InputLines::default();
        assert!(input.consume(b"SNAPSHOT \xe9").is_empty());
        assert!(input.consume(b"\x9f").is_empty());
        assert_eq!(input.consume(b"\xa9\r\nKEY enter\n"), vec!["SNAPSHOT \u{97e9}", "KEY enter"]);
    }

    #[test]
    fn truncation_reset_clears_discard_state_and_partial_bytes() {
        let mut input = InputLines::default();
        input.consume(&vec![b'x'; MAX_COMMAND_BYTES + 1]);
        input.clear();
        assert!(input.is_empty());
        assert_eq!(input.consume(b"KEY enter\n"), vec!["KEY enter"]);
        input.consume(b"KEY le");
        input.clear();
        assert_eq!(input.consume(b"KEY right\n"), vec!["KEY right"]);
    }
}

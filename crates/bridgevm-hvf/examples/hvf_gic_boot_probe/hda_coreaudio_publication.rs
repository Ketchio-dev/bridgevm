//! Complete-frame publication, shared by the atomic sink and framing contracts.

pub(super) trait PcmQueue {
    fn len(&self) -> usize;
    fn publish_parts(&mut self, first: &[u8], second: &[u8], capacity: usize) -> bool;
}

#[cfg(test)]
impl PcmQueue for std::collections::VecDeque<u8> {
    fn len(&self) -> usize {
        self.len()
    }
    fn publish_parts(&mut self, first: &[u8], second: &[u8], capacity: usize) -> bool {
        if first.len() + second.len() > capacity.saturating_sub(self.len()) {
            return false;
        }
        self.extend(first.iter().copied());
        self.extend(second.iter().copied());
        true
    }
}

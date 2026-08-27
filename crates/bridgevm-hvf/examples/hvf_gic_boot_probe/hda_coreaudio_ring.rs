//! Bulk draining for the CoreAudio callback's byte ring.

use std::collections::VecDeque;

pub(super) fn drain_ring_into(ring: &mut VecDeque<u8>, destination: &mut [u8]) -> usize {
    let count = ring.len().min(destination.len());
    if count == 0 {
        return 0;
    }

    let (first, second) = ring.as_slices();
    let first_len = first.len().min(count);
    destination[..first_len].copy_from_slice(&first[..first_len]);
    let second_len = count - first_len;
    if second_len != 0 {
        destination[first_len..count].copy_from_slice(&second[..second_len]);
    }
    drop(ring.drain(..count));
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drains_contiguous_bytes_and_leaves_destination_tail_untouched() {
        let mut ring = VecDeque::from([1, 2, 3]);
        let mut destination = [0xee; 5];

        assert_eq!(drain_ring_into(&mut ring, &mut destination), 3);
        assert_eq!(destination, [1, 2, 3, 0xee, 0xee]);
        assert!(ring.is_empty());
    }

    #[test]
    fn partial_destination_leaves_remaining_ring_bytes_in_order() {
        let mut ring = VecDeque::from([1, 2, 3, 4]);
        let mut destination = [0; 2];

        assert_eq!(drain_ring_into(&mut ring, &mut destination), 2);
        assert_eq!(destination, [1, 2]);
        assert_eq!(ring, VecDeque::from([3, 4]));
    }

    #[test]
    fn drains_wrapped_ring_across_both_internal_slices() {
        let mut ring = VecDeque::with_capacity(4);
        ring.extend([1, 2, 3, 4]);
        assert_eq!(ring.pop_front(), Some(1));
        assert_eq!(ring.pop_front(), Some(2));
        assert_eq!(ring.pop_front(), Some(3));
        ring.extend([5, 6, 7]);
        assert!(!ring.as_slices().1.is_empty());

        let mut destination = [0; 4];
        assert_eq!(drain_ring_into(&mut ring, &mut destination), 4);
        assert_eq!(destination, [4, 5, 6, 7]);
        assert!(ring.is_empty());
    }

    #[test]
    fn empty_ring_does_not_touch_destination() {
        let mut ring = VecDeque::new();
        let mut destination = [0xaa; 3];

        assert_eq!(drain_ring_into(&mut ring, &mut destination), 0);
        assert_eq!(destination, [0xaa; 3]);
    }
}

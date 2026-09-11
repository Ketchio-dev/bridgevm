use super::super::{MAX_COMMAND_BYTES, MAX_PENDING_COMMANDS, MAX_READ_BYTES_PER_TICK};

pub(super) fn byte_budget(queued: usize, unread: u64) -> u64 {
    let slots = MAX_PENDING_COMMANDS.saturating_sub(queued);
    let limit = if queued == 0 {
        MAX_READ_BYTES_PER_TICK
    } else {
        (slots * (MAX_COMMAND_BYTES + 1)) as u64
    };
    unread.min(limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_queue_retains_existing_bulk_read_limit() {
        assert_eq!(byte_budget(0, u64::MAX), MAX_READ_BYTES_PER_TICK);
    }

    #[test]
    fn one_free_slot_reads_at_most_one_maximum_frame() {
        assert_eq!(byte_budget(63, u64::MAX), 257);
        assert_eq!(byte_budget(1, u64::MAX), 63 * 257);
    }

    #[test]
    fn full_or_overfull_queue_never_reads_ahead() {
        assert_eq!(byte_budget(64, u64::MAX), 0);
        assert_eq!(byte_budget(usize::MAX, u64::MAX), 0);
    }

    #[test]
    fn short_file_and_empty_file_do_not_overread() {
        assert_eq!(byte_budget(63, 7), 7);
        assert_eq!(byte_budget(0, 0), 0);
    }
}

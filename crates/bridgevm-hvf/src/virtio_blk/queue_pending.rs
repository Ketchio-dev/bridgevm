//! Caps each guest notification's avail-ring work at the active queue size.

pub(super) fn pending_entries(last_avail_idx: u16, avail_idx: u16, queue_num: u16) -> u16 {
    avail_idx.wrapping_sub(last_avail_idx).min(queue_num)
}

#[cfg(test)]
mod tests {
    use super::super::tests::{temp_path, TestMem};
    use super::super::{read_u16, VirtioMmioBlock, SECTOR_SIZE};
    use std::fs;

    #[test]
    fn process_queue_caps_malformed_avail_delta_at_queue_size() {
        let path = temp_path("avail-bound");
        fs::write(&path, vec![0u8; SECTOR_SIZE as usize]).unwrap();
        let mut dev = VirtioMmioBlock::open_read_only_modern(&path).unwrap();
        let mut mem = TestMem::new(0x4000_0000, 0x10000);
        let desc = 0x4000_1000;
        let avail = 0x4000_2000;
        let used = 0x4000_3000;
        let queue_num = 8u16;

        dev.queue_num = queue_num;
        dev.queue_desc = desc;
        dev.queue_driver = avail;
        dev.queue_device = used;
        dev.queue_ready = true;
        mem.write(avail + 2, &queue_num.wrapping_add(1).to_le_bytes());

        dev.process_queue(&mut mem);

        assert_eq!(dev.last_avail_idx, queue_num);
        assert_eq!(read_u16(&mem, used + 2), Some(queue_num));
        fs::remove_file(path).ok();
    }
}
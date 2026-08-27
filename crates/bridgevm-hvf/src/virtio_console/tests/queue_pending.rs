use super::super::*;
use super::helpers::*;

#[test]
fn console_tx_queues_cap_malformed_avail_delta_at_queue_size() {
    for queue_index in [QUEUE_CONTROL_TX, QUEUE_AGENT_TX] {
        let mut dev = VirtioPciConsole::new();
        let mut mem = TestMem::new(0x4000_0000, 0x10000);
        let desc = 0x4000_1000;
        let avail = 0x4000_2000;
        let used = 0x4000_3000;
        let queue_size = 8u16;

        setup_queue(
            &mut dev,
            &mut mem,
            queue_index as u16,
            desc,
            avail,
            used,
            queue_index as u16,
        );
        mem.write(avail + 2, &queue_size.wrapping_add(1).to_le_bytes());

        pci_write(
            &mut dev,
            PCI_NOTIFY_CFG_OFFSET + u64::from(queue_index as u16) * 4,
            4,
            0,
            &mut mem,
        );

        let stats = dev.stats();
        assert_eq!(stats.queues[queue_index].last_avail_idx, queue_size);
        assert_eq!(
            stats.queues[queue_index].used_produced,
            u64::from(queue_size)
        );
        assert_eq!(stats.queues[queue_index].notify_count, 1);
        assert_eq!(
            u16::from_le_bytes(mem.read(used + 2, 2).try_into().unwrap()),
            queue_size
        );
    }
}

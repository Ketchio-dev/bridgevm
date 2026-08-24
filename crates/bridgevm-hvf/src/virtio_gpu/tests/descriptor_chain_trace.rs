// Descriptor-chain rejection classification and trace evidence.

use super::super::descriptor_chain_trace::DescriptorChainError;
use super::super::*;
use super::helpers::*;

#[test]
fn descriptor_walk_reports_the_index_and_out_of_range_next() {
    let mut mem = TestMem::new(0x4000_0000, 0x10000);
    let mut queue = VirtioGpuQueue::new(0);
    queue.size = 16;
    queue.desc = 0x4000_1000;
    write_desc(&mut mem, queue.desc, 0, 0x4000_8000, 32, DESC_F_NEXT, 16);
    let mut partial = Vec::new();
    assert_eq!(
        VirtioGpu::descriptor_chain_into(&mem, &queue, 0, &mut partial),
        Err(DescriptorChainError::NextOutOfRange { index: 0, next: 16 })
    );
    assert_eq!(partial.len(), 1);
}

#[test]
fn queue_consumes_and_records_a_rejected_descriptor_chain() {
    let path = trace_test_path("descriptor-chain-rejected");
    let mut dev = VirtioPciGpu::new(1280, 800);
    dev.gpu.trace = crate::virtio_gpu_trace::VirtioGpuTraceRecorder::test_file(&path);
    let mut mem = TestMem::new(0x4000_0000, 0x10000);
    let (desc, avail, used) = (0x4000_1000, 0x4000_2000, 0x4000_3000);
    setup_queue(&mut dev, &mut mem, 0, desc, avail, used, 0);
    write_desc(&mut mem, desc, 0, 0x4000_8000, 32, DESC_F_NEXT, 16);
    mem.write(avail + 2, &1u16.to_le_bytes());
    mem.write(avail + 4, &0u16.to_le_bytes());
    pci_write(&mut dev, PCI_NOTIFY_CFG_OFFSET, 4, 0, &mut mem);

    assert_eq!(dev.stats().queues[0].last_avail_idx, 1);
    assert_eq!(
        u16::from_le_bytes(mem.read(used + 2, 2).try_into().unwrap()),
        1
    );
    assert_eq!(
        u32::from_le_bytes(mem.read(used + 8, 4).try_into().unwrap()),
        0
    );
    drop(dev);
    let contents = std::fs::read_to_string(&path).unwrap();
    let _ = std::fs::remove_file(path);
    assert!(contents.contains(r#""event":"descriptor_chain_rejected""#));
    assert!(contents.contains(r#""reason":"next_out_of_range""#));
    assert!(contents.contains(r#""head":0,"queue_size":16,"last_avail_idx":0"#));
    assert!(contents.contains(r#""partial_descriptor_count":1,"reason""#));
    assert!(contents.contains(r#""index":0,"next":16"#));
}

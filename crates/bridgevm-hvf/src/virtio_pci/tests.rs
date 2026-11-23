//! Per-device differences on the shared transport that the device suites do not pin down.

use super::*;
use crate::virtio_console::VirtioPciConsole;
use crate::virtio_gpu::VirtioPciGpu;
use crate::virtio_net::VirtioPciNet;

struct NoMem;

impl GuestMemoryMut for NoMem {
    fn write_bytes(&mut self, _: u64, _: &[u8]) -> bool {
        false
    }

    fn read_bytes(&self, _: u64, _: usize) -> Option<Vec<u8>> {
        None
    }
}

type Access<'a> = &'a mut dyn FnMut(u64, VirtioPciOp) -> VirtioPciResult;

fn write(access: Access<'_>, offset: u64, size: u8, value: u64) {
    assert_eq!(
        access(offset, VirtioPciOp::Write { size, value }),
        VirtioPciResult::WriteAck
    );
}

fn read(access: Access<'_>, offset: u64, size: u8) -> u64 {
    match access(offset, VirtioPciOp::Read { size }) {
        VirtioPciResult::ReadValue(value) => value,
        other => panic!("read returned {other:?}"),
    }
}

/// Config and queue-0 MSI-X vectors after the driver programs 0x7ff into both.
fn programmed_vectors(access: Access<'_>) -> [u64; 2] {
    for offset in [COMMON_CONFIG_MSIX_VECTOR, COMMON_QUEUE_MSIX_VECTOR] {
        write(access, offset, 2, 0x7ff);
    }
    [COMMON_CONFIG_MSIX_VECTOR, COMMON_QUEUE_MSIX_VECTOR].map(|offset| read(access, offset, 2))
}

#[test]
fn only_gpu_turns_vectors_past_its_msix_table_into_no_vector() {
    let mut console = VirtioPciConsole::new();
    let mut net = VirtioPciNet::new_loopback();
    let mut gpu = VirtioPciGpu::new(64, 48);
    assert_eq!(
        programmed_vectors(&mut |o, op| console.access(o, op, &mut NoMem)),
        [0x7ff; 2]
    );
    assert_eq!(
        programmed_vectors(&mut |o, op| net.access(o, op, &mut NoMem)),
        [0x7ff; 2]
    );
    assert_eq!(
        programmed_vectors(&mut |o, op| gpu.access(o, op, &mut NoMem)),
        [u64::from(VIRTIO_MSI_NO_VECTOR); 2]
    );
}

#[test]
fn console_emerg_wr_takes_partial_writes_and_net_config_is_read_only() {
    let mut console = VirtioPciConsole::new();
    let config = &mut |o, op| console.access(PCI_DEVICE_CFG_OFFSET + o, op, &mut NoMem);
    write(config, 8, 4, 0x1122_3344);
    write(config, 9, 1, 0xaa);
    assert_eq!(read(config, 8, 4), 0x1122_aa44);

    let mut net = VirtioPciNet::new_loopback();
    let config = &mut |o, op| net.access(PCI_DEVICE_CFG_OFFSET + o, op, &mut NoMem);
    let mac = read(config, 0, 4);
    write(config, 0, 4, !mac);
    assert_eq!(read(config, 0, 4), mac);
}

//! Read-to-clear PCI ISR contract (Virtio 1.2, sections 4.1.4.5 and 4.1.4.5.1).
#[path = "virtio_pci_isr/platform.rs"]
mod platform;
use bridgevm_hvf::{
    fwcfg::GuestMemoryMut,
    platform_virt::FlatGuestRam,
    virtio_console::{VirtioConsoleResult, VirtioPciConsole, VirtioPciConsoleOp},
    virtio_gpu::{VirtioGpuResult, VirtioPciGpu, VirtioPciGpuOp},
    virtio_net::{VirtioNetResult, VirtioPciNet, VirtioPciNetOp},
};

const ISR: u64 = 0x1000;
const DESC: u64 = 0x1000;
const AVAIL: u64 = 0x2000;
const USED: u64 = 0x3000;
const DATA: u64 = 0x4000;

fn rx_memory() -> FlatGuestRam {
    let mut mem = FlatGuestRam::new(0, 0x5000);
    assert!(mem.write_bytes(DESC, &DATA.to_le_bytes()));
    assert!(mem.write_bytes(DESC + 8, &128u32.to_le_bytes()));
    assert!(mem.write_bytes(DESC + 12, &2u16.to_le_bytes())); // device-writable
    assert!(mem.write_bytes(AVAIL + 2, &1u16.to_le_bytes()));
    mem
}

fn configure_rx(mut write: impl FnMut(u64, u8, u64), queue: u64, features: u64) {
    for (offset, size, value) in [
        (0x14, 1, 0),
        (0x14, 1, 3), // reset, ACKNOWLEDGE | DRIVER
        (0x08, 4, 0),
        (0x0c, 4, features),
        (0x08, 4, 1),
        (0x0c, 4, 1),  // negotiate VIRTIO_F_VERSION_1
        (0x14, 1, 11), // FEATURES_OK
        (0x16, 2, queue),
        (0x18, 2, 8),
        (0x20, 8, DESC),
        (0x28, 8, AVAIL),
        (0x30, 8, USED),
        (0x1c, 2, 1),
        (0x14, 1, 15), // queue enable, DRIVER_OK
    ] {
        write(offset, size, value);
    }
}

#[test]
fn net_rx_isr_read_acknowledges_completed_buffer() {
    for size in [1, 2, 4, 8] {
        let mut dev = VirtioPciNet::new_loopback();
        let mut mem = rx_memory();
        configure_rx(
            |offset, size, value| {
                assert_eq!(
                    dev.access(offset, VirtioPciNetOp::Write { size, value }, &mut mem),
                    VirtioNetResult::WriteAck
                );
            },
            0,
            0,
        );
        dev.backend_mut().push_receive(b"ethernet-frame".to_vec());
        assert!(dev.pump_receive(&mut mem));
        assert_eq!(mem.read_bytes(USED + 2, 2).unwrap(), [1, 0]);
        assert!(dev.interrupt_line_level());
        assert_eq!(
            dev.access(ISR + 1, VirtioPciNetOp::Read { size: 1 }, &mut mem),
            VirtioNetResult::ReadValue(0)
        );
        assert!(dev.interrupt_line_level());
        assert_eq!(
            dev.access(ISR, VirtioPciNetOp::Read { size }, &mut mem),
            VirtioNetResult::ReadValue(1)
        );
        assert!(!dev.interrupt_line_level());
        assert_eq!(
            dev.access(ISR, VirtioPciNetOp::Read { size }, &mut mem),
            VirtioNetResult::ReadValue(0)
        );
        assert_eq!(mem.read_bytes(USED + 2, 2).unwrap(), [1, 0]);
    }
}

#[test]
fn console_rx_isr_read_acknowledges_completed_buffer() {
    for size in [1, 2, 4, 8] {
        let mut dev = VirtioPciConsole::new();
        let mut mem = rx_memory();
        configure_rx(
            |offset, size, value| {
                assert_eq!(
                    dev.access(offset, VirtioPciConsoleOp::Write { size, value }, &mut mem),
                    VirtioConsoleResult::WriteAck
                );
            },
            4,
            2, // VIRTIO_CONSOLE_F_MULTIPORT for agent port 1
        );
        dev.agent_send(b"PING");
        assert!(dev.poll(&mut mem));
        assert_eq!(mem.read_bytes(DATA, 4).unwrap(), b"PING");
        assert_eq!(mem.read_bytes(USED + 2, 2).unwrap(), [1, 0]);
        assert!(dev.interrupt_line_level());
        assert_eq!(
            dev.access(ISR + 1, VirtioPciConsoleOp::Read { size: 1 }, &mut mem),
            VirtioConsoleResult::ReadValue(0)
        );
        assert!(dev.interrupt_line_level());
        assert_eq!(
            dev.access(ISR, VirtioPciConsoleOp::Read { size }, &mut mem),
            VirtioConsoleResult::ReadValue(1)
        );
        assert!(!dev.interrupt_line_level());
        assert_eq!(
            dev.access(ISR, VirtioPciConsoleOp::Read { size }, &mut mem),
            VirtioConsoleResult::ReadValue(0)
        );
    }
}

#[test]
fn gpu_resize_isr_read_acknowledges_then_rearms() {
    for size in [1, 2, 4, 8] {
        let mut dev = VirtioPciGpu::new(640, 480);
        let mut mem = FlatGuestRam::new(0, 0x1000);
        for width in [800, 1024] {
            assert!(dev.request_display_resolution(width, 600));
            assert!(dev.interrupt_line_level());
            assert_eq!(
                dev.access(ISR + 1, VirtioPciGpuOp::Read { size: 1 }, &mut mem),
                VirtioGpuResult::ReadValue(0)
            );
            assert!(dev.interrupt_line_level());
            assert_eq!(
                dev.access(ISR, VirtioPciGpuOp::Read { size }, &mut mem),
                VirtioGpuResult::ReadValue(2)
            );
            assert!(!dev.interrupt_line_level());
            assert_eq!(
                dev.access(ISR, VirtioPciGpuOp::Read { size }, &mut mem),
                VirtioGpuResult::ReadValue(0)
            );
            assert_eq!(dev.display_resolution(), (width, 600));
        }
    }
}

#[test]
fn gpu_reset_clears_unread_isr() {
    let mut dev = VirtioPciGpu::new(640, 480);
    let mut mem = FlatGuestRam::new(0, 0x1000);
    assert!(dev.request_display_resolution(800, 600));
    assert!(dev.interrupt_line_level());
    assert_eq!(
        dev.access(0x14, VirtioPciGpuOp::Write { size: 1, value: 0 }, &mut mem),
        VirtioGpuResult::WriteAck
    );
    assert!(!dev.interrupt_line_level());
    assert_eq!(
        dev.access(ISR, VirtioPciGpuOp::Read { size: 1 }, &mut mem),
        VirtioGpuResult::ReadValue(0)
    );
}

#[test]
fn net_isr_read_preserves_masked_msix_delivery() {
    use bridgevm_hvf::pcie::{VIRTIO_NET_MSIX_PBA_OFFSET, VIRTIO_NET_MSIX_TABLE_OFFSET};
    let mut dev = VirtioPciNet::new_loopback();
    let mut mem = rx_memory();
    configure_rx(
        |offset, size, value| {
            dev.access(offset, VirtioPciNetOp::Write { size, value }, &mut mem);
        },
        0,
        0,
    );
    dev.access(0x1a, VirtioPciNetOp::Write { size: 2, value: 0 }, &mut mem);
    let table = u64::from(VIRTIO_NET_MSIX_TABLE_OFFSET);
    let pba = u64::from(VIRTIO_NET_MSIX_PBA_OFFSET);
    dev.msix_bar_access(
        table,
        VirtioPciNetOp::Write {
            size: 8,
            value: 0xfee0_0000,
        },
    );
    dev.msix_bar_access(
        table + 8,
        VirtioPciNetOp::Write {
            size: 4,
            value: 0x51,
        },
    );
    dev.backend_mut().push_receive(b"ethernet-frame".to_vec());
    assert!(dev.pump_receive(&mut mem));
    assert!(dev.drain_pending_msix(true, false).is_empty()); // vector still masked
    assert_eq!(
        dev.msix_bar_access(pba, VirtioPciNetOp::Read { size: 8 }),
        VirtioNetResult::ReadValue(1)
    );
    assert_eq!(
        dev.access(ISR, VirtioPciNetOp::Read { size: 1 }, &mut mem),
        VirtioNetResult::ReadValue(1)
    );
    assert_eq!(
        dev.msix_bar_access(pba, VirtioPciNetOp::Read { size: 8 }),
        VirtioNetResult::ReadValue(1)
    );
    dev.msix_bar_access(table + 12, VirtioPciNetOp::Write { size: 4, value: 0 });
    assert!(dev.drain_pending_msix(true, true).is_empty()); // function still masked
    let messages = dev.drain_pending_msix(true, false);
    assert_eq!(messages.len(), 1);
    assert_eq!(
        (messages[0].vector, messages[0].address, messages[0].data),
        (0, 0xfee0_0000, 0x51)
    );
    assert!(dev.drain_pending_msix(true, false).is_empty());
    assert_eq!(
        dev.msix_bar_access(pba, VirtioPciNetOp::Read { size: 8 }),
        VirtioNetResult::ReadValue(0)
    );
    assert!(!dev.interrupt_line_level());
}

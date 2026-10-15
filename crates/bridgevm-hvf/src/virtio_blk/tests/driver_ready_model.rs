//! The exposed BAR MMIO-register aliases, not full modern PCI initialization.
use super::*;
use std::io::Write;

struct MediaFile(PathBuf);
impl Drop for MediaFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn write(dev: &mut VirtioPciBlock, mem: &mut TestMem, offset: u64, value: u64) {
    assert_eq!(
        dev.access(offset, VirtioPciBlockOp::Write { size: 4, value }, mem),
        VirtioMmioBlockResult::WriteAck
    );
}

fn fixture(label: &str, legacy: bool) -> (VirtioPciBlock, TestMem, MediaFile, u64) {
    let media = MediaFile(temp_path(label));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&media.0)
        .unwrap();
    file.write_all(&[0x5a; 512]).unwrap();
    let mut dev = VirtioPciBlock::open_read_only(&media.0).unwrap();
    let mut mem = TestMem::new(0x4000_0000, 0x10000);
    let desc = 0x4000_1000;
    let (avail, used) = if legacy {
        (desc + u64::from(QUEUE_MAX) * 16, 0x4000_2000)
    } else {
        (0x4000_2000, 0x4000_3000)
    };
    if legacy {
        for (offset, size, value) in [
            (0x12, 1, 0),
            (0x12, 1, 1),
            (0x12, 1, 3),
            (0x04, 4, u64::from(VIRTIO_BLK_F_RO | VIRTIO_BLK_F_BLK_SIZE)),
            (0x0e, 2, 0),
            (0x08, 4, desc / 4096),
        ] {
            dev.legacy_io_access(offset, VirtioPciBlockOp::Write { size, value }, &mut mem);
        }
    } else {
        for value in [0, 1, 3] {
            write(&mut dev, &mut mem, REG_STATUS, value);
        }
        for (selector, accepted) in [
            (0, u64::from(VIRTIO_BLK_F_RO | VIRTIO_BLK_F_BLK_SIZE)),
            (1, 1),
        ] {
            write(&mut dev, &mut mem, REG_DEVICE_FEATURES_SEL, selector);
            let offered = dev.access(
                REG_DEVICE_FEATURES,
                VirtioPciBlockOp::Read { size: 4 },
                &mut mem,
            );
            assert_eq!(offered, VirtioMmioBlockResult::ReadValue(accepted));
            write(&mut dev, &mut mem, REG_DRIVER_FEATURES_SEL, selector);
            write(&mut dev, &mut mem, REG_DRIVER_FEATURES, accepted);
        }
        write(&mut dev, &mut mem, REG_STATUS, 0x0b);
        write(&mut dev, &mut mem, REG_QUEUE_SEL, 0);
        write(&mut dev, &mut mem, REG_QUEUE_NUM, 8);
        for (register, address) in [
            (REG_QUEUE_DESC_LOW, desc),
            (REG_QUEUE_DRIVER_LOW, avail),
            (REG_QUEUE_DEVICE_LOW, used),
        ] {
            write(&mut dev, &mut mem, register, address & 0xffff_ffff);
            write(&mut dev, &mut mem, register + 4, address >> 32);
        }
        write(&mut dev, &mut mem, REG_QUEUE_READY, 1);
    }
    mem.write(0x4000_4000, &VIRTIO_BLK_T_IN.to_le_bytes());
    for (index, addr, len, flags, next) in [
        (0, 0x4000_4000, 16, DESC_F_NEXT, 1),
        (1, 0x4000_5000, 512, DESC_F_NEXT | DESC_F_WRITE, 2),
        (2, 0x4000_6000, 1, DESC_F_WRITE, 0),
    ] {
        write_desc(
            &mut mem,
            desc,
            index,
            Descriptor {
                addr,
                len,
                flags,
                next,
            },
        );
    }
    mem.write(0x4000_5000, &[0xa5; 512]);
    mem.write(0x4000_6000, &[0xff]);
    mem.write(avail + 2, &1u16.to_le_bytes());
    (dev, mem, media, used)
}

fn notify(dev: &mut VirtioPciBlock, mem: &mut TestMem) {
    dev.access(
        PCI_NOTIFY_CFG_OFFSET,
        VirtioPciBlockOp::Write { size: 2, value: 0 },
        mem,
    );
}

fn complete(dev: &VirtioPciBlock, mem: &TestMem, used: u64) {
    assert_eq!(mem.read(0x4000_5000, 512), [0x5a; 512]);
    assert_eq!(mem.read(0x4000_6000, 1), [VIRTIO_BLK_S_OK]);
    assert_eq!(mem.read(used + 2, 2), 1u16.to_le_bytes());
    assert_eq!(dev.stats().request_count, 1);
    assert!(dev.interrupt_line_level());
}

#[test]
fn modern_alias_queue_waits_for_driver_ok() {
    let (mut dev, mut mem, _media, used) = fixture("modern-premature", false);
    let before = mem.bytes.clone();
    // Defensive invalid-driver notification, through the model's BAR aliases.
    notify(&mut dev, &mut mem);
    assert!(
        mem.bytes == before,
        "premature notification changed guest memory"
    );
    assert_eq!(dev.stats().request_count, 0);
    assert!(!dev.interrupt_line_level());
    write(&mut dev, &mut mem, REG_STATUS, 0x0f);
    notify(&mut dev, &mut mem);
    complete(&dev, &mem, used);
}

#[test]
fn modern_alias_ready_queue_completes_once() {
    let (mut dev, mut mem, _media, used) = fixture("modern-ready", false);
    write(&mut dev, &mut mem, REG_STATUS, 0x0f);
    notify(&mut dev, &mut mem);
    complete(&dev, &mem, used);
    notify(&mut dev, &mut mem);
    complete(&dev, &mem, used);
}

#[test]
fn legacy_pio_queue_may_complete_before_driver_ok() {
    let (mut dev, mut mem, _media, used) = fixture("legacy-ready-exception", true);
    assert_eq!(dev.stats().status & 4, 0);
    dev.legacy_io_access(
        0x10,
        VirtioPciBlockOp::Write { size: 2, value: 0 },
        &mut mem,
    );
    complete(&dev, &mem, used);
}

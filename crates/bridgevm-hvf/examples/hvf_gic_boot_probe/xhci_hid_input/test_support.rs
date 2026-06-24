use bridgevm_hvf::dtb::VirtFdtConfig;
use bridgevm_hvf::machine;
use bridgevm_hvf::platform_virt::{FlatGuestRam, MmioOp, MmioOutcome, VirtPlatform};

pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(crate) fn new_platform() -> VirtPlatform {
    VirtPlatform::new(VirtFdtConfig::default())
}

pub(crate) fn emit_uart(platform: &mut VirtPlatform, bytes: &[u8]) {
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
    for byte in bytes {
        assert_eq!(
            platform.on_mmio(
                machine::UART.base,
                MmioOp::Write {
                    size: 1,
                    value: u64::from(*byte),
                },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );
    }
}

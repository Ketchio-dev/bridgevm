use bridgevm_hvf::bridgevm_pc_boot_info::BOOT_INFO_MAGIC;
use bridgevm_hvf::machine::bridgevm_pc as board;

pub const MEMORY_SIZE: usize = 0x1_0000;
pub const RESULT_OFFSET: usize = 0x1000;
pub const PASS_RESULT: u32 = 1;
pub const BOOT_MAGIC: u64 = u64::from_le_bytes(*BOOT_INFO_MAGIC);
pub const RSDP_MAGIC: u64 = u64::from_le_bytes(*b"RSD PTR ");
pub const XSDT_MAGIC: u64 = u32::from_le_bytes(*b"XSDT") as u64;

// The guest checks BVMBOOT1, ABI v1, the 112-byte header checksum, the RSDP
// signature, the equality of the RSDP and header XSDT pointers, and the XSDT
// signature. It writes 1 on success or a stage-specific 2..=7 failure code,
// then exits through HVC #0.
pub const CODE: [u32; 41] = [
    0xf940_0002,
    0xeb08_005f,
    0x5400_0341,
    0xb940_0802,
    0x7100_045f,
    0x5400_0321,
    0x5280_0003,
    0xd280_0e04,
    0xaa00_03e5,
    0x3840_14a6,
    0x0b06_0063,
    0xf100_0484,
    0x54ff_ffa1,
    0x1200_1c63,
    0x3500_0243,
    0xf940_0c02,
    0xf940_0043,
    0xeb09_007f,
    0x5400_0201,
    0xf940_1403,
    0xf940_0c42,
    0xeb03_005f,
    0x5400_01c1,
    0xb940_0043,
    0x6b0a_007f,
    0x5400_01a1,
    0x5280_0022,
    0x1400_000c,
    0x5280_0042,
    0x1400_000a,
    0x5280_0062,
    0x1400_0008,
    0x5280_0082,
    0x1400_0006,
    0x5280_00a2,
    0x1400_0004,
    0x5280_00c2,
    0x1400_0002,
    0x5280_00e2,
    0xb900_0022,
    0xd400_0002,
];

pub fn result_gpa() -> Result<u64, String> {
    board::RAM_BASE
        .checked_add(RESULT_OFFSET as u64)
        .ok_or_else(|| "BridgeVM PC live-probe result GPA overflow".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bridgevm_hvf::bridgevm_pc_boot_info::BOOT_INFO_HEADER_SIZE;

    #[test]
    fn guest_contract_constants_bind_boot_info_v1() {
        assert_eq!(board::BOARD_ABI_VERSION, 1);
        assert_eq!(BOOT_INFO_HEADER_SIZE, 112);
        assert_eq!(BOOT_MAGIC.to_le_bytes(), *b"BVMBOOT1");
        assert_eq!(RSDP_MAGIC.to_le_bytes(), *b"RSD PTR ");
        assert_eq!((XSDT_MAGIC as u32).to_le_bytes(), *b"XSDT");
        assert_eq!(result_gpa().unwrap(), board::RAM_BASE + 0x1000);
    }

    #[test]
    fn guest_image_is_bounded_and_terminates_with_hvc() {
        assert!(CODE.len() * 4 < RESULT_OFFSET);
        assert_eq!(CODE[9], 0x3840_14a6);
        assert_eq!(CODE[12], 0x54ff_ffa1);
        assert_eq!(CODE[39], 0xb900_0022);
        assert_eq!(CODE[40], 0xd400_0002);
    }
}

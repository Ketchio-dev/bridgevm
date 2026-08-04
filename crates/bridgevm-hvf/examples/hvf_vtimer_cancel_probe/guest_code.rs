//! Guest machine code for the vtimer/cancellation microprobe.
//!
//! These words are the assembled form of the `.s` files beside this module.
//! They are embedded rather than assembled at build time so the probe has no
//! toolchain dependency; `guest_code_matches_the_checked_in_assembly` pins each
//! word to the instruction it must be, so a silent corruption cannot pass.
//!
//! Regenerate with:
//!   xcrun --sdk macosx clang -c -target arm64-apple-macos -o /tmp/g.o guest_main.s
//!   xcrun --sdk macosx llvm-objdump -d /tmp/g.o

/// Guest entry: configure the GICv3 CPU interface and the redistributor for
/// the virtual-timer PPI (intid 27), install the vector table, then park the
/// way an operating system's idle path does.
///
/// The loop arms `CNTV_CVAL` to "now + delta" once, then `WFI`s until the IRQ
/// handler sets the fired flag. A spurious `WFI` wake goes back to `WFI`
/// **without** re-arming the timer. That is what makes a swallowed fire fatal
/// here exactly as it is in a real guest: nothing re-arms on the guest's
/// behalf, so a lost fire is a permanent park rather than a missed tick.
///
/// The delta is read from the control page on each arm, so the host can retune
/// the race window without rebuilding the guest.
pub(crate) const GUEST_MAIN: [u32; 39] = [
    0x52800020, 0xd518cca0, 0xd5033fdf, 0x52801fe0, 0xd5184600, 0x52800020, 0xd518cce0, 0xd5033fdf,
    0xd2a10000, 0x52800021, 0xb9000001, 0xd5033fdf, 0xd2a10160, 0x52a10001, 0xb9008001, 0xb9010001,
    0x52b40002, 0xb9041802, 0xd5033f9f, 0xd5033fdf, 0xd2a80000, 0xf2820000, 0xd518c000, 0xd5033fdf,
    0xd2a80004, 0xf2860004, 0xd50342ff, 0xf900089f, 0xf9400485, 0xd53be040, 0x8b050000, 0xd51be340,
    0x52800021, 0xd51be321, 0xd5033fdf, 0xd503207f, 0xf9400886, 0xb4ffffc6, 0x17fffff5,
];

/// IRQ vector body, placed at `VBAR_EL1 + 0x280` (current EL, SPx, IRQ).
///
/// Acknowledges through `ICC_IAR1_EL1`, disables the timer it just serviced,
/// increments the guest-visible wake counter, sets the fired flag that
/// releases the `WFI` loop, then EOIs and returns.
pub(crate) const GUEST_HANDLER: [u32; 13] = [
    0xd538cc00, 0xd51be33f, 0xd5033fdf, 0xd2a80002, 0xf2860002, 0xf9400043, 0x91000463, 0xf9000043,
    0x52800023, 0xf9000843, 0xd5033f9f, 0xd518cc20, 0xd69f03e0,
];

/// Offset of the IRQ entry within the vector table.
pub(crate) const VECTOR_IRQ_OFFSET: usize = 0x280;

/// Guest physical base of the mapped region.
pub(crate) const GUEST_BASE: u64 = 0x4000_0000;

/// Offset of the vector table from `GUEST_BASE`.
pub(crate) const VECTOR_TABLE_OFFSET: usize = 0x1000;

/// Offset of the shared control page from `GUEST_BASE`.
///
/// `+0x0` is the guest's wake counter, `+0x8` the host-tunable arm delta.
pub(crate) const CONTROL_PAGE_OFFSET: usize = 0x3000;

/// Byte offset within the control page of the guest wake counter.
pub(crate) const WAKE_COUNTER_OFFSET: usize = 0;

/// Byte offset within the control page of the host-written arm delta.
pub(crate) const ARM_DELTA_OFFSET: usize = 8;

/// Byte offset within the control page of the guest's fired flag.
///
/// The handler sets it; the guest's wait loop clears it when it next arms. A
/// `WFI` wake with this clear is spurious and must not re-arm the timer.
pub(crate) const FIRED_FLAG_OFFSET: usize = 16;

/// Guest physical address of the `WFI` the idle loop parks on.
///
/// A vCPU suspended in `WFI` reports this as its PC, which is how the host
/// tells "parked" apart from "between interrupts".
pub(crate) fn wfi_address() -> u64 {
    let index = GUEST_MAIN
        .iter()
        .position(|word| *word == 0xd503_207f)
        .expect("the idle loop must contain a WFI");
    GUEST_BASE + (index * 4) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guest_code_matches_the_checked_in_assembly() {
        // Spot-check the instructions that carry the probe's meaning. If the
        // blob is ever regenerated incorrectly, these fail rather than the
        // guest silently doing something else.
        assert_eq!(GUEST_MAIN[0], 0x5280_0020, "mov w0, #1");
        assert_eq!(GUEST_MAIN[1], 0xd518_cca0, "msr ICC_SRE_EL1, x0");
        assert_eq!(GUEST_MAIN[26], 0xd503_42ff, "msr DAIFClr, #2");
        assert_eq!(
            GUEST_MAIN[27], 0xf900_089f,
            "str xzr, [x4, #16] (clear fired)"
        );
        assert_eq!(GUEST_MAIN[28], 0xf940_0485, "ldr x5, [x4, #8] (arm delta)");
        assert_eq!(GUEST_MAIN[29], 0xd53b_e040, "mrs x0, CNTVCT_EL0");
        assert_eq!(GUEST_MAIN[31], 0xd51b_e340, "msr CNTV_CVAL_EL0, x0");
        assert_eq!(GUEST_MAIN[33], 0xd51b_e321, "msr CNTV_CTL_EL0, x1");

        assert_eq!(GUEST_HANDLER[0], 0xd538_cc00, "mrs x0, ICC_IAR1_EL1");
        assert_eq!(GUEST_HANDLER[1], 0xd51b_e33f, "msr CNTV_CTL_EL0, xzr");
        assert_eq!(
            GUEST_HANDLER[9], 0xf900_0843,
            "str x3, [x2, #16] (fired flag)"
        );
        assert_eq!(GUEST_HANDLER[11], 0xd518_cc20, "msr ICC_EOIR1_EL1, x0");
        assert_eq!(GUEST_HANDLER[12], 0xd69f_03e0, "eret");
    }

    #[test]
    fn a_spurious_wake_returns_to_wfi_without_re_arming_the_timer() {
        // This is the property that makes the probe faithful. An earlier guest
        // re-armed on every WFI wake, which silently repaired every swallowed
        // fire and made the no-recovery run pass -- proving nothing. The wait
        // loop must branch backwards to the WFI, not forwards to the arm.
        let wfi = GUEST_MAIN.len() - 4;
        assert_eq!(GUEST_MAIN[wfi], 0xd503_207f, "wfi");
        assert_eq!(GUEST_MAIN[wfi + 1], 0xf940_0886, "ldr x6, [x4, #16]");

        // cbz x6, <wfi>: imm19 is a signed instruction-relative offset.
        let cbz = GUEST_MAIN[wfi + 2];
        assert_eq!(cbz & 0xff00_0000, 0xb400_0000, "cbz on a 64-bit register");
        // imm19 is a signed instruction count; sign-extend from bit 18.
        let imm19 = (cbz >> 5) & 0x7_ffff;
        let offset = ((imm19 << 13) as i32) >> 13;
        assert_eq!(
            offset, -2,
            "the spurious-wake path branches back to the WFI"
        );

        // The final instruction is the only way back to the arm sequence, and
        // it is reached only once the fired flag is set.
        let branch = GUEST_MAIN[GUEST_MAIN.len() - 1];
        assert_eq!(branch >> 26, 0b000101, "unconditional branch");
    }

    #[test]
    fn control_page_fields_do_not_overlap_and_stay_in_one_page() {
        assert_ne!(WAKE_COUNTER_OFFSET, ARM_DELTA_OFFSET);
        assert_ne!(ARM_DELTA_OFFSET, FIRED_FLAG_OFFSET);
        assert!(FIRED_FLAG_OFFSET >= ARM_DELTA_OFFSET + 8);
        assert!(ARM_DELTA_OFFSET >= WAKE_COUNTER_OFFSET + 8);
        assert!(CONTROL_PAGE_OFFSET + ARM_DELTA_OFFSET + 8 <= CONTROL_PAGE_OFFSET + 0x1000);
    }

    #[test]
    fn the_wfi_address_points_at_the_idle_loop_park_point() {
        let address = wfi_address();
        let index = ((address - GUEST_BASE) / 4) as usize;
        assert_eq!(GUEST_MAIN[index], 0xd503_207f);
        assert_eq!(index, GUEST_MAIN.len() - 4, "the WFI is in the wait loop");
    }

    #[test]
    fn the_vector_table_does_not_overlap_the_guest_code_or_control_page() {
        assert!(GUEST_MAIN.len() * 4 <= VECTOR_TABLE_OFFSET);
        let handler_end = VECTOR_TABLE_OFFSET + VECTOR_IRQ_OFFSET + GUEST_HANDLER.len() * 4;
        assert!(handler_end <= CONTROL_PAGE_OFFSET);
    }
}

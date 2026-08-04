// Bare-metal EL1 guest modelling how an OS parks on the virtual timer:
// arm CNTV once, then WFI until the IRQ handler reports the timer actually
// fired. A spurious WFI wake returns to WFI *without* re-arming, which is what
// makes a swallowed fire fatal here exactly as it is in a real guest.
    movz w0, #1
    msr  S3_0_C12_C12_5, x0        // ICC_SRE_EL1 = SRE
    isb
    movz w0, #0xff
    msr  S3_0_C4_C6_0, x0          // ICC_PMR_EL1 = 0xff
    movz w0, #1
    msr  S3_0_C12_C12_7, x0        // ICC_IGRPEN1_EL1 = 1
    isb

    movz x0, #0x0800, lsl #16      // GICD base 0x08000000
    movz w1, #1
    str  w1, [x0]                  // GICD_CTLR = EnableGrp1
    isb

    movz x0, #0x080b, lsl #16      // redistributor SGI frame 0x080b0000
    movz w1, #0x0800, lsl #16      // bit 27 = vtimer PPI
    str  w1, [x0, #0x80]           // GICR_IGROUPR0 -> group 1
    str  w1, [x0, #0x100]          // GICR_ISENABLER0 -> enable PPI 27
    movz w2, #0xa000, lsl #16
    str  w2, [x0, #0x418]          // GICR_IPRIORITYR: intid 27 priority 0xa0
    dsb  sy
    isb

    movz x0, #0x4000, lsl #16
    movk x0, #0x1000
    msr  vbar_el1, x0              // vector table at 0x40001000
    isb
    movz x4, #0x4000, lsl #16
    movk x4, #0x3000               // control page
    msr  daifclr, #2               // unmask IRQ

arm:
    str  xzr, [x4, #16]            // clear the fired flag
    ldr  x5, [x4, #8]              // host-tunable arm delta
    mrs  x0, cntvct_el0
    add  x0, x0, x5
    msr  cntv_cval_el0, x0
    movz w1, #1
    msr  cntv_ctl_el0, x1          // ENABLE=1, IMASK=0
    isb
wait:
    wfi
    ldr  x6, [x4, #16]
    cbz  x6, wait                  // spurious wake: park again, do NOT re-arm
    b    arm

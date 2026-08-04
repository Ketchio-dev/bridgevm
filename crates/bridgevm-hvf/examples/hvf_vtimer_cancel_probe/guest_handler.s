// IRQ vector (VBAR_EL1 + 0x280): ack, stop the timer, publish the wake, EOI.
    mrs  x0, S3_0_C12_C12_0        // ICC_IAR1_EL1
    msr  cntv_ctl_el0, xzr         // disable the timer we just serviced
    isb
    movz x2, #0x4000, lsl #16
    movk x2, #0x3000
    ldr  x3, [x2]
    add  x3, x3, #1
    str  x3, [x2]                  // guest-visible wake counter
    movz w3, #1
    str  x3, [x2, #16]             // fired flag: releases the WFI loop
    dsb  sy
    msr  S3_0_C12_C12_1, x0        // ICC_EOIR1_EL1
    eret

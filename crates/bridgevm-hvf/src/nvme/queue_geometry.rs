//! Geometry shared by Create I/O Completion and Submission Queue commands.

use super::*;

// NVMe 1.4 §§4.1.3, 5.3–5.4: command-specific Invalid Queue Size.
const SC_INVALID_QUEUE_SIZE: u16 = 0x0102;
const PHYSICALLY_CONTIGUOUS: u32 = CREATE_IO_CQ_PC_BIT;

pub(super) fn io_queue_size(cmd: &SubmissionEntry) -> Result<u16, u16> {
    let zero_based = (cmd.cdw10 >> 16) as u16;
    if zero_based == 0 || zero_based >= MAX_QUEUE_ENTRIES {
        return Err(SC_INVALID_QUEUE_SIZE);
    }
    // CAP.CQR is set: PRP1 must be a contiguous buffer, not a PRP list.
    if cmd.cdw11 & PHYSICALLY_CONTIGUOUS == 0 {
        return Err(SC_INVALID_FIELD);
    }
    Ok(zero_based + 1)
}

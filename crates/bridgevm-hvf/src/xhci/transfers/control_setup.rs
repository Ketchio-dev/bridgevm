use crate::fwcfg::GuestMemoryMut;

use super::trb::{read_transfer_trb, trace_transfer_trb, trb_type};

const LINK_TRB_POINTER_MASK: u64 = !0xf;
const TRB_TYPE_LINK: u32 = 6;

pub(super) enum ControlSetupStart {
    Ready(u64),
    ReadFailed(u64),
}

pub(super) fn control_setup_start(
    mem: &dyn GuestMemoryMut,
    current_dequeue: u64,
) -> ControlSetupStart {
    let Some(candidate) = read_transfer_trb(mem, current_dequeue) else {
        return ControlSetupStart::ReadFailed(current_dequeue);
    };
    if trb_type(candidate.control) == TRB_TYPE_LINK {
        trace_transfer_trb("setup_link", candidate);
        return ControlSetupStart::Ready(candidate.parameter & LINK_TRB_POINTER_MASK);
    }
    ControlSetupStart::Ready(current_dequeue)
}

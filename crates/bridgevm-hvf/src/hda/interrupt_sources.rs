//! Controller and stream interrupt masks.

use super::*;

const CORBCTL_CMEIE: u8 = 1;

impl HdaController {
    pub(crate) fn interrupt_sources(&self) -> u32 {
        let mut sources = 0;
        let controller_pending = (self.rirb_sts & RIRBSTS_RINTFL != 0
            && self.rirb_ctl & RIRBCTL_RINTCTL != 0)
            || (self.rirb_sts & RIRBSTS_OIS != 0 && self.rirb_ctl & RIRBCTL_OIC != 0)
            || (self.corb_sts & CORBSTS_CMEI != 0 && self.corb_ctl & CORBCTL_CMEIE != 0);
        if controller_pending && self.intctl & INTCTL_CIE != 0 {
            sources |= INTSTS_CIS;
        }
        let stream_pending = (self.stream.sts & SDSTS_BCIS != 0
            && self.stream.ctl & SDCTL_IOCE != 0)
            || (self.stream.sts & SDSTS_FIFOE != 0 && self.stream.ctl & SDCTL_FEIE != 0)
            || (self.stream.sts & SDSTS_DESE != 0 && self.stream.ctl & SDCTL_DEIE != 0);
        if stream_pending && self.intctl & INTCTL_STREAM0 != 0 {
            sources |= INTCTL_STREAM0;
        }
        sources
    }
}

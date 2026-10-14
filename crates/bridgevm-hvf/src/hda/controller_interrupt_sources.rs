//! HDA 1.0a sections 3.3.8–3.3.9: enabled codec State Changes request service.

use super::*;

const CORBCTL_CMEIE: u8 = 1;

impl HdaController {
    pub(super) fn controller_interrupt_pending(&self) -> bool {
        (self.statests & self.wakeen != 0)
            || (self.rirb_sts & RIRBSTS_RINTFL != 0 && self.rirb_ctl & RIRBCTL_RINTCTL != 0)
            || (self.rirb_sts & RIRBSTS_OIS != 0 && self.rirb_ctl & RIRBCTL_OIC != 0)
            || (self.corb_sts & CORBSTS_CMEI != 0 && self.corb_ctl & CORBCTL_CMEIE != 0)
    }
}

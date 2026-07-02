pub(super) const XHCI_PORT_COUNT: usize = 8;

const PORTSC_CCS: u32 = 1 << 0;
const PORTSC_PED: u32 = 1 << 1;
const PORTSC_PR: u32 = 1 << 4;
const PORTSC_PLS_SHIFT: u32 = 5;
const PORTSC_PLS_U0: u32 = 0 << PORTSC_PLS_SHIFT;
const PORTSC_PLS_RX_DETECT: u32 = 5 << PORTSC_PLS_SHIFT;
const PORTSC_PLS_POLLING: u32 = 7 << PORTSC_PLS_SHIFT;
pub(super) const PORTSC_PP: u32 = 1 << 9;
const PORTSC_SPEED_HIGH: u32 = 3 << 10;
const PORTSC_CSC: u32 = 1 << 17;
const PORTSC_PRC: u32 = 1 << 21;
pub(super) const PORT_REG_BASE: u64 = 0x440;
pub(super) const PORT_REG_STRIDE: u64 = 0x10;

#[derive(Debug, Clone, Copy)]
pub(super) struct PortState {
    connected: bool,
    enabled: bool,
    connect_change: bool,
    reset_change: bool,
    speed: u32,
}

impl PortState {
    const fn disconnected() -> Self {
        Self {
            connected: false,
            enabled: false,
            connect_change: false,
            reset_change: false,
            speed: 0,
        }
    }

    const fn high_speed_hid_candidate() -> Self {
        Self {
            connected: true,
            enabled: true,
            connect_change: true,
            reset_change: false,
            speed: PORTSC_SPEED_HIGH,
        }
    }

    const fn post_hcrst(self) -> Self {
        // QEMU oracle: xhci_reset re-runs xhci_port_update for every port,
        // which re-posts CSC for attached devices even when a previous driver
        // already acknowledged the connect — the reset controller genuinely
        // re-detects the device.
        Self {
            connected: self.connected,
            enabled: false,
            connect_change: self.connected,
            reset_change: false,
            speed: self.speed,
        }
    }

    pub(super) fn portsc(self) -> u32 {
        let mut value = PORTSC_PP | self.port_link_state();
        if self.connected {
            value |= PORTSC_CCS | self.speed;
        }
        if self.enabled {
            value |= PORTSC_PED;
        }
        if self.connect_change {
            value |= PORTSC_CSC;
        }
        if self.reset_change {
            value |= PORTSC_PRC;
        }
        value
    }

    // QEMU oracle: xhci_port_update sets PLS=RxDetect with no device attached
    // and PLS=Polling for an attached USB2 device awaiting port reset; a
    // completed reset links at U0. PLS=U0 with CCS=1/PED=0 is an illegal USB2
    // combination that host drivers treat as a broken controller.
    const fn port_link_state(self) -> u32 {
        if !self.connected {
            PORTSC_PLS_RX_DETECT
        } else if self.enabled {
            PORTSC_PLS_U0
        } else {
            PORTSC_PLS_POLLING
        }
    }

    pub(super) const fn has_change(self) -> bool {
        self.connect_change || self.reset_change
    }

    pub(super) const fn change_acknowledged_by(self, value: u32) -> bool {
        (value & PORTSC_CSC != 0 && self.connect_change)
            || (value & PORTSC_PRC != 0 && self.reset_change)
    }

    pub(super) fn write_portsc(&mut self, value: u32) -> bool {
        if value & PORTSC_CSC != 0 {
            self.connect_change = false;
        }
        if value & PORTSC_PRC != 0 {
            self.reset_change = false;
        }
        if value & PORTSC_PR != 0 && self.connected {
            self.enabled = true;
            self.reset_change = true;
            return true;
        }
        false
    }
}

pub(super) fn initial_ports() -> [PortState; XHCI_PORT_COUNT] {
    let mut ports = [PortState::disconnected(); XHCI_PORT_COUNT];
    ports[0] = PortState::high_speed_hid_candidate();
    ports
}

pub(super) fn post_hcrst_ports(
    ports: [PortState; XHCI_PORT_COUNT],
) -> [PortState; XHCI_PORT_COUNT] {
    ports.map(PortState::post_hcrst)
}

pub(super) fn port_reg(offset: u64) -> Option<(usize, u64)> {
    let relative = offset.checked_sub(PORT_REG_BASE)?;
    if relative >= XHCI_PORT_COUNT as u64 * PORT_REG_STRIDE {
        return None;
    }
    Some((
        (relative / PORT_REG_STRIDE) as usize,
        relative % PORT_REG_STRIDE,
    ))
}

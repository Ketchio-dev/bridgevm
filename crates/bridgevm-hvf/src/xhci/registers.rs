use super::{
    event::XHCI_INTERRUPTER_COUNT,
    ports::{port_reg, post_hcrst_ports},
    trace, XhciController, USB_CMD_HCRST,
};

const INTERRUPTER_REG_BASE: u64 = 0x1020;
const INTERRUPTER_REG_STRIDE: u64 = 0x20;

fn interrupter_reg(offset: u64) -> Option<(usize, u64)> {
    let relative = offset.checked_sub(INTERRUPTER_REG_BASE)?;
    if relative >= XHCI_INTERRUPTER_COUNT as u64 * INTERRUPTER_REG_STRIDE {
        return None;
    }
    Some((
        (relative / INTERRUPTER_REG_STRIDE) as usize,
        relative % INTERRUPTER_REG_STRIDE,
    ))
}

impl XhciController {
    pub(super) fn register_byte(&self, offset: u64) -> u8 {
        let shift = ((offset & 0x3) * 8) as u32;
        ((self.read_dword(offset & !0x3) >> shift) & 0xff) as u8
    }

    pub(super) fn read_dword(&self, offset: u64) -> u32 {
        if let Some((port, reg)) = port_reg(offset) {
            return if reg == 0x0 {
                self.ports[port].portsc()
            } else {
                0
            };
        }
        if let Some((index, reg)) = interrupter_reg(offset) {
            let interrupter = &self.interrupters[index];
            return match reg {
                0x00 => interrupter.iman,
                0x04 => interrupter.imod,
                0x08 => interrupter.erstsz,
                0x10 => interrupter.erstba as u32,
                0x14 => (interrupter.erstba >> 32) as u32,
                0x18 => self.erdp_low(index),
                0x1c => (interrupter.erdp >> 32) as u32,
                _ => 0,
            };
        }
        match offset {
            0x00 => 0x0100_0040,
            0x04 => 0x0800_1040,
            0x08 => 0x0000_000f,
            0x0c => 0x0000_0000,
            0x10 => 0x0008_7001,
            0x14 => 0x0000_2000,
            0x18 => 0x0000_1000,
            0x1c => 0x0000_0000,
            0x20 => 0x0200_0402,
            0x24 => 0x2042_5355,
            0x28 => 0x0000_0401,
            0x2c => 0x0000_0000,
            0x30 => 0x0300_0002,
            0x34 => 0x2042_5355,
            0x38 => 0x0000_0405,
            0x3c => 0x0000_0000,
            0x40 => self.usb_command,
            0x44 => self.usb_status(),
            0x48 => 0x0000_0001,
            0x54 => self.dnctrl,
            0x58 => self.crcr as u32,
            0x5c => (self.crcr >> 32) as u32,
            0x70 => self.dcbaap as u32,
            0x74 => (self.dcbaap >> 32) as u32,
            0x78 => self.config,
            0x1000 => 0x0000_0003,
            _ => 0,
        }
    }

    pub(super) fn write_dword(&mut self, offset: u64, value: u32) -> bool {
        if let Some((port, reg)) = port_reg(offset) {
            if reg == 0x0 {
                let acknowledged_change = self.ports[port].change_acknowledged_by(value);
                let generated_change = self.ports[port].write_portsc(value);
                if acknowledged_change && !self.ports.iter().any(|port| port.has_change()) {
                    self.port_status_change_pending = false;
                }
                return generated_change;
            }
            return false;
        }
        if let Some((index, reg)) = interrupter_reg(offset) {
            match reg {
                0x00 => self.write_iman(index, value),
                0x04 => self.interrupters[index].imod = value,
                0x08 => {
                    self.interrupters[index].erstsz = value;
                    self.reset_event_ring(index);
                }
                0x10 => {
                    let interrupter = &mut self.interrupters[index];
                    interrupter.erstba = (interrupter.erstba & !0xffff_ffff) | u64::from(value);
                    self.reset_event_ring(index);
                }
                0x14 => {
                    let interrupter = &mut self.interrupters[index];
                    interrupter.erstba =
                        (interrupter.erstba & 0xffff_ffff) | (u64::from(value) << 32);
                    self.reset_event_ring(index);
                }
                0x18 => self.write_erdp_low(index, value),
                0x1c => self.write_erdp_high(index, value),
                _ => {}
            }
            return false;
        }
        match offset {
            0x40 => {
                if value & USB_CMD_HCRST != 0 {
                    trace::host_controller_reset(value);
                    self.ports = post_hcrst_ports(self.ports);
                    self.reset_programmed_state();
                    if self.ports.iter().any(|port| port.has_change()) {
                        self.mark_port_status_change_pending();
                    }
                }
                self.usb_command = value & !USB_CMD_HCRST;
            }
            0x54 => self.dnctrl = value,
            0x58 => self.write_crcr_low(value),
            0x5c => self.write_crcr_high(value),
            0x70 => self.dcbaap = (self.dcbaap & !0xffff_ffff) | u64::from(value),
            0x74 => self.dcbaap = (self.dcbaap & 0xffff_ffff) | (u64::from(value) << 32),
            0x78 => self.config = value,
            _ => {}
        }
        false
    }
}

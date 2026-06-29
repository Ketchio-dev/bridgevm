use super::{
    ports::{initial_ports, port_reg},
    trace, XhciController, USB_CMD_HCRST,
};

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
            0x28 => 0x0000_0405,
            0x2c => 0x0000_0000,
            0x30 => 0x0300_0002,
            0x34 => 0x2042_5355,
            0x38 => 0x0000_0401,
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
            0x1020 => self.iman0,
            0x1024 => self.imod0,
            0x1028 => self.erstsz0,
            0x1030 => self.erstba0 as u32,
            0x1034 => (self.erstba0 >> 32) as u32,
            0x1038 => self.erdp_low(),
            0x103c => (self.erdp0 >> 32) as u32,
            _ => 0,
        }
    }

    pub(super) fn write_dword(&mut self, offset: u64, value: u32) {
        if let Some((port, reg)) = port_reg(offset) {
            if reg == 0x0 {
                self.ports[port].write_portsc(value);
            }
            return;
        }
        match offset {
            0x40 => {
                if value & USB_CMD_HCRST != 0 {
                    trace::host_controller_reset(value);
                    self.ports = initial_ports();
                    self.reset_programmed_state();
                }
                self.usb_command = value & !USB_CMD_HCRST;
            }
            0x54 => self.dnctrl = value,
            0x58 => self.write_crcr_low(value),
            0x5c => self.write_crcr_high(value),
            0x70 => self.dcbaap = (self.dcbaap & !0xffff_ffff) | u64::from(value),
            0x74 => self.dcbaap = (self.dcbaap & 0xffff_ffff) | (u64::from(value) << 32),
            0x78 => self.config = value,
            0x1020 => self.write_iman0(value),
            0x1024 => self.imod0 = value,
            0x1028 => {
                self.erstsz0 = value;
                self.reset_event_ring();
            }
            0x1030 => {
                self.erstba0 = (self.erstba0 & !0xffff_ffff) | u64::from(value);
                self.reset_event_ring();
            }
            0x1034 => {
                self.erstba0 = (self.erstba0 & 0xffff_ffff) | (u64::from(value) << 32);
                self.reset_event_ring();
            }
            0x1038 => self.write_erdp_low(value),
            0x103c => self.write_erdp_high(value),
            _ => {}
        }
    }
}

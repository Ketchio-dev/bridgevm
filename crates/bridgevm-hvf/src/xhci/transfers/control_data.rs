use crate::fwcfg::GuestMemoryMut;

use super::super::usb::ControlInData;

pub(super) fn write_control_in_data(
    mem: &mut dyn GuestMemoryMut,
    gpa: u64,
    data: ControlInData,
    len: usize,
) -> bool {
    match data {
        ControlInData::Static(bytes) => mem.write_bytes(gpa, &bytes[..len]),
        ControlInData::Byte(value) => {
            let bytes = [value];
            mem.write_bytes(gpa, &bytes[..len])
        }
    }
}

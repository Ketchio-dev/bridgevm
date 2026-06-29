pub(crate) fn host_controller_reset(usb_command: u32) {
    if super::trace::bringup_enabled() {
        println!("{}", format_host_controller_reset(usb_command));
    }
}

fn format_host_controller_reset(usb_command: u32) -> String {
    format!("xHCI USBCMD HCRST observed usb_command={usb_command:#x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_controller_reset_trace_includes_usb_command_value() {
        let line = format_host_controller_reset(0x2);

        assert_eq!(line, "xHCI USBCMD HCRST observed usb_command=0x2");
        assert!(line.contains("HCRST"));
        assert!(line.contains("usb_command=0x2"));
    }
}

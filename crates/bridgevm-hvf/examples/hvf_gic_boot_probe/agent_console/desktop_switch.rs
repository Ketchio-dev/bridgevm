//! Brings the Windows desktop to the resolution the firmware set. Windows keeps
//! the desktop mode it saved for the display, so after the VMM changes the
//! firmware's preferred resolution (`uefi_resolution`) an installed Windows
//! shows its old desktop scaled into the new framebuffer. In the boot that
//! changed it, the agent switches the desktop to that mode once its service
//! starts, as picking the recommended resolution in Settings does; Windows
//! saves it, so later boots need nothing.

use std::sync::atomic::{AtomicU64, Ordering};

use super::protocol::base64_encode_into;

/// The requested mode as `width << 32 | height`; 0 when none or already sent.
static REQUESTED: AtomicU64 = AtomicU64::new(0);

/// Ask for a desktop switch to `width`x`height` at the next service start.
pub fn request_desktop_mode((width, height): (u32, u32)) {
    REQUESTED.store(u64::from(width) << 32 | u64::from(height), Ordering::Relaxed);
}

/// The agent command for the requested switch, once.
pub(super) fn take_desktop_switch_command() -> Option<String> {
    let mode = REQUESTED.swap(0, Ordering::Relaxed);
    (mode != 0).then(|| desktop_switch_command((mode >> 32) as u32, mode as u32))
}

fn desktop_switch_command(width: u32, height: u32) -> String {
    let script = format!(
        "$ErrorActionPreference='Stop'; Add-Type -TypeDefinition 'using System; \
         using System.Runtime.InteropServices; public static class BvDesk {{ \
         [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] public struct M {{ \
         [MarshalAs(UnmanagedType.ByValTStr, SizeConst=32)] public string N; public short V1, V2, Size, Extra; \
         public int Fields, X, Y, O, F; public short C1, C2, C3, C4, C5; \
         [MarshalAs(UnmanagedType.ByValTStr, SizeConst=32)] public string Form; public short Lp; \
         public int Bpp, W, H, Fl, Hz, I1, I2, I3, I4, R1, R2, P1, P2; }} \
         [DllImport(\"user32.dll\", CharSet=CharSet.Unicode)] public static extern bool EnumDisplaySettings(string d, int i, ref M m); \
         [DllImport(\"user32.dll\", CharSet=CharSet.Unicode)] public static extern int ChangeDisplaySettingsEx(string d, ref M m, IntPtr h, int f, IntPtr l); }}'; \
         $m = New-Object BvDesk+M; $m.Size = 220; \
         $null = [BvDesk]::EnumDisplaySettings([NullString]::Value, -1, [ref]$m); \
         $before = '{{0}}x{{1}}' -f $m.W, $m.H; $result = 'unchanged'; \
         if ($m.W -ne {width} -or $m.H -ne {height}) {{ $m.W = {width}; $m.H = {height}; $m.Fields = 0x180000; \
         $result = [BvDesk]::ChangeDisplaySettingsEx([NullString]::Value, [ref]$m, [IntPtr]::Zero, 1, [IntPtr]::Zero) }}; \
         'BVDESKTOP before={{0}} target={width}x{height} result={{1}}' -f $before, $result"
    );
    let utf16: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut line =
        String::from("powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand ");
    base64_encode_into(&utf16, &mut line);
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoded(command: &str) -> String {
        let b64 = command.rsplit(' ').next().unwrap();
        let bytes = super::super::protocol::base64_decode(b64).unwrap();
        let units: Vec<u16> = bytes.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16(&units).unwrap()
    }

    #[test]
    fn one_requested_switch_is_sent_once_with_the_exact_mode() {
        assert_eq!(take_desktop_switch_command(), None);
        request_desktop_mode((1920, 1080));
        let command = take_desktop_switch_command().expect("requested");
        assert!(command.starts_with("powershell -NoProfile -NonInteractive"));
        let script = decoded(&command);
        assert!(script.contains("if ($m.W -ne 1920 -or $m.H -ne 1080)"), "{script}");
        assert!(script.contains("target=1920x1080"));
        assert!(script.contains("EnumDisplaySettings([NullString]::Value, -1"));
        assert_eq!(take_desktop_switch_command(), None, "only once");
    }
}

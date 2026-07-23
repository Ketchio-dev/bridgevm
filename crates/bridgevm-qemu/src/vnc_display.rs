//! Collision-free VNC display-number assignment across concurrent VMs.

use crate::*;

/// The TCP port VNC display `:0` listens on; display `:N` listens on
/// `VNC_BASE_PORT + N`.
pub(crate) const VNC_BASE_PORT: u16 = 5900;

/// How many VNC display numbers to scan for a free one before giving up.
pub(crate) const VNC_DISPLAY_SCAN_LIMIT: u16 = 64;

/// Move a built command's `-display vnc=:0` onto the lowest free VNC display
/// number, so concurrently running Compatibility Mode VMs don't collide on TCP
/// 5900 (the second QEMU would otherwise fail to start with "Failed to find an
/// available port: Address already in use"). The command builder is kept pure
/// and deterministic (always `vnc=:0`); spawn paths call this just before they
/// launch + record the command, so the recorded `-display` reflects the chosen
/// display (the macOS app's viewer endpoint reads `vnc=:N` back to compute the
/// VNC port).
///
/// `avoid` lists display numbers already handed to other live VMs. This is
/// required because a VM's QEMU does not bind its VNC port until partway through
/// startup, so a pure "is the port free right now" probe would hand the same
/// `:0` to two VMs launched back-to-back (the second would then lose the race
/// and fail to start). The caller passes the displays of its running backends so
/// each new VM gets a distinct one.
///
/// Returns `Ok(())` after assigning a display (or as a no-op for a non-VNC
/// display). Returns `Err` if this IS a VNC display but no free number exists in
/// range — so the spawn fails loudly instead of silently leaving the colliding
/// `vnc=:0` template (which would re-introduce the very "Address already in use"
/// crash this function exists to prevent).
pub fn assign_free_vnc_display(command: &mut QemuCommand, avoid: &[u16]) -> Result<(), String> {
    let Some(index) = command.args.iter().position(|arg| arg == "-display") else {
        return Ok(());
    };
    let Some(value) = command.args.get(index + 1) else {
        return Ok(());
    };
    if !value.starts_with("vnc=:") {
        return Ok(());
    }
    let display = lowest_free_vnc_display(VNC_DISPLAY_SCAN_LIMIT, avoid).ok_or_else(|| {
        format!(
            "no free VNC display in range :0..:{VNC_DISPLAY_SCAN_LIMIT} (in use/avoided: {avoid:?}); too many Compatibility Mode VMs are running at once"
        )
    })?;
    command.args[index + 1] = format!("vnc=:{display}");
    Ok(())
}

/// Extract the VNC display number from a rendered command's `-display vnc=:N`
/// (used to collect the displays already in use by running VMs). Returns `None`
/// for a non-VNC display or a malformed value.
pub fn vnc_display_in_command(args: &[String]) -> Option<u16> {
    let index = args.iter().position(|arg| arg == "-display")?;
    args.get(index + 1)?.strip_prefix("vnc=:")?.parse().ok()
}

/// Find the lowest VNC display number that is not in `avoid` and whose TCP port
/// is bindable (free).
pub(crate) fn lowest_free_vnc_display(scan_limit: u16, avoid: &[u16]) -> Option<u16> {
    use std::net::TcpListener;
    (0..scan_limit).find(|display| {
        !avoid.contains(display)
            && TcpListener::bind(("127.0.0.1", VNC_BASE_PORT + display)).is_ok()
    })
}

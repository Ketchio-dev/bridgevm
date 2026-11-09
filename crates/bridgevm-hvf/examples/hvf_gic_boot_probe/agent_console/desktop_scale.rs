//! Sets the Windows display scale of the virtio-gpu display through the guest
//! agent, for `RESIZE WxH@P`.
//!
//! The script finds the active display path whose adapter is the virtio-gpu
//! (PCI 1AF4:1050, viogpudo) and reads its scale with DisplayConfigGetDeviceInfo
//! type -3: the minimum, current and maximum, as steps on [`DESKTOP_SCALES`]
//! from the scale Windows recommends. If the requested scale, clamped to that
//! maximum, differs, it sets it with DisplayConfigSetDeviceInfo type -4 (the
//! call the Settings app makes) and reads it back. It prints one `BVDPI` line.
//!
//! Windows keeps the scale per monitor, relative to its recommendation, and
//! our EDID changes with each size, so the latest scale is applied again after
//! every desktop switch. At most one is pending, for the latest request: it
//! waits until no switch is queued or running, and requests made while one
//! runs give one follow-up.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::OnceLock;

use super::desktop_switch::{powershell_command, powershell_command_head};

/// The scales Windows offers, in percent.
pub const DESKTOP_SCALES: [u32; 12] = [100, 125, 150, 175, 200, 225, 250, 300, 350, 400, 450, 500];

/// Set while the latest requested scale has not been handed to the agent.
const PENDING: u32 = 1 << 31;

/// Everything but the final `[BvS]::Run(P)` call. C# 5 (Windows PowerShell's
/// compiler) inside a PowerShell single-quoted string: no single quotes, no
/// line breaks. Display-config paths are 72 bytes and start with the source's
/// adapter LUID and id, which follow type and size in each request header
/// (bytes 8-20). Type 4 answers the adapter's device path (276 bytes, the path
/// from 20); type -3 the minimum, current and maximum steps (32 bytes, at 20,
/// 24 and 28); type -4 takes one step (24 bytes, at 20).
const SCRIPT_BODY: &str = concat!(
    r#"Add-Type -TypeDefinition '"#,
    r#"using System;using System.Runtime.InteropServices;"#,
    r#"public static class BvS{const string U="user32.dll";"#,
    r#"[DllImport(U)]static extern int QueryDisplayConfig(int f,ref int p,byte[] a,ref int m,byte[] b,IntPtr t);"#,
    r#"[DllImport(U)]static extern int DisplayConfigGetDeviceInfo(byte[] r);"#,
    r#"[DllImport(U)]static extern int DisplayConfigSetDeviceInfo(byte[] r);"#,
    r#"static int[] L={100,125,150,175,200,225,250,300,350,400,450,500};static byte[] S=new byte[12];"#,
    r#"static int G(byte[] b,int o){return BitConverter.ToInt32(b,o);}"#,
    r#"static byte[] Q(int t,int n){var r=new byte[n];BitConverter.GetBytes(t).CopyTo(r,0);BitConverter.GetBytes(n).CopyTo(r,4);S.CopyTo(r,8);return r;}"#,
    r#"static int[] D(){var r=Q(-3,32);return DisplayConfigGetDeviceInfo(r)==0?new[]{G(r,20),G(r,24),G(r,28)}:null;}"#,
    r#"static int X(int i){return L[Math.Max(0,Math.Min(i,L.Length-1))];}"#,
    r#"public static string Run(int w){int p=16,m=32;var a=new byte[p*72];var b=new byte[m*64];bool v=false;"#,
    r#"if(QueryDisplayConfig(2,ref p,a,ref m,b,IntPtr.Zero)==0)for(int i=0;i<p&&!v;i++){Array.Copy(a,i*72,S,0,12);var r=Q(4,276);"#,
    r#"v=DisplayConfigGetDeviceInfo(r)==0&&System.Text.Encoding.Unicode.GetString(r,20,256).ToUpperInvariant().Contains("VEN_1AF4&DEV_1050");}"#,
    r#"var d=v?D():null;"#,
    r#"if(d==null)return "BVDPI target="+w+" before=none after=none rec=none max=none result="+(v?"get_failed":"no_display");"#,
    r#"int z=-d[0],k=d[1],t=Math.Max(0,Math.Min(Array.IndexOf(L,w),z+d[2]));string res="0";"#,
    r#"if(t-z!=k){var r=Q(-4,24);BitConverter.GetBytes(t-z).CopyTo(r,20);res=""+DisplayConfigSetDeviceInfo(r);}"#,
    r#"var e=D();"#,
    r#"return "BVDPI target="+w+" before="+X(z+k)+" after="+(e==null?"none":""+X(z+e[1]))+" rec="+X(z)+" max="+X(z+d[2])+" result="+res;}}';"#,
);

fn requested() -> &'static AtomicU32 {
    #[cfg(not(test))]
    {
        static REQUESTED: AtomicU32 = AtomicU32::new(0);
        &REQUESTED
    }
    // Tests drive harnesses on their own threads; keep their requests apart.
    #[cfg(test)]
    {
        thread_local!(static REQUESTED: &'static AtomicU32 = Box::leak(Box::new(AtomicU32::new(0))));
        REQUESTED.with(|slot| *slot)
    }
}

/// Ask the agent to set the display scale to `percent`, one of
/// [`DESKTOP_SCALES`]; the latest wins.
pub fn request_desktop_scale(percent: u32) {
    debug_assert!(DESKTOP_SCALES.contains(&percent));
    requested().store(percent | PENDING, Ordering::Relaxed);
}

/// A desktop switch brings a new monitor: apply the latest scale, if any, again.
pub(super) fn reapply_desktop_scale() {
    // Once set, the request is never 0 again, so this cannot mark an empty one.
    if requested().load(Ordering::Relaxed) != 0 {
        requested().fetch_or(PENDING, Ordering::Relaxed);
    }
}

pub(super) fn has_requested_scale() -> bool {
    requested_scale() & PENDING != 0
}

/// The latest request with its pending bit; vCPU0 watches it for changes (service_thread.rs).
pub(super) fn requested_scale() -> u32 {
    requested().load(Ordering::Relaxed)
}

/// The agent command for the latest request, once.
pub(super) fn take_desktop_scale_command() -> Option<String> {
    let scale = requested().fetch_and(!PENDING, Ordering::Relaxed);
    (scale & PENDING != 0).then(|| desktop_scale_command(scale & !PENDING))
}

/// Whether `command` is a scale command built here.
pub(super) fn is_desktop_scale_command(command: &str) -> bool {
    static HEAD: OnceLock<String> = OnceLock::new();
    command.starts_with(HEAD.get_or_init(|| powershell_command_head(SCRIPT_BODY)))
}

fn desktop_scale_command(percent: u32) -> String {
    powershell_command(&format!("{SCRIPT_BODY}[BvS]::Run({percent})"))
}

#[cfg(test)]
#[path = "desktop_scale_tests.rs"]
mod tests;

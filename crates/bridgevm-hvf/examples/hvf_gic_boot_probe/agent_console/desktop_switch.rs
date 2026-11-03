//! Switches the Windows desktop to a requested mode through the guest agent.
//!
//! Two callers ask: the boot that changed the firmware's preferred resolution
//! (`uefi_resolution`; Windows keeps the desktop mode it saved) and every live
//! `RESIZE WxH`. With a virtio-gpu display (PCI 1AF4:1050, viogpudo) the
//! script first makes it the only display on the desktop: primary at 0,0, the
//! other attached displays set to 0x0 (their devices stay enabled, so Windows
//! can fall back to them). viogpudo puts the host's size in its custom mode
//! slot, but Windows keeps its cached mode list, so until WxH is listed the
//! script forces a mode re-enumeration (SetDisplayConfig on the active
//! configuration with SDC_FORCE_MODE_ENUMERATION), for up to 10 s, then
//! selects that listed mode. Without a virtio display the primary display is
//! switched directly, as before. The script prints one `BVDISPLAY` line.
//!
//! Only the latest request is kept: requests made before the agent's service
//! starts become one command at `BVAGENT SERVICE start`, a queued switch takes
//! the newest size, and requests made while one runs give one follow-up.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use super::protocol::base64_encode_into;

/// `RUN` lines go through `cmd.exe /c`, which refuses lines over 8191 chars.
pub(super) const MAX_RUN_COMMAND_CHARS: usize = 8191;

const POWERSHELL: &str = "powershell -NoProfile -NonInteractive -EncodedCommand ";

/// Everything but the final `[BvD]::Run(W,H)` call. C# inside a PowerShell
/// single-quoted string: no single quotes. DEVMODEW is 220 bytes (dmSize at
/// 68, dmFields 72, position 76/80, width/height 172/176), DISPLAY_DEVICEW is
/// 840 bytes (name at 4, StateFlags 324, DeviceID 328); the display-config
/// paths (72 bytes) and modes (64 bytes) only round-trip, so they stay bytes.
const SCRIPT_BODY: &str = r#"Add-Type -TypeDefinition '
using System;using System.Collections.Generic;using System.Runtime.InteropServices;using System.Threading;
[module:DefaultCharSet(CharSet.Unicode)]public static class BvD{const string U="user32.dll";
[DllImport(U)]static extern bool EnumDisplayDevicesW(string d,int i,byte[] b,int f);
[DllImport(U)]static extern bool EnumDisplaySettingsW(string d,int i,byte[] m);
[DllImport(U)]static extern int ChangeDisplaySettingsExW(string d,byte[] m,IntPtr w,int f,IntPtr l);
[DllImport(U)]static extern int GetDisplayConfigBufferSizes(int f,out int p,out int m);
[DllImport(U)]static extern int QueryDisplayConfig(int f,ref int p,byte[] a,ref int m,byte[] b,IntPtr t);
[DllImport(U)]static extern int SetDisplayConfig(int p,byte[] a,int m,byte[] b,int f);
static string V,Pr;static bool A;static List<string> O;
static int G(byte[] b,int o){return BitConverter.ToInt32(b,o);}
static void P(byte[] b,int o,int v){BitConverter.GetBytes(v).CopyTo(b,o);}
static string T(byte[] b,int o,int n){return System.Text.Encoding.Unicode.GetString(b,o,n).Split((char)0)[0];}
static byte[] M(string d,int i){var m=new byte[220];m[68]=220;return EnumDisplaySettingsW(d,i,m)?m:null;}
static string S(string d){var m=M(d,-1);return m==null?"none":G(m,172)+"x"+G(m,176);}
static int C(string d,byte[] m,int f){return ChangeDisplaySettingsExW(d,m,IntPtr.Zero,f,IntPtr.Zero);}
static void Scan(){V=null;Pr="none";A=false;O=new List<string>();
for(int i=0;;i++){var b=new byte[840];P(b,0,840);if(!EnumDisplayDevicesW(null,i,b,0))return;
string n=T(b,4,64);int f=G(b,324);
if(V==null&&T(b,328,256).Contains("VEN_1AF4&DEV_1050")){V=n;A=(f&1)!=0;}else if((f&1)!=0)O.Add(n);
if((f&4)!=0)Pr=n;}}
static string Force(){int p,m;GetDisplayConfigBufferSizes(2,out p,out m);var a=new byte[p*72];var b=new byte[m*64];
QueryDisplayConfig(2,ref p,a,ref m,b,IntPtr.Zero);return ""+SetDisplayConfig(p,a,m,b,0x80|0x20|0x400|0x1000);}
public static string Run(int w,int h){
Scan();string t=V,want=w+"x"+h,before=S(t),res="not_listed",force="none",lay="none";var det=new List<string>();
var cur=V!=null&&A?M(V,-1):null;
if(cur!=null&&O.Count>0){P(cur,72,0x20);P(cur,76,0);P(cur,80,0);lay=""+C(V,cur,0x10000011);
foreach(var o in O){var z=new byte[220];z[68]=220;P(z,72,0x180020);det.Add(o+"="+C(o,z,0x10000001));}
lay+="/"+C(null,null,0);}
var end=DateTime.Now.AddSeconds(10);
while(true){if(S(t)==want){res="0";break;}
if(t==null){var m=M(null,-1);P(m,72,0x180000);P(m,172,w);P(m,176,h);res=""+C(null,m,1);break;}
byte[] f=null;for(int n=0;f==null;n++){var m=M(t,n);if(m==null)break;if(G(m,172)==w&&G(m,176)==h)f=m;}
res=f==null?"not_listed":""+C(t,f,1);
if(res!="0")force=Force();
if(DateTime.Now>end)break;Thread.Sleep(400);}
Scan();
return "BVDISPLAY target="+want+" display="+(t??"primary")+" before="+before+" after="+S(t)+" primary="+Pr+" layout="+lay+" detached="+(det.Count==0?"none":string.Join(",",det))+" force="+force+" result="+res;}}'
"#;

fn requested() -> &'static AtomicU64 {
    #[cfg(not(test))]
    {
        static REQUESTED: AtomicU64 = AtomicU64::new(0);
        &REQUESTED
    }
    // Tests drive harnesses on their own threads; keep their requests apart.
    #[cfg(test)]
    {
        thread_local!(static REQUESTED: &'static AtomicU64 = Box::leak(Box::new(AtomicU64::new(0))));
        REQUESTED.with(|slot| *slot)
    }
}

/// Ask the agent to switch the desktop to `width`x`height`; the latest wins.
pub fn request_desktop_mode((width, height): (u32, u32)) {
    requested().store(
        u64::from(width) << 32 | u64::from(height),
        Ordering::Relaxed,
    );
}

pub(super) fn has_requested_mode() -> bool {
    requested().load(Ordering::Relaxed) != 0
}

/// The agent command for the latest request, once.
pub(super) fn take_desktop_switch_command() -> Option<String> {
    let mode = requested().swap(0, Ordering::Relaxed);
    (mode != 0).then(|| desktop_switch_command((mode >> 32) as u32, mode as u32))
}

/// Whether `command` is a switch built here, whatever its size.
pub(super) fn is_desktop_switch_command(command: &str) -> bool {
    command.starts_with(command_head())
}

/// The command up to the last base64 group that holds only `SCRIPT_BODY`.
fn command_head() -> &'static str {
    static HEAD: OnceLock<String> = OnceLock::new();
    HEAD.get_or_init(|| {
        let body = utf16le(SCRIPT_BODY);
        let mut head = String::from(POWERSHELL);
        base64_encode_into(&body[..body.len() / 3 * 3], &mut head);
        head
    })
}

fn utf16le(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn desktop_switch_command(width: u32, height: u32) -> String {
    let script = format!("{SCRIPT_BODY}[BvD]::Run({width},{height})");
    let mut line = String::from(POWERSHELL);
    base64_encode_into(&utf16le(&script), &mut line);
    debug_assert!(line.len() <= MAX_RUN_COMMAND_CHARS);
    line
}

#[cfg(test)]
#[path = "desktop_switch_tests.rs"]
mod tests;

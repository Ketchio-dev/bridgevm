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
//! selects that listed mode and saves it. When the virtio display is the only
//! one, that SetDisplayConfig already asks for a WxH source mode and leaves
//! the target mode to Windows: the switch then happens in one step, where the
//! unchanged configuration first fell back to another listed mode whenever
//! the current one was the previous custom size. If Windows refuses it, the
//! unchanged configuration is applied. Without a virtio display the primary
//! display is switched directly, as before. The script prints one
//! `BVDISPLAY` line.
//!
//! Only the latest request is kept: requests made before the agent's service
//! starts become one command at `BVAGENT SERVICE start`, a queued switch takes
//! the newest size, and requests made while one runs give one follow-up. The
//! latest `RESIZE WxH@P` scale is set again after each switch
//! (desktop_scale.rs).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use super::protocol::base64_encode_into;

/// `RUN` lines go through `cmd.exe /c`, which refuses lines over 8,191
/// characters. bvagent.ps1 runs `& cmd.exe /c $cmd`, so cmd.exe's line is
/// `"C:\Windows\system32\cmd.exe" /c "<command>"`: the command gets what that
/// 35-character wrapper leaves (as in HvfClipboardPasteCommandLimit.swift).
pub(super) const MAX_RUN_COMMAND_CHARS: usize =
    8191 - r#""C:\Windows\system32\cmd.exe" /c """#.len();

const POWERSHELL: &str = "powershell -NoProfile -NonInteractive -EncodedCommand ";

/// Everything but the final `[BvD]::Run(W,H)` call. C# inside a PowerShell
/// single-quoted string: no single quotes. DEVMODEW is 220 bytes (dmSize at
/// 68, dmFields 72, position 76/80, width/height 172/176), DISPLAY_DEVICEW is
/// 840 bytes (name at 4, StateFlags 324, DeviceID 328). Display-config paths
/// are 72 bytes (target mode index at 32), modes 64 (type at 0, source mode
/// width/height at 16/20). No line breaks: they would only cost length.
const SCRIPT_BODY: &str = concat!(
    r#"Add-Type -TypeDefinition '"#,
    r#"using System;using System.Collections.Generic;using System.Runtime.InteropServices;using System.Threading;"#,
    r#"[module:DefaultCharSet(CharSet.Unicode)]public static class BvD{const string U="user32.dll";"#,
    r#"[DllImport(U)]static extern bool EnumDisplayDevicesW(string d,int i,byte[] b,int f);"#,
    r#"[DllImport(U)]static extern bool EnumDisplaySettingsW(string d,int i,byte[] m);"#,
    r#"[DllImport(U)]static extern int ChangeDisplaySettingsExW(string d,byte[] m,IntPtr w,int f,IntPtr l);"#,
    r#"[DllImport(U)]static extern int QueryDisplayConfig(int f,ref int p,byte[] a,ref int m,byte[] b,IntPtr t);"#,
    r#"[DllImport(U)]static extern int SetDisplayConfig(int p,byte[] a,int m,byte[] b,int f);"#,
    r#"static string V,Pr;static bool A;static List<string> O;"#,
    r#"static int G(byte[] b,int o){return BitConverter.ToInt32(b,o);}"#,
    r#"static void P(byte[] b,int o,int v){BitConverter.GetBytes(v).CopyTo(b,o);}"#,
    r#"static string T(byte[] b,int o,int n){return System.Text.Encoding.Unicode.GetString(b,o,n).Split((char)0)[0];}"#,
    r#"static byte[] M(string d,int i){var m=new byte[220];m[68]=220;return EnumDisplaySettingsW(d,i,m)?m:null;}"#,
    r#"static string S(string d){var m=M(d,-1);return m==null?"none":G(m,172)+"x"+G(m,176);}"#,
    r#"static int C(string d,byte[] m,int f){return ChangeDisplaySettingsExW(d,m,IntPtr.Zero,f,IntPtr.Zero);}"#,
    r#"static void Scan(){V=null;Pr="none";A=false;O=new List<string>();"#,
    r#"for(int i=0;;i++){var b=new byte[840];P(b,0,840);if(!EnumDisplayDevicesW(null,i,b,0))return;"#,
    r#"string n=T(b,4,64);int f=G(b,324);"#,
    r#"if(V==null&&T(b,328,256).Contains("VEN_1AF4&DEV_1050")){V=n;A=(f&1)!=0;}else if((f&1)!=0)O.Add(n);"#,
    r#"if((f&4)!=0)Pr=n;}}"#,
    r#"static string Force(int w,int h){string s="";for(int k=A&&O.Count==0?0:1;k<2;k++){int p=16,m=32;"#,
    r#"var a=new byte[p*72];var b=new byte[m*64];QueryDisplayConfig(2,ref p,a,ref m,b,IntPtr.Zero);"#,
    r#"if(k==0){for(int i=0;i<m;i++)if(G(b,i*64)==1){P(b,i*64+16,w);P(b,i*64+20,h);}for(int i=0;i<p;i++)P(a,i*72+32,-1);}"#,
    r#"int r=SetDisplayConfig(p,a,m,b,0x80|0x20|0x400|0x1000);s+=r+";";if(r==0)break;}return s;}"#,
    r#"public static string Run(int w,int h){"#,
    r#"Scan();string t=V,want=w+"x"+h,before=S(t),res="not_listed",force="none",lay="none";var det=new List<string>();"#,
    r#"var cur=V!=null&&A?M(V,-1):null;"#,
    r#"if(cur!=null&&O.Count>0){P(cur,72,0x20);P(cur,76,0);P(cur,80,0);lay=""+C(V,cur,0x10000011);"#,
    r#"foreach(var o in O){var z=new byte[220];z[68]=220;P(z,72,0x180020);det.Add(o+"="+C(o,z,0x10000001));}"#,
    r#"lay+="/"+C(null,null,0);Scan();}"#,
    r#"var end=DateTime.Now.AddSeconds(10);"#,
    r#"if(t==null){res="0";if(S(t)!=want){var m=M(null,-1);P(m,72,0x180000);P(m,172,w);P(m,176,h);res=""+C(null,m,1);}}"#,
    r#"else while(true){byte[] f=null;for(int n=0;f==null;n++){var m=M(t,n);if(m==null)break;if(G(m,172)==w&&G(m,176)==h)f=m;}"#,
    r#"res=f==null?"not_listed":""+C(t,f,1);if(res=="0"&&S(t)==want||DateTime.Now>end)break;"#,
    r#"force=Force(w,h);Thread.Sleep(400);}"#,
    r#"Scan();"#,
    r#"return "BVDISPLAY target="+want+" before="+before+" after="+S(t)+" primary="+Pr+" layout="+lay+" detached="+(det.Count==0?"none":string.Join(",",det))+" force="+force+" result="+res;}}';"#,
);

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
    requested_mode() != 0
}

/// The pending request, 0 for none; vCPU0 watches it for changes (service_thread.rs).
pub(super) fn requested_mode() -> u64 {
    requested().load(Ordering::Relaxed)
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

fn command_head() -> &'static str {
    static HEAD: OnceLock<String> = OnceLock::new();
    HEAD.get_or_init(|| powershell_command_head(SCRIPT_BODY))
}

fn utf16le(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

/// The start of every `powershell_command` whose script begins with `body`:
/// up to the last base64 group that holds only `body`.
pub(super) fn powershell_command_head(body: &str) -> String {
    let body = utf16le(body);
    let mut head = String::from(POWERSHELL);
    base64_encode_into(&body[..body.len() / 3 * 3], &mut head);
    head
}

/// The agent command that runs `script` in Windows PowerShell.
pub(super) fn powershell_command(script: &str) -> String {
    let mut line = String::from(POWERSHELL);
    base64_encode_into(&utf16le(script), &mut line);
    debug_assert!(line.len() <= MAX_RUN_COMMAND_CHARS);
    line
}

fn desktop_switch_command(width: u32, height: u32) -> String {
    powershell_command(&format!("{SCRIPT_BODY}[BvD]::Run({width},{height})"))
}

#[cfg(test)]
#[path = "desktop_switch_tests.rs"]
pub(crate) mod tests;

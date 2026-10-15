//! Legacy text entry pastes non-ASCII through the guest clipboard
//! (HvfClipboardPaste.swift). Clipboard auto-sync must not then publish the
//! typed text, possibly a password, over the Mac pasteboard.

use super::*;
use crate::agent_console::clip_paste::*;

/// `HvfClipboardPaste(base64: b64("한글"), id: 0F6A2C1E-…).command`, generated
/// by compiling HvfClipboardPaste.swift; the app appends it to the control file.
const HANGUL_PASTE: &str = "powershell -NoProfile -STA -Command \"$ErrorActionPreference='Stop';Set-Clipboard -Value ([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('7ZWc6riA'))) -ErrorAction Stop;Write-Output 'BVPASTE_READY 0F6A2C1E-7B3D-4E59-9A8C-5D2E1F0B3A47'\"";

impl ClipRig {
    /// `RUN <b64>` as bvagent.ps1 runs it through cmd.exe: Set-Clipboard
    /// replaces the guest text, and Out-String frames the echoed marker.
    pub(super) fn run_reply(&mut self, line: &str) -> String {
        let b64 = line.trim_end().strip_prefix("RUN ");
        let b64 = b64.unwrap_or_else(|| panic!("unexpected request {line:?}"));
        let command = String::from_utf8(base64_decode(b64).unwrap()).unwrap();
        assert!(
            command.contains("Set-Clipboard"),
            "unexpected command {command:?}"
        );
        let quoted = |open: &str| {
            let rest = &command[command.find(open).unwrap() + open.len()..];
            rest[..rest.find('\'').unwrap()].to_owned()
        };
        let payload = base64_decode(&quoted("FromBase64String('")).unwrap();
        self.guest = String::from_utf8(payload).unwrap();
        let output = format!("{}\r\n", quoted("Write-Output '"));
        format!("OUT 0 {}", base64_encode(output.as_bytes()))
    }
    fn paste(&mut self) {
        self.h
            .queue
            .push_back(ServiceReq::Ctl(HANGUL_PASTE.to_owned()));
    }
}

#[test]
fn legacy_paste_marker_tracks_swift_paste_command() {
    let swift = include_str!(
        "../../../../../apps/macos/Sources/BridgeVMControl/HvfEngine/HvfClipboardPaste.swift"
    );
    let marker = format!("marker = \"{LEGACY_PASTE_MARKER}\\(id.uuidString)\"");
    assert!(
        swift.contains(&marker),
        "Swift paste marker drifted from {marker:?}"
    );
    assert!(swift.contains("Set-Clipboard -Value ") && swift.contains("Write-Output '\\(marker)'"));
    assert!(is_legacy_paste(HANGUL_PASTE));
    assert!(!is_legacy_paste(
        "powershell -Command Set-Clipboard -Value x"
    ));
}

#[test]
fn legacy_paste_is_not_published_to_mac_pasteboard() {
    let mut rig = ClipRig::synced("old");
    rig.paste();
    rig.settle();
    rig.settle();
    assert_eq!(rig.guest, "한글");
    assert!(
        rig.mac_sets().is_empty(),
        "typed text written to the Mac pasteboard"
    );
    assert_eq!(rig.h.last_synced.text.as_deref(), Some("한글"));
}

#[test]
fn guest_copy_after_adopted_paste_still_syncs() {
    let mut rig = ClipRig::synced("old");
    rig.paste();
    rig.settle();
    rig.guest = "guest-copy".into();
    rig.settle();
    assert_eq!(rig.mac_sets(), vec!["guest-copy".to_owned()]);
}

#[test]
fn legacy_paste_dropped_by_agent_restart_is_not_published() {
    let mut rig = ClipRig::synced("old");
    rig.paste();
    rig.enqueue();
    let run = rig.send();
    // Set-Clipboard ran, then the agent restarted before replying.
    rig.run_reply(&run);
    rig.h
        .handle_service_reply("READY GUEST", None, None, rig.now);
    rig.settle();
    rig.settle();
    assert!(
        rig.mac_sets().is_empty(),
        "typed text written to the Mac pasteboard"
    );
    assert_eq!(rig.h.last_synced.text.as_deref(), Some("한글"));
}

#[test]
fn restart_requeued_mac_copy_ahead_of_paste_does_not_publish_paste() {
    let mut rig = ClipRig::synced("old");
    rig.copy_on_mac("mac-new");
    rig.enqueue();
    assert!(rig.send().starts_with("CLIPSET "));
    rig.paste();
    rig.h
        .handle_service_reply("READY GUEST", None, None, rig.now);
    rig.settle();
    rig.settle();
    assert_eq!(rig.guest, "한글");
    assert!(
        rig.mac_sets().is_empty(),
        "typed text written to the Mac pasteboard"
    );
}

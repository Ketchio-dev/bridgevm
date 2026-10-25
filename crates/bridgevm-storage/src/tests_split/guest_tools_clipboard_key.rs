//! The guest-tools runtime record carries the guest's last clipboard text under
//! the JSON key `clipboard`. The app decodes that key for its "Copy to Mac
//! clipboard" route; its GuestClipboardCopyRouteTests decode this exact text.

use crate::*;

#[test]
fn guest_tools_runtime_carries_the_guest_clipboard_under_clipboard() {
    let metadata = GuestToolsRuntimeMetadata {
        connected: true,
        guest_os: Some("linux".to_string()),
        agent_version: Some("0.1.0".to_string()),
        capabilities: vec!["clipboard".to_string()],
        last_heartbeat_at_unix: Some(1_710_000_060),
        guest_ip_addresses: Vec::new(),
        shared_folders: Vec::new(),
        metrics: None,
        last_command_result: None,
        agent_update: None,
        clipboard: Some(GuestToolsClipboardMetadata {
            text: "guest copied text".to_string(),
            updated_at_unix: 1_710_000_064,
        }),
        updated_at_unix: 1_710_000_061,
    };
    let json = serde_json::to_string(&metadata).unwrap();
    let expected = concat!(
        r#"{"connected":true,"guest_os":"linux","agent_version":"0.1.0","#,
        r#""capabilities":["clipboard"],"last_heartbeat_at_unix":1710000060,"#,
        r#""guest_ip_addresses":[],"shared_folders":[],"metrics":null,"#,
        r#""clipboard":{"text":"guest copied text","updated_at_unix":1710000064},"#,
        r#""updated_at_unix":1710000061}"#,
    );
    assert_eq!(json, expected);
}

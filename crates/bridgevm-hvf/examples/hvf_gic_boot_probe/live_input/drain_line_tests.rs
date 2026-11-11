//! The app's switch from the HID route to ordered input counts these lines as the VMM taking a
//! command off its queue (HvfLegacyInputActivity.takesCommand). A wording change here would
//! silently keep every boot on HID, so both sides are pinned.

use super::LiveInputCommand;

const DEVICE_APPLY: &str = include_str!("device_apply.rs");
const APP: &str =
    include_str!("../../../../../apps/macos/Sources/BridgeVMControl/HvfEngine/HvfLegacyInputActivity.swift");

#[test]
fn the_lines_the_app_counts_as_a_taken_command_are_the_ones_the_vmm_prints() {
    for printed in [
        "\"live input accepted: command={command:?}\"",
        "\"live input rejected: command={command:?} queue_error=invalid\"",
        "\"live input rejected: kind=key parse_error={}\"",
        "\"live input rejected: kind=pointer parse_error={}\"",
    ] {
        assert!(DEVICE_APPLY.contains(printed), "device_apply.rs no longer prints {printed}");
    }
    let key = format!("{:?}", LiveInputCommand::Key("text-hex:61".into()));
    let click = format!("{:?}", LiveInputCommand::Pointer("click:1x2".into()));
    let mv = format!("{:?}", LiveInputCommand::Pointer("move:1x2".into()));
    assert!(key.starts_with("Key(") && click.starts_with("Pointer(") && mv.starts_with("Pointer(\"move:"));
    for read in [
        r#""live input accepted: command=""#,
        r#""live input rejected: command=""#,
        r#"" queue_error=invalid""#,
        r#""live input rejected: kind=key parse_error=""#,
        r#""live input rejected: kind=pointer parse_error=""#,
        r#""Key(""#,
        r#""Pointer(""#,
        r#""Pointer(\"move:""#,
    ] {
        assert!(APP.contains(read), "HvfLegacyInputActivity.swift no longer reads {read}");
    }
}

//! Translating protocol window-input events into xdotool invocations and their JSON payloads.

use crate::*;
use anyhow::Result;
use bridgevm_agent_protocol::WindowInputEvent;
use std::path::Path;

pub(crate) fn run_xdotool_window_input(
    program: &Path,
    event: &WindowInputEvent,
) -> Result<(), String> {
    match event {
        WindowInputEvent::Pointer {
            x,
            y,
            action,
            button,
        } => {
            let x_arg = x.to_string();
            let y_arg = y.to_string();
            run_command_status(program, &["mousemove", "--sync", &x_arg, &y_arg])?;
            match action.as_str() {
                "move" => Ok(()),
                "press" => {
                    let button = xdotool_button(button.as_deref())?;
                    run_command_status(program, &["mousedown", button])
                }
                "release" => {
                    let button = xdotool_button(button.as_deref())?;
                    run_command_status(program, &["mouseup", button])
                }
                "click" => {
                    let button = xdotool_button(button.as_deref())?;
                    run_command_status(program, &["click", button])
                }
                _ => Err(format!("unsupported pointer action {action}")),
            }
        }
        WindowInputEvent::Key { key, action } => match action.as_str() {
            "press" => run_command_status(program, &["keydown", key]),
            "release" => run_command_status(program, &["keyup", key]),
            "tap" => run_command_status(program, &["key", key]),
            _ => Err(format!("unsupported key action {action}")),
        },
    }
}

pub(crate) fn xdotool_button(button: Option<&str>) -> Result<&'static str, String> {
    match button {
        Some("left") => Ok("1"),
        Some("middle") => Ok("2"),
        Some("right") => Ok("3"),
        Some(button) => Err(format!("unsupported pointer button {button}")),
        None => Err("pointer button is required".to_string()),
    }
}

pub(crate) fn window_input_payload(event: &WindowInputEvent, source: &str) -> serde_json::Value {
    match event {
        WindowInputEvent::Pointer {
            x,
            y,
            action,
            button,
        } => serde_json::json!({
            "kind": "pointer",
            "x": x,
            "y": y,
            "action": action,
            "button": button,
            "source": source
        }),
        WindowInputEvent::Key { key, action } => serde_json::json!({
            "kind": "key",
            "key": key,
            "action": action,
            "source": source
        }),
    }
}

pub(crate) fn window_input_label(event: &WindowInputEvent) -> &'static str {
    match event {
        WindowInputEvent::Pointer { .. } => "pointer",
        WindowInputEvent::Key { .. } => "key",
    }
}

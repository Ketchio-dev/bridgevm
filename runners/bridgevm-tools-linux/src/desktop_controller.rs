//! The real desktop controller: app launch via gio/gtk-launch and window actions via wmctrl.

use crate::*;
use bridgevm_agent_protocol::WindowInputEvent;
use std::path::PathBuf;

pub(crate) struct DesktopController {
    pub(crate) mode: DesktopControllerMode,
}

pub(crate) enum DesktopControllerMode {
    Simulated,
    Real {
        app_launcher: Option<AppLauncher>,
        window_tool: Option<PathBuf>,
        input_tool: Option<PathBuf>,
    },
}

pub(crate) enum AppLauncher {
    Gio(PathBuf),
    GtkLaunch(PathBuf),
}

impl DesktopController {
    pub(crate) fn simulated() -> Self {
        Self {
            mode: DesktopControllerMode::Simulated,
        }
    }

    pub(crate) fn real(
        app_launcher: Option<AppLauncher>,
        window_tool: Option<PathBuf>,
        input_tool: Option<PathBuf>,
    ) -> Self {
        Self {
            mode: DesktopControllerMode::Real {
                app_launcher,
                window_tool,
                input_tool,
            },
        }
    }

    pub(crate) fn list_applications(&self) -> Option<CommandOutcome> {
        let DesktopControllerMode::Real {
            app_launcher: Some(_),
            ..
        } = &self.mode
        else {
            return None;
        };
        Some(match read_desktop_applications() {
            Ok(applications) => {
                let names = applications
                    .iter()
                    .map(|app| format!("{}:{}", app.id, app.name))
                    .collect::<Vec<_>>()
                    .join(",");
                let payload = applications
                    .iter()
                    .map(|app| {
                        serde_json::json!({
                            "id": app.id,
                            "name": app.name,
                            "launched": false,
                            "source": "linux-desktop-file"
                        })
                    })
                    .collect::<Vec<_>>();
                CommandOutcome::ok_with_result(
                    Some(format!("applications: {names}")),
                    serde_json::json!({ "applications": payload }),
                )
            }
            Err(message) if message == "no visible .desktop applications were found" => {
                return None;
            }
            Err(message) => CommandOutcome::error("applications-list-failed", message),
        })
    }

    pub(crate) fn launch_application(&mut self, id: &str) -> Option<CommandOutcome> {
        let DesktopControllerMode::Real {
            app_launcher: Some(app_launcher),
            ..
        } = &self.mode
        else {
            return None;
        };
        let applications = match read_desktop_applications() {
            Ok(applications) => applications,
            Err(message) if message == "no visible .desktop applications were found" => {
                return None;
            }
            Err(message) => {
                return Some(CommandOutcome::error("applications-list-failed", message))
            }
        };
        let Some(app) = applications.into_iter().find(|app| app.id == id) else {
            return Some(CommandOutcome::error(
                "application-not-found",
                format!("application {id} was not found"),
            ));
        };
        Some(match run_application_launcher(app_launcher, &app) {
            Ok(()) => CommandOutcome::ok_with_result(
                Some(format!("launched application {}", app.name)),
                serde_json::json!({
                    "application": {
                        "id": app.id,
                        "name": app.name,
                        "launched": true,
                        "source": "linux-desktop-file"
                    }
                }),
            ),
            Err(message) => CommandOutcome::error("application-launch-failed", message),
        })
    }

    pub(crate) fn list_windows(&self) -> Option<CommandOutcome> {
        let DesktopControllerMode::Real {
            window_tool: Some(window_tool),
            ..
        } = &self.mode
        else {
            return None;
        };
        Some(match read_wmctrl_windows(window_tool) {
            Ok(windows) => {
                let names = windows
                    .iter()
                    .map(|window| format!("{}:{}", window.id, window.title))
                    .collect::<Vec<_>>()
                    .join(",");
                let payload = windows
                    .iter()
                    .map(|window| {
                        let mut window_payload = desktop_window_payload(window);
                        window_payload["focused"] = serde_json::Value::Bool(false);
                        window_payload
                    })
                    .collect::<Vec<_>>();
                CommandOutcome::ok_with_result(
                    Some(format!("windows: {names}")),
                    serde_json::json!({ "windows": payload }),
                )
            }
            Err(message) if message == "wmctrl reported no desktop windows" => {
                return None;
            }
            Err(message) => CommandOutcome::error("windows-list-failed", message),
        })
    }

    pub(crate) fn focus_window(&mut self, id: &str) -> Option<CommandOutcome> {
        self.run_wmctrl_window_action(id, "-ia", "focused", "focus-window-failed")
    }

    pub(crate) fn close_window(&mut self, id: &str) -> Option<CommandOutcome> {
        self.run_wmctrl_window_action(id, "-ic", "closed", "close-window-failed")
    }

    pub(crate) fn set_window_bounds(
        &mut self,
        id: &str,
        x: i64,
        y: i64,
        width: u64,
        height: u64,
    ) -> Option<CommandOutcome> {
        let DesktopControllerMode::Real {
            window_tool: Some(window_tool),
            ..
        } = &self.mode
        else {
            return None;
        };
        let windows = match read_wmctrl_windows(window_tool) {
            Ok(windows) => windows,
            Err(message) if message == "wmctrl reported no desktop windows" => return None,
            Err(message) => return Some(CommandOutcome::error("windows-list-failed", message)),
        };
        let Some(window) = windows.into_iter().find(|window| window.id == id) else {
            return Some(CommandOutcome::error(
                "window-not-found",
                format!("window {id} was not found"),
            ));
        };

        let geometry = format!("0,{x},{y},{width},{height}");
        Some(
            match run_command_status(window_tool, &["-ir", &window.id, "-e", &geometry]) {
                Ok(()) => {
                    let mut window_payload = desktop_window_payload(&window);
                    window_payload["bounds"] = window_bounds_payload(x, y, width, height);
                    window_payload["bounds_changed"] = serde_json::Value::Bool(true);
                    CommandOutcome::ok_with_result(
                        Some(format!("set bounds for window {}", window.title)),
                        serde_json::json!({ "window": window_payload }),
                    )
                }
                Err(message) => CommandOutcome::error("window-bounds-failed", message),
            },
        )
    }

    pub(crate) fn input_window(
        &mut self,
        id: &str,
        event: &WindowInputEvent,
    ) -> Option<CommandOutcome> {
        let DesktopControllerMode::Real {
            window_tool: Some(window_tool),
            input_tool,
            ..
        } = &self.mode
        else {
            return None;
        };
        let windows = match read_wmctrl_windows(window_tool) {
            Ok(windows) => windows,
            Err(message) if message == "wmctrl reported no desktop windows" => return None,
            Err(message) => return Some(CommandOutcome::error("windows-list-failed", message)),
        };
        let Some(window) = windows.into_iter().find(|window| window.id == id) else {
            return Some(CommandOutcome::error(
                "window-not-found",
                format!("window {id} was not found"),
            ));
        };
        let Some(input_tool) = input_tool else {
            return Some(CommandOutcome::error(
                "window-input-unsupported",
                "xdotool is not available for guest window input",
            ));
        };

        if let Err(message) = run_command_status(window_tool, &["-ia", &window.id]) {
            return Some(CommandOutcome::error("window-input-focus-failed", message));
        }

        Some(match run_xdotool_window_input(input_tool, event) {
            Ok(()) => {
                let mut window_payload = desktop_window_payload(&window);
                window_payload["input"] = window_input_payload(event, "xdotool");
                CommandOutcome::ok_with_result(
                    Some(format!(
                        "sent {} input to window {}",
                        window_input_label(event),
                        window.title
                    )),
                    serde_json::json!({ "window": window_payload }),
                )
            }
            Err(message) => CommandOutcome::error("window-input-failed", message),
        })
    }

    pub(crate) fn run_wmctrl_window_action(
        &mut self,
        id: &str,
        flag: &str,
        verb: &str,
        error_code: &str,
    ) -> Option<CommandOutcome> {
        let DesktopControllerMode::Real {
            window_tool: Some(window_tool),
            ..
        } = &self.mode
        else {
            return None;
        };
        let windows = match read_wmctrl_windows(window_tool) {
            Ok(windows) => windows,
            Err(message) if message == "wmctrl reported no desktop windows" => return None,
            Err(message) => return Some(CommandOutcome::error("windows-list-failed", message)),
        };
        let Some(window) = windows.into_iter().find(|window| window.id == id) else {
            return Some(CommandOutcome::error(
                "window-not-found",
                format!("window {id} was not found"),
            ));
        };
        Some(match run_command_status(window_tool, &[flag, &window.id]) {
            Ok(()) => {
                let mut window_payload = desktop_window_payload(&window);
                window_payload[verb] = serde_json::Value::Bool(true);
                CommandOutcome::ok_with_result(
                    Some(format!("{verb} window {}", window.title)),
                    serde_json::json!({ "window": window_payload }),
                )
            }
            Err(message) => CommandOutcome::error(error_code, message),
        })
    }

    #[cfg(test)]
    pub(crate) fn is_real_for_test(&self) -> bool {
        matches!(self.mode, DesktopControllerMode::Real { .. })
    }
}

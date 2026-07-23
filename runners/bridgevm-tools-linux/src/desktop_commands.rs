//! Application and window command handlers that delegate to the desktop controller.

use crate::*;
use bridgevm_agent_protocol::WindowInputEvent;

impl GuestToolsState {
    pub(crate) fn list_applications(&self) -> CommandOutcome {
        if !self.applications_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "applications capability is not enabled",
            );
        }

        if let Some(outcome) = self.desktop_controller.list_applications() {
            return outcome;
        }

        let names = self
            .applications
            .iter()
            .map(|(id, app)| format!("{id}:{}", app.name))
            .collect::<Vec<_>>()
            .join(",");
        let applications = self
            .applications
            .iter()
            .map(|(id, app)| {
                serde_json::json!({
                    "id": id,
                    "name": app.name,
                    "launched": app.launched
                })
            })
            .collect::<Vec<_>>();
        CommandOutcome::ok_with_result(
            Some(format!("applications: {names}")),
            serde_json::json!({ "applications": applications }),
        )
    }

    pub(crate) fn launch_application(&mut self, id: &str) -> CommandOutcome {
        if !self.applications_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "applications capability is not enabled",
            );
        }
        if let Some(outcome) = self.desktop_controller.launch_application(id) {
            return outcome;
        }
        let Some(app) = self.applications.get_mut(id) else {
            return CommandOutcome::error(
                "application-not-found",
                format!("application {id} was not found"),
            );
        };

        app.launched = true;
        CommandOutcome::ok_with_result(
            Some(format!(
                "accepted launch request for application {}",
                app.name
            )),
            serde_json::json!({
                "application": {
                    "id": id,
                    "name": app.name,
                    "launched": app.launched
                }
            }),
        )
    }

    pub(crate) fn list_windows(&self) -> CommandOutcome {
        if !self.windows_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "windows capability is not enabled",
            );
        }

        if let Some(outcome) = self.desktop_controller.list_windows() {
            return outcome;
        }

        let windows = self
            .windows
            .iter()
            .filter(|(_, window)| !window.closed)
            .map(|(id, window)| format!("{id}:{}", window.title))
            .collect::<Vec<_>>()
            .join(",");
        let window_payload = self
            .windows
            .iter()
            .filter(|(_, window)| !window.closed)
            .map(|(id, window)| {
                let mut payload = window_entry_payload(id, window);
                payload["focused"] = serde_json::Value::Bool(window.focused);
                payload
            })
            .collect::<Vec<_>>();
        CommandOutcome::ok_with_result(
            Some(format!("windows: {windows}")),
            serde_json::json!({ "windows": window_payload }),
        )
    }

    pub(crate) fn focus_window(&mut self, id: &str) -> CommandOutcome {
        if !self.windows_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "windows capability is not enabled",
            );
        }
        if let Some(outcome) = self.desktop_controller.focus_window(id) {
            return outcome;
        }
        if self.windows.get(id).is_none_or(|window| window.closed) {
            return CommandOutcome::error("window-not-found", format!("window {id} was not found"));
        }

        for window in self.windows.values_mut() {
            window.focused = false;
        }
        let window = self.windows.get_mut(id).expect("window checked above");
        window.focused = true;
        let mut window_payload = window_entry_payload(id, window);
        window_payload["focused"] = serde_json::Value::Bool(window.focused);
        CommandOutcome::ok_with_result(
            Some(format!(
                "accepted focus request for window {}",
                window.title
            )),
            serde_json::json!({ "window": window_payload }),
        )
    }

    pub(crate) fn close_window(&mut self, id: &str) -> CommandOutcome {
        if !self.windows_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "windows capability is not enabled",
            );
        }
        if let Some(outcome) = self.desktop_controller.close_window(id) {
            return outcome;
        }
        let Some(window) = self.windows.get_mut(id) else {
            return CommandOutcome::error("window-not-found", format!("window {id} was not found"));
        };
        if window.closed {
            return CommandOutcome::error("window-not-found", format!("window {id} was not found"));
        }

        window.closed = true;
        window.focused = false;
        let mut window_payload = window_entry_payload(id, window);
        window_payload["closed"] = serde_json::Value::Bool(window.closed);
        CommandOutcome::ok_with_result(
            Some(format!("closed window {}", window.title)),
            serde_json::json!({ "window": window_payload }),
        )
    }

    pub(crate) fn set_window_bounds(
        &mut self,
        id: &str,
        x: i64,
        y: i64,
        width: u64,
        height: u64,
    ) -> CommandOutcome {
        if !self.windows_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "windows capability is not enabled",
            );
        }
        if let Some(outcome) = self
            .desktop_controller
            .set_window_bounds(id, x, y, width, height)
        {
            return outcome;
        }
        let Some(window) = self.windows.get_mut(id) else {
            return CommandOutcome::error("window-not-found", format!("window {id} was not found"));
        };
        if window.closed {
            return CommandOutcome::error("window-not-found", format!("window {id} was not found"));
        }

        window.bounds = Some(DesktopWindowBounds {
            x,
            y,
            width,
            height,
        });
        let mut window_payload = window_entry_payload(id, window);
        window_payload["bounds_changed"] = serde_json::Value::Bool(true);
        CommandOutcome::ok_with_result(
            Some(format!("set bounds for window {}", window.title)),
            serde_json::json!({ "window": window_payload }),
        )
    }

    pub(crate) fn window_input(&mut self, id: &str, event: &WindowInputEvent) -> CommandOutcome {
        if !self.windows_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "windows capability is not enabled",
            );
        }
        if let Some(outcome) = self.desktop_controller.input_window(id, event) {
            return outcome;
        }
        let Some(window) = self.windows.get(id) else {
            return CommandOutcome::error("window-not-found", format!("window {id} was not found"));
        };
        if window.closed {
            return CommandOutcome::error("window-not-found", format!("window {id} was not found"));
        }

        let mut window_payload = window_entry_payload(id, window);
        window_payload["focused"] = serde_json::Value::Bool(window.focused);
        window_payload["input"] = window_input_payload(event, "scaffold");
        CommandOutcome::ok_with_result(
            Some(format!(
                "accepted {} input for window {}",
                window_input_label(event),
                window.title
            )),
            serde_json::json!({ "window": window_payload }),
        )
    }
}

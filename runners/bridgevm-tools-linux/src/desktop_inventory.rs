//! Enumerating real .desktop applications and wmctrl windows, and their payload shapes.

use crate::*;
use anyhow::Result;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

pub(crate) const MAX_DESKTOP_FILE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DesktopApplication {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DesktopWindow {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) desktop: Option<i64>,
    pub(crate) pid: Option<u32>,
    pub(crate) bounds: Option<DesktopWindowBounds>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DesktopWindowBounds {
    pub(crate) x: i64,
    pub(crate) y: i64,
    pub(crate) width: u64,
    pub(crate) height: u64,
}

pub(crate) fn window_bounds_payload(x: i64, y: i64, width: u64, height: u64) -> serde_json::Value {
    serde_json::json!({
        "x": x,
        "y": y,
        "width": width,
        "height": height
    })
}

pub(crate) fn desktop_window_payload(window: &DesktopWindow) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "id": window.id,
        "title": window.title,
        "source": "wmctrl"
    });
    if let Some(desktop) = window.desktop {
        payload["desktop"] = serde_json::json!(desktop);
    }
    if let Some(pid) = window.pid {
        payload["pid"] = serde_json::json!(pid);
    }
    if let Some(bounds) = &window.bounds {
        payload["bounds"] = window_bounds_payload(bounds.x, bounds.y, bounds.width, bounds.height);
    }
    payload
}

pub(crate) fn read_desktop_applications() -> Result<Vec<DesktopApplication>, String> {
    let mut dirs = vec![
        PathBuf::from("/usr/local/share/applications"),
        PathBuf::from("/usr/share/applications"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/applications"));
    }

    let mut apps = BTreeMap::<String, DesktopApplication>::new();
    for dir in dirs {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("desktop") {
                continue;
            }
            let Some(app) = parse_desktop_application(&path) else {
                continue;
            };
            apps.entry(app.id.clone()).or_insert(app);
        }
    }

    if apps.is_empty() {
        return Err("no visible .desktop applications were found".to_string());
    }
    Ok(apps.into_values().collect())
}

pub(crate) fn parse_desktop_application(path: &Path) -> Option<DesktopApplication> {
    let contents = read_utf8_file_bounded(path, MAX_DESKTOP_FILE_BYTES).ok()?;
    let mut name = None;
    let mut app_type = None;
    let mut no_display = false;
    let mut hidden = false;
    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "Name" => name = Some(value.trim().to_string()),
            "Type" => app_type = Some(value.trim().to_string()),
            "NoDisplay" => no_display = value.trim().eq_ignore_ascii_case("true"),
            "Hidden" => hidden = value.trim().eq_ignore_ascii_case("true"),
            _ => {}
        }
    }
    if app_type.as_deref() != Some("Application") || no_display || hidden {
        return None;
    }
    let id = path.file_name()?.to_string_lossy().to_string();
    let name = name.filter(|value| !value.is_empty())?;
    Some(DesktopApplication {
        id,
        name,
        path: path.to_path_buf(),
    })
}

pub(crate) fn run_application_launcher(
    launcher: &AppLauncher,
    app: &DesktopApplication,
) -> Result<(), String> {
    match launcher {
        AppLauncher::Gio(program) => {
            let path = app.path.to_string_lossy().to_string();
            run_command_status(program, &["launch", &path])
        }
        AppLauncher::GtkLaunch(program) => run_command_status(program, &[&app.id]),
    }
}

pub(crate) fn read_wmctrl_windows(program: &Path) -> Result<Vec<DesktopWindow>, String> {
    match run_command_output(program, &["-l", "-p", "-G"]) {
        Ok(output) => parse_wmctrl_windows(&output, true).or_else(|_| {
            let fallback = run_command_output(program, &["-l"])?;
            parse_wmctrl_windows(&fallback, false)
        }),
        Err(enhanced_error) => {
            let fallback = run_command_output(program, &["-l"]).map_err(|fallback_error| {
                format!("{enhanced_error}; fallback -l also failed: {fallback_error}")
            })?;
            parse_wmctrl_windows(&fallback, false)
        }
    }
}

pub(crate) fn parse_wmctrl_windows(
    output: &str,
    enhanced: bool,
) -> Result<Vec<DesktopWindow>, String> {
    let windows = output
        .lines()
        .filter_map(|line| {
            if enhanced {
                parse_wmctrl_window_enhanced(line)
            } else {
                parse_wmctrl_window_basic(line)
            }
        })
        .collect::<Vec<_>>();
    if windows.is_empty() {
        return Err("wmctrl reported no desktop windows".to_string());
    }
    Ok(windows)
}

pub(crate) fn parse_wmctrl_window_enhanced(line: &str) -> Option<DesktopWindow> {
    let mut parts = line.split_whitespace();
    let id = parts.next()?.to_string();
    let desktop = parts.next()?.parse::<i64>().ok()?;
    let pid = parts.next()?.parse::<u32>().ok()?;
    let x = parts.next()?.parse::<i64>().ok()?;
    let y = parts.next()?.parse::<i64>().ok()?;
    let width = parts.next()?.parse::<u64>().ok()?;
    let height = parts.next()?.parse::<u64>().ok()?;
    let _host = parts.next()?;
    let title = parts.collect::<Vec<_>>().join(" ");
    if id.is_empty() || title.is_empty() {
        return None;
    }
    Some(DesktopWindow {
        id,
        title,
        desktop: Some(desktop),
        pid: Some(pid),
        bounds: Some(DesktopWindowBounds {
            x,
            y,
            width,
            height,
        }),
    })
}

pub(crate) fn parse_wmctrl_window_basic(line: &str) -> Option<DesktopWindow> {
    let mut parts = line.split_whitespace();
    let id = parts.next()?.to_string();
    let desktop = parts.next()?.parse::<i64>().ok();
    let _host = parts.next()?;
    let title = parts.collect::<Vec<_>>().join(" ");
    if id.is_empty() || title.is_empty() {
        return None;
    }
    Some(DesktopWindow {
        id,
        title,
        desktop,
        pid: None,
        bounds: None,
    })
}

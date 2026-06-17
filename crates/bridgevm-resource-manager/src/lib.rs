use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceProfile {
    Automatic,
    BatterySaver,
    Performance,
    Developer,
    Office,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceDecision {
    pub vcpu: String,
    pub memory: String,
    pub display_fps_cap: String,
    pub rationale: String,
}

impl ResourceProfile {
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "battery-saver" | "battery_saver" | "battery" => Self::BatterySaver,
            "performance" => Self::Performance,
            "developer" | "dev" => Self::Developer,
            "office" => Self::Office,
            _ => Self::Automatic,
        }
    }
}

pub fn decide(profile: ResourceProfile, foreground: bool, on_battery: bool) -> ResourceDecision {
    match (profile, foreground, on_battery) {
        (ResourceProfile::Performance, true, _) => ResourceDecision {
            vcpu: "4".to_string(),
            memory: "6144".to_string(),
            display_fps_cap: "60".to_string(),
            rationale: "Foreground performance profile.".to_string(),
        },
        (ResourceProfile::Developer, _, _) => ResourceDecision {
            vcpu: "4".to_string(),
            memory: "4096".to_string(),
            display_fps_cap: "adaptive".to_string(),
            rationale: "Developer profile keeps CPU headroom for builds and networking."
                .to_string(),
        },
        (ResourceProfile::Office, true, _) => ResourceDecision {
            vcpu: "2".to_string(),
            memory: "4096".to_string(),
            display_fps_cap: "30".to_string(),
            rationale: "Office profile favors smooth interactive use.".to_string(),
        },
        (_, false, true) | (ResourceProfile::BatterySaver, _, _) => ResourceDecision {
            vcpu: "1".to_string(),
            memory: "2048".to_string(),
            display_fps_cap: "10".to_string(),
            rationale: "Battery or background throttling active.".to_string(),
        },
        _ => ResourceDecision {
            vcpu: "2".to_string(),
            memory: "4096".to_string(),
            display_fps_cap: "adaptive".to_string(),
            rationale: "Automatic balanced policy.".to_string(),
        },
    }
}

pub fn decide_from_manifest_profile(profile: &str) -> ResourceDecision {
    decide(ResourceProfile::parse(profile), true, false)
}

/// Resource decision for a launch that accounts for the host power state.
///
/// Policy: on battery, profiles that mean "let the app decide" (Automatic,
/// Office, BatterySaver) step down to conserve power; profiles where the user
/// asked for headroom (Performance, Developer) keep their level. This only ever
/// affects `auto` memory/cpu — explicit per-VM values are preserved by
/// [`resolve_memory`]/[`resolve_vcpu`] regardless of this decision.
pub fn decide_for_launch(profile: ResourceProfile, on_battery: bool) -> ResourceDecision {
    if !on_battery {
        return decide(profile, true, false);
    }
    match profile {
        ResourceProfile::Performance | ResourceProfile::Developer => decide(profile, true, true),
        _ => decide(ResourceProfile::BatterySaver, true, true),
    }
}

/// Power-aware variant of [`decide_from_manifest_profile`].
pub fn decide_from_manifest_profile_with_power(
    profile: &str,
    on_battery: bool,
) -> ResourceDecision {
    decide_for_launch(ResourceProfile::parse(profile), on_battery)
}

/// Parse `pmset -g batt` output to decide whether the host is on battery.
/// `pmset` reports the drawing source on the first line, e.g.
/// `Now drawing from 'Battery Power'` vs `'AC Power'`.
pub fn parse_pmset_battery_state(output: &str) -> bool {
    output
        .lines()
        .find(|line| line.contains("Now drawing from"))
        .map(|line| line.to_ascii_lowercase().contains("battery"))
        .unwrap_or(false)
}

/// Read the host power state. Honors `BRIDGEVM_FORCE_ON_BATTERY` (`1`/`0`) for
/// tests and demos; otherwise shells out to `pmset -g batt` (macOS) with a hard
/// timeout. Defaults to "on AC" (false) when the state can't be determined — a
/// missing tool, a non-zero exit, OR a hang — so it can never wedge or
/// needlessly throttle a VM launch (this runs on the cold-start hot path).
pub fn read_on_battery() -> bool {
    if let Ok(forced) = std::env::var("BRIDGEVM_FORCE_ON_BATTERY") {
        return forced == "1" || forced.eq_ignore_ascii_case("true");
    }
    pmset_battery_output(std::time::Duration::from_secs(2))
        .map(|out| parse_pmset_battery_state(&out))
        .unwrap_or(false)
}

/// Run `pmset -g batt` and return its stdout, killing it (and returning `None`)
/// if it does not finish within `timeout`. `pmset` is normally instant, but an
/// unbounded external command on the launch path is a liability if `powerd`/
/// IORegistry ever wedges.
fn pmset_battery_output(timeout: std::time::Duration) -> Option<String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut child = Command::new("pmset")
        .args(["-g", "batt"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                break;
            }
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(_) => return None,
        }
    }
    // pmset's output is small (well under the pipe buffer), so reading after exit
    // cannot have deadlocked it.
    let mut out = String::new();
    child.stdout.take()?.read_to_string(&mut out).ok()?;
    Some(out)
}

pub fn resolve_memory(manifest_memory: &str, decision: &ResourceDecision) -> String {
    if manifest_memory == "auto" {
        decision.memory.clone()
    } else {
        manifest_memory.to_string()
    }
}

pub fn resolve_vcpu(manifest_cpu: &str, decision: &ResourceDecision) -> String {
    if manifest_cpu == "auto" {
        decision.vcpu.clone()
    } else {
        manifest_cpu.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_profiles_and_decides_resources() {
        assert_eq!(
            ResourceProfile::parse("battery-saver"),
            ResourceProfile::BatterySaver
        );
        assert_eq!(
            ResourceProfile::parse("performance"),
            ResourceProfile::Performance
        );
        assert_eq!(
            ResourceProfile::parse("unknown"),
            ResourceProfile::Automatic
        );

        let automatic = decide(ResourceProfile::Automatic, true, false);
        assert_eq!(automatic.memory, "4096");
        assert_eq!(automatic.vcpu, "2");

        let performance = decide(ResourceProfile::Performance, true, false);
        assert_eq!(performance.memory, "6144");
        assert_eq!(performance.vcpu, "4");

        let battery = decide(ResourceProfile::BatterySaver, true, false);
        assert_eq!(battery.memory, "2048");
        assert_eq!(battery.vcpu, "1");
        assert_eq!(battery.display_fps_cap, "10");
    }

    #[test]
    fn resolves_auto_but_preserves_manual_values() {
        let decision = decide(ResourceProfile::Performance, true, false);
        assert_eq!(resolve_memory("auto", &decision), "6144");
        assert_eq!(resolve_vcpu("auto", &decision), "4");
        assert_eq!(resolve_memory("8192", &decision), "8192");
        assert_eq!(resolve_vcpu("6", &decision), "6");
    }

    #[test]
    fn parses_pmset_battery_state() {
        assert!(parse_pmset_battery_state(
            "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=...) 82%; discharging"
        ));
        assert!(!parse_pmset_battery_state(
            "Now drawing from 'AC Power'\n -InternalBattery-0 (id=...) 100%; charged"
        ));
        // Desktop Macs print no battery line at all -> treated as on AC.
        assert!(!parse_pmset_battery_state("No batteries available"));
    }

    #[test]
    fn launch_decision_steps_down_auto_profiles_on_battery() {
        // On battery, an "auto" Automatic/Office profile conserves power...
        let auto_batt = decide_for_launch(ResourceProfile::Automatic, true);
        assert_eq!(auto_batt.memory, "2048");
        assert_eq!(auto_batt.vcpu, "1");
        let office_batt = decide_for_launch(ResourceProfile::Office, true);
        assert_eq!(office_batt.memory, "2048");
        // ...but a Performance profile keeps its headroom even on battery.
        let perf_batt = decide_for_launch(ResourceProfile::Performance, true);
        assert_eq!(perf_batt.memory, "6144");
        // On AC, nothing is throttled.
        let auto_ac = decide_for_launch(ResourceProfile::Automatic, false);
        assert_eq!(auto_ac.memory, "4096");
    }

    #[test]
    fn power_aware_resolution_only_affects_auto_values() {
        // On battery, "auto" steps down but an explicit value is untouched.
        let decision = decide_from_manifest_profile_with_power("automatic", true);
        assert_eq!(resolve_memory("auto", &decision), "2048");
        assert_eq!(resolve_memory("8192", &decision), "8192");
    }
}

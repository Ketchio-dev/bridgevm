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
}

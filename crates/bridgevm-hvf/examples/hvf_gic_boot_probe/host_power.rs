//! The Mac's battery and power adapter for the guest's ACPI battery
//! (`bridgevm_hvf::host_power`).
//!
//! Read once before the VM is built. A Mac without an internal battery (Mac
//! mini, Studio, iMac; a UPS does not count) gets no battery device and an
//! unchanged DSDT. Otherwise a thread off the vCPUs re-reads it when macOS
//! posts a power-source change (IOPSNotificationCreateRunLoopSource) and at
//! least every 30 s, hands a changed reading to the platform, which raises the
//! GED event, and asks vCPU0 to leave `hv_vcpu_run` so its drain delivers the
//! interrupt now instead of at its next exit. A failed read keeps the last one.
//!
//! `BRIDGEVM_HVF_FAKE_BATTERY=percent,charging,ac` (`80,0,0`: 80 %, not
//! charging, on battery) replaces the Mac's reading, for tests on a Mac
//! without a battery. A value starting with `/` names a file holding that
//! triple, re-read every 5 s, so a test can change it while Windows runs.
//!
//! Windows sees AC power until the guest agent's READY says `onbattery`
//! (`for_guest`): Task Scheduler does not start an older agent's task on
//! battery, and the update that fixes the task comes through that agent.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use bridgevm_hvf::host_power::{HostBattery, HostPower};
use bridgevm_hvf::platform_virt::VirtPlatform;

use crate::{hv_vcpus_exit, parse_flag, HvVcpuT};

#[path = "host_power_mac.rs"]
mod mac;

pub(crate) const FAKE_ENV: &str = "BRIDGEVM_HVF_FAKE_BATTERY";
/// The fallback re-read of the Mac when no change notification arrives.
const MAC_POLL: Duration = Duration::from_secs(30);
const FAKE_FILE_POLL: Duration = Duration::from_secs(5);
/// macOS posts several notifications while an unplug settles: read once after.
const SETTLE: Duration = Duration::from_secs(1);
/// How often the agent's word is looked at while Windows is kept on AC.
const HELD_POLL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PowerSource {
    Mac,
    Fixed(HostPower),
    File(PathBuf),
}

impl PowerSource {
    pub(crate) fn from_env() -> Self {
        Self::from_value(std::env::var(FAKE_ENV).ok().as_deref())
    }

    fn from_value(value: Option<&str>) -> Self {
        let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
            return Self::Mac;
        };
        if value.starts_with('/') {
            return Self::File(PathBuf::from(value));
        }
        match parse_fake(value) {
            Some(power) => Self::Fixed(power),
            None => {
                eprintln!("{FAKE_ENV}='{value}' is not percent,charging,ac (like 80,0,0); reading the Mac instead");
                Self::Mac
            }
        }
    }

    pub(crate) fn read(&self) -> Option<HostPower> {
        match self {
            Self::Mac => mac::read(),
            Self::Fixed(power) => Some(*power),
            Self::File(path) => read_fake_file(path),
        }
    }

    fn label(&self) -> String {
        match self {
            Self::Mac => "the Mac".into(),
            Self::Fixed(_) => FAKE_ENV.into(),
            Self::File(path) => format!("{FAKE_ENV} file {}", path.display()),
        }
    }
}

/// `percent,charging,ac`, with 0/1 (or true/false, yes/no, on/off) flags.
pub(crate) fn parse_fake(text: &str) -> Option<HostPower> {
    let mut parts = text.trim().split(',').map(str::trim);
    let percent = parts.next()?.parse::<u8>().ok().filter(|p| *p <= 100)?;
    let charging = parse_flag(parts.next()?)?;
    let ac_online = parse_flag(parts.next()?)?;
    if parts.next().is_some() {
        return None;
    }
    Some(HostPower {
        ac_online,
        battery: Some(HostBattery {
            percent,
            charging,
            minutes_to_empty: None,
            minutes_to_full: None,
        }),
    })
}

fn read_fake_file(path: &Path) -> Option<HostPower> {
    use std::io::Read as _;
    let mut text = String::new();
    std::fs::File::open(path)
        .ok()?
        .take(256)
        .read_to_string(&mut text)
        .ok()?;
    parse_fake(&text)
}

/// What the VM starts with when the feature is on (the Mac, or the fake, has
/// a battery): `for_guest` of the reading, as no agent has spoken yet. One
/// line says what Windows gets.
pub(crate) fn initial_reading(source: &PowerSource) -> Option<HostPower> {
    let reading = source.read();
    match reading {
        Some(power) if power.battery.is_some() => println!(
            "host power: battery from {} ({}); Windows gets a battery (BAT0) and AC adapter (ADP0){}",
            source.label(),
            describe(&power),
            if power.ac_online {
                ""
            } else {
                ", on AC power until the guest agent's task survives battery"
            }
        ),
        Some(_) => println!(
            "host power: no battery from {}; Windows gets no battery or AC adapter",
            source.label()
        ),
        None => println!(
            "host power: could not read {}; Windows gets no battery this boot",
            source.label()
        ),
    }
    reading
        .filter(|power| power.battery.is_some())
        .map(|power| for_guest(power, false))
}

pub(crate) fn describe(power: &HostPower) -> String {
    let ac = if power.ac_online {
        "on AC"
    } else {
        "on battery"
    };
    match power.battery {
        Some(battery) => format!(
            "{}% {} {ac} to_empty={} to_full={}",
            battery.percent,
            if battery.charging {
                "charging"
            } else {
                "not charging"
            },
            minutes(battery.minutes_to_empty),
            minutes(battery.minutes_to_full),
        ),
        None => format!("no battery {ac}"),
    }
}

fn minutes(value: Option<u32>) -> String {
    value.map_or_else(|| "?".into(), |m| format!("{m}min"))
}

/// What Windows gets. Until the running guest agent's task survives battery
/// (`agent_survives_battery`, from its READY), a Mac on battery is shown on
/// AC and not charging: an older agent's task keeps Task Scheduler's
/// defaults, which do not start it on battery and stop it when Windows goes
/// on battery. The percentage is still the Mac's.
pub(crate) fn for_guest(power: HostPower, agent_survives_battery: bool) -> HostPower {
    if agent_survives_battery || power.ac_online {
        return power;
    }
    HostPower {
        ac_online: true,
        battery: power.battery.map(|battery| HostBattery {
            charging: false,
            minutes_to_empty: None,
            minutes_to_full: None,
            ..battery
        }),
    }
}

/// Follow the source and the agent's word on its own thread for the probe's
/// lifetime. It holds only a weak platform and ends once the platform is gone.
/// `shown` is what the platform was built with: `for_guest(reading, false)`.
pub(crate) fn start_monitor(
    source: PowerSource,
    shown: HostPower,
    agent_survives_battery: Arc<AtomicBool>,
    platform: Weak<Mutex<VirtPlatform>>,
    vcpu: HvVcpuT,
) {
    let spawned = std::thread::Builder::new()
        .name("host-power".into())
        .spawn(move || monitor(source, shown, agent_survives_battery, platform, vcpu));
    if let Err(error) = spawned {
        eprintln!("host power: cannot follow the Mac's battery ({error}); Windows keeps the first reading");
    }
}

fn monitor(
    source: PowerSource,
    mut shown: HostPower,
    agent_survives_battery: Arc<AtomicBool>,
    platform: Weak<Mutex<VirtPlatform>>,
    vcpu: HvVcpuT,
) {
    let changes = matches!(source, PowerSource::Mac).then(mac::ChangeWait::install);
    let mut reading = source.read().unwrap_or(shown);
    let mut real = false;
    let mut read_at = Instant::now();
    loop {
        // Kept on AC with the Mac on battery: look at the agent's word sooner.
        let poll = if changes.is_some() {
            MAC_POLL
        } else {
            FAKE_FILE_POLL
        };
        let wait = if !real && !reading.ac_online {
            HELD_POLL
        } else {
            poll
        };
        let changed = match &changes {
            Some(changes) => changes.wait(wait),
            None => {
                std::thread::sleep(wait);
                true
            }
        };
        if changed && changes.is_some() {
            std::thread::sleep(SETTLE);
        }
        if changed || read_at.elapsed() >= poll {
            // A failed read keeps the last one.
            reading = source.read().unwrap_or(reading);
            read_at = Instant::now();
        }
        if real != agent_survives_battery.load(Ordering::Relaxed) {
            real = !real;
            println!(
                "{}",
                if real {
                    "host power: real state from now on (agent survives battery)"
                } else {
                    "host power: Windows sees AC power until the guest agent's task survives battery"
                }
            );
        }
        let power = for_guest(reading, real);
        if power == shown {
            continue;
        }
        let Some(platform) = platform.upgrade() else {
            return;
        };
        let Ok(mut locked) = platform.lock() else {
            return;
        };
        locked.set_host_power(power);
        drop(locked);
        drop(platform);
        shown = power;
        println!(
            "BVBATTERY host {} (Windows: {})",
            describe(&reading),
            describe(&power)
        );
        // The GED edge is queued; vCPU0's pre-run drain delivers it.
        unsafe { hv_vcpus_exit(&vcpu, 1) };
    }
}

#[cfg(test)]
#[path = "host_power_tests.rs"]
mod tests;

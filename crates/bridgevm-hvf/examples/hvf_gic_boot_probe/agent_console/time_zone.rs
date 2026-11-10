//! The Mac's time zone for Windows: its IANA name from `/etc/localtime`,
//! mapped to a Windows time zone ID through the app's own table
//! (apps/macos/.../WindowsTimeZones.swift, generated from CLDR by
//! scripts/generate-windows-time-zones.py), so both use one mapping.
//!
//! New installs take the Mac's zone once, from the answer file; after that
//! Windows kept it, so after a trip the guest showed the old zone's time.
//! The clock resync (time_resync.rs) sets this zone first at every boot and
//! when the Mac's zone changes. The mapped ID comes from the app's table,
//! never from the guest.

use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// How often a running VM looks at the Mac's zone (one readlink).
pub(super) const ZONE_CHECK: Duration = Duration::from_secs(60);

const TABLE: &str = include_str!("../../../../../apps/macos/Sources/BridgeVMControl/WindowsTimeZones.swift");

/// `(Windows ID, space-separated IANA names)` rows of the app's table.
fn rows() -> &'static [(&'static str, &'static str)] {
    static ROWS: OnceLock<Vec<(&'static str, &'static str)>> = OnceLock::new();
    ROWS.get_or_init(|| {
        TABLE
            .lines()
            .filter_map(|line| {
                let row = line.trim().strip_prefix("(\"")?.strip_suffix("\"),")?;
                row.split_once("\", \"")
            })
            .collect()
    })
}

/// The Windows time zone ID for an IANA name, the first row naming it.
pub(super) fn windows_zone(iana: &str) -> Option<&'static str> {
    rows()
        .iter()
        .find(|(_, names)| names.split(' ').any(|name| name == iana))
        .map(|(windows, _)| *windows)
}

/// The IANA name `/etc/localtime` points at (`.../zoneinfo/Asia/Seoul`).
pub(super) fn iana_from_localtime(link: &Path) -> Option<String> {
    let target = std::fs::read_link(link).ok()?;
    let target = target.to_str()?;
    let at = target.find("zoneinfo/")? + "zoneinfo/".len();
    Some(target[at..].to_string())
}

/// The Mac's zone as a Windows ID; `None` when unknown, and Windows' zone is
/// then left alone.
pub(super) fn host_windows_zone() -> Option<&'static str> {
    windows_zone(&iana_from_localtime(Path::new("/etc/localtime"))?)
}

/// What the resync has set and when the Mac's zone was last read.
#[derive(Default)]
pub(super) struct ZoneWatch {
    /// The zone the last resync command carried.
    pub(super) applied: Option<&'static str>,
    /// The Mac's zone moved away from `applied`: a resync is due.
    pub(super) changed: bool,
    checked: Option<Instant>,
    /// Tests: the Mac's zone, instead of /etc/localtime (unset: unknown).
    #[cfg(test)]
    pub(super) host: Option<Option<&'static str>>,
}

impl ZoneWatch {
    pub(super) fn host_zone(&self) -> Option<&'static str> {
        // Tests never read the Mac's own zone.
        #[cfg(test)]
        return self.host.flatten();
        #[cfg(not(test))]
        host_windows_zone()
    }

    /// At most once per `ZONE_CHECK`: after a resync carried a zone, a Mac
    /// zone that differs from it marks a resync due.
    pub(super) fn tick(&mut self, now: Instant) {
        if self.checked.is_some_and(|at| now.saturating_duration_since(at) < ZONE_CHECK) {
            return;
        }
        self.checked = Some(now);
        if let (Some(applied), Some(host)) = (self.applied, self.host_zone()) {
            self.changed |= host != applied;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_apps_table_maps_mac_zones_to_windows_ids() {
        assert!(rows().len() > 100, "rows parsed from WindowsTimeZones.swift: {}", rows().len());
        assert_eq!(windows_zone("Asia/Seoul"), Some("Korea Standard Time"));
        assert_eq!(windows_zone("America/Toronto"), Some("Eastern Standard Time"));
        assert_eq!(windows_zone("Asia/Kolkata"), Some("India Standard Time"));
        assert_eq!(windows_zone("Europe/London"), Some("GMT Standard Time"));
        assert_eq!(windows_zone("Not/AZone"), None);
        assert_eq!(windows_zone("Asia"), None);
        // Every ID is quoted inside the guest command: no quote can end it.
        for (windows, _) in rows() {
            assert!(windows.chars().all(|c| c.is_ascii_alphanumeric() || " .+-()".contains(c)), "{windows}");
        }
    }

    #[test]
    fn the_iana_name_is_read_from_the_localtime_link() {
        let dir = std::env::temp_dir().join(format!("bridgevm-tz-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let link = dir.join("localtime");
        std::os::unix::fs::symlink("/var/db/timezone/zoneinfo/Asia/Seoul", &link).unwrap();
        assert_eq!(iana_from_localtime(&link).as_deref(), Some("Asia/Seoul"));
        std::fs::write(dir.join("plain"), b"TZif").unwrap();
        assert_eq!(iana_from_localtime(&dir.join("plain")), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

//! Guest-initiated writes/deletes use pinned, no-follow directory descriptors.
//!
//! - Resident GET completion is bound to both its active request and wire path.
//! - Guest file adoption starts only after the selected destination root is verified.
//! - Guest deletion may mutate only a verified, held shared-root destination.

use super::host_destination::Destination;
use super::*;

/// Part of the host pass's setup (share_idle.rs).
pub(in crate::agent_console) fn forget_absent_share_failures(share: &mut ShareState) {
    (share.engine.for_pass()).forget_absent_failed(|name| Destination::absent(&share.host_dir, name));
}

impl AgentConsoleHarness {
    pub(in crate::agent_console) fn handle_share_get_reply(
        &mut self,
        name: &str,
        line: &str,
        now: Instant,
    ) {
        if let Some(rest) = line.strip_prefix("GETEND ") {
            if self.handle_share_get_end(name, rest, now) {
                self.complete_in_flight();
            }
        } else if line.starts_with("ERR") {
            self.get_accum = None;
            println!("BVAGENT SHARE get-error {name} {line}");
            self.complete_in_flight();
        }
    }

    pub(in crate::agent_console) fn handle_share_get_end(
        &mut self,
        name: &str,
        _rest: &str,
        now: Instant,
    ) -> bool {
        if !matches!(
            self.in_flight.as_ref(),
            Some((ServiceReq::ShareGet { name: active }, _)) if active == name
        ) {
            return false;
        }
        let Some(share) = self.share.as_ref() else {
            return false;
        };
        write_share_guest_path_into(share, name, &mut self.share_guest_path_scratch);
        if !self
            .get_accum
            .as_ref()
            .is_some_and(|accum| accum.path == self.share_guest_path_scratch)
        {
            return false;
        }
        let Some(finished) = self.take_finished_get() else {
            return false;
        };
        // Part of a file whose large copy could not start: nothing was kept, nothing is written.
        if finished.range.as_ref().is_some_and(|range| !range.whole(finished.total)) {
            return true;
        }
        self.apply_finished_share_get(name, finished, now);
        true
    }

    pub(in crate::agent_console) fn apply_finished_share_get(
        &mut self,
        name: &str,
        finished: FinishedGet,
        now: Instant,
    ) {
        if finished.bytes.len() != finished.total {
            println!(
                "BVAGENT SHARE get-short {name} got={} expected={}",
                finished.bytes.len(),
                finished.total
            );
            return;
        }
        let Some(share) = self.share.as_mut() else {
            return;
        };
        let root = match share.checked_root() {
            Ok(root) => root,
            Err(_) => {
                println!("BVAGENT SHARE get-deferred <root-unavailable>");
                return;
            }
        };
        // A preferred guest edit goes only over the Mac file it was preferred to (share_synced.rs).
        let recovery = share.engine.recovery_copy(name);
        let empty_recovery = recovery && finished.bytes.is_empty();
        let preferred = share.engine.preferred_mac_stat(name);
        let mac = share.fetch_mac.get(name).copied().or_else(|| preferred.map(Some));
        let bytes = match share
            .engine
            .on_guest_file(name.to_string(), finished.bytes, None)
        {
            GuestFileOutcome::AlreadySynced => {
                share.fetch_mac.remove(name);
                return;
            }
            GuestFileOutcome::Skip(reason) => return print_share_skip(name, reason),
            GuestFileOutcome::WriteHost(bytes) => bytes,
        };
        if empty_recovery {
            share.fetch_mac.remove(name);
            share.engine.on_host_copy_kept(name);
            return;
        }
        let destination = Destination::from_root(root, name, true);
        // A preferred copy holding the very bytes of the Mac file it was preferred to (an agent
        // without putmtime keeps its own write time on the upload) is not published: it would
        // only date the Mac file anew and mark it quarantined. Anything else goes on to the write.
        if let (false, Some(stat), Ok(d)) = (recovery, preferred, &destination) {
            if d.same_content(&bytes, stat) {
                share.fetch_mac.remove(name);
                share.engine.on_host_copy_same(name, stat);
                println!("BVAGENT SHARE skip {name} same-as-mac");
                return;
            }
        }
        let mtime = (share.engine.guest_mtime(name))
            .and_then(share_sync::share_mtime::guest_mtime_ms)
            .and_then(share_sync::share_mtime::system_time);
        share.forget_host_walk();
        let written = destination
            .and_then(|d| d.write_at_unless_changed(&bytes, mtime, mac, recovery, || share.mark_fetched(name, now)));
        match written {
            Ok(mtime) => {
                share.fetch_mac.remove(name);
                share.engine.on_host_write_succeeded(name, mtime);
                println!(
                    "BVAGENT SHARE guest->host {name} bytes={} t={}",
                    bytes.len(),
                    self.t_ms(now)
                );
            }
            Err(e) if crate::agent_console::share::host_destination::is_mac_changed(&e) => {
                share.fetch_mac.remove(name);
                // A recovery returns this only once its copy is kept beside the Mac file.
                if recovery {
                    share.recovery_copy_kept(name, now);
                } else {
                    share.engine.on_host_copy_kept(name);
                }
                println!("BVAGENT SHARE skip {name} mac-changed");
            }
            Err(e) => {
                let exhausted = share.engine.on_host_write_error(name, is_persistent(&e));
                // A full Mac disk fails these writes; run.log may be on it (host_log).
                bridgevm_hvf::host_log::out(format_args!("BVAGENT SHARE write-refused {name} {e}"));
                if crate::agent_console::share::share_big::is_out_of_space(&e) {
                    share.report_low_space(name, bytes.len() as u64);
                }
                if exhausted {
                    self.share = None;
                    self.queue.retain(|request| !is_share_req(request));
                    bridgevm_hvf::host_log::out("BVAGENT SHARE disabled failed-destination-limit");
                }
            }
        }
    }

    pub(in crate::agent_console) fn handle_share_delete(
        &mut self,
        name: &str,
        direction: ShareDelDirection,
        now: Instant,
    ) {
        if !matches!(direction, ShareDelDirection::GuestToHost) {
            return;
        }
        let Some(share) = self.share.as_mut() else {
            println!("BVAGENT SHARE del-skipped {name} share-disabled");
            return;
        };
        let Ok(root) = share.checked_root() else {
            println!("BVAGENT SHARE del-deferred <root-unavailable>");
            return;
        };
        share.forget_host_walk();
        let expected = share.engine.delete_host_stat(name);
        match root.try_clone().and_then(|dir| Destination::from_root(dir, name, false)).and_then(|d| match expected {
            Some(expected) => d.delete_unless_changed(expected),
            None => {
                println!("BVAGENT SHARE del-unchecked {name} host-version-unknown");
                d.delete()
            }
        }) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) if super::host_destination::is_mac_changed(&e) => {
                share.engine.on_host_copy_kept(name);
                println!("BVAGENT SHARE del-skipped {name} mac-changed");
                return;
            }
            Err(e) => {
                println!("BVAGENT SHARE del-refused {name} {e}");
                return;
            }
        }
        // A folder renamed or deleted in Windows leaves no empty copy of its tree on the Mac.
        Destination::remove_empty_folders(root, name);
        share.engine.on_host_deleted(name);
        println!("BVAGENT SHARE del guest->host {name} t={}", self.t_ms(now));
    }
}

#[cfg(test)]
pub(in crate::agent_console) mod get_test_fixture {
    //! Feed fetched bytes through the same full guest-path replies as the resident wire.

    use super::*;

    pub(in crate::agent_console) fn fetched(h: &mut AgentConsoleHarness, name: &str, bytes: &[u8]) {
        let now = Instant::now();
        let request = ServiceReq::ShareGet {
            name: name.to_owned(),
        };
        let wire = h
            .service_req_line(&request, now)
            .expect("share GET wire")
            .to_owned();
        h.in_flight = Some((request, now));
        let path = wire
            .strip_prefix("GET ")
            .and_then(|line| line.strip_suffix('\n'))
            .expect("GET path argument");
        // bvagent.ps1 GET replies use 24 KiB raw chunks.
        let chunk_bytes = 24 * 1024;
        let chunks = bytes.len().div_ceil(chunk_bytes);
        h.handle_service_reply(
            &format!("GETBEG {path} {} {chunks}", bytes.len()),
            None,
            None,
            now,
        );
        for (sequence, chunk) in bytes.chunks(chunk_bytes).enumerate() {
            h.handle_service_reply(
                &format!("GETCHUNK {sequence} {}", base64_encode(chunk)),
                None,
                None,
                now,
            );
        }
        h.handle_service_reply(&format!("GETEND {chunks}"), None, None, now);
    }
}

#[cfg(test)]
mod mtime_tests {
    //! A guest file fetched to the Mac keeps the modification time the guest listed.

    use super::*;
    use crate::agent_console::agent_console_tests::tests::harness;
    use crate::agent_console::share::share_host_write::get_test_fixture::fetched;

    #[test]
    fn a_fetched_guest_file_keeps_its_listed_modification_time() {
        let dir = std::env::temp_dir().join(format!("bvagent-get-mtime-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut h = harness();
        h.share = Some(ShareState::new(ShareSync::new(1024), dir.clone(), "C:\\share".into(), Duration::from_secs(1)));
        let entry = |name: &str, mtime: &str| LsEntry { name: name.into(), size: 3, is_dir: false, mtime: mtime.into() };
        let engine = &mut h.share.as_mut().unwrap().engine;
        let actions = engine.on_guest_listing_normalized([
            entry("dated.txt", "2026-11-08T00:05:12.1234567Z"),
            entry("undated.txt", "not a time"),
        ]);
        assert_eq!(actions.len(), 2);
        let ms = |time: std::time::SystemTime| time.duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
        let before = ms(std::time::SystemTime::now());
        fetched(&mut h, "dated.txt", b"abc");
        fetched(&mut h, "undated.txt", b"abc");
        let modified = |name: &str| ms(std::fs::metadata(dir.join(name)).unwrap().modified().unwrap());
        assert_eq!(modified("dated.txt"), 1_794_096_312_123);
        // A time the host cannot read leaves the write's own time.
        assert!(modified("undated.txt") + 1_000 >= before);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

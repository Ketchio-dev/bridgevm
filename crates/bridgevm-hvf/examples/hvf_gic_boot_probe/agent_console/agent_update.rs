//! Guest tools updates: the app's copy of the guest agent replaces an older
//! one in the running guest.
//!
//! The agent reports an ID over its own files in READY (`agent=<16 hex>`,
//! bvagent.ps1 `Get-BvAgentId`); the probe computes the same ID over the app's
//! copy. An agent without the field, or with another ID, gets the app's files,
//! one attempt per app session (the marker file in the VM bundle):
//! 1. Prepare: lock `C:\Program Files\BridgeVM\Agent` to SYSTEM and
//!    Administrators (the display driver's descriptor) and empty `<id>.new`.
//! 2. PUTBEG/PUTCHUNK/PUTEND every file into `<id>.new`.
//! 3. Arm: `bvagent-update.ps1 -Arm` checks every SHA-256 and parses the
//!    scripts, renames the folder to `<id>` and starts a guard task outside the
//!    agent. The guard points the agent task at the new folder and restarts the
//!    agent, and puts the old task back unless Confirm arrives within 180 s.
//! 4. Confirm, once the new agent's READY carries the new ID.

use super::*;
use sha2::{Digest, Sha256};

#[cfg(test)]
#[path = "agent_update_tests.rs"]
mod tests;

/// The guest agent's files, in the order both sides hash them.
pub(super) const AGENT_FILES: [&str; 9] = [
    "bvagent.ps1",
    "bvagent-input.ps1",
    "bvagent-unicode-input.cs",
    "bvagent-key-input.cs",
    "bvagent-pointer-input.cs",
    "bvagent-window-inventory.ps1",
    "bv-window-inventory.cs",
    "bvagent-task.ps1",
    "bvagent-update.ps1",
];

/// Larger than any agent file by far; the whole set is about 100 KB.
const MAX_AGENT_FILE_BYTES: u64 = 1024 * 1024;

pub(super) fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// The first 16 hex digits of SHA-256 over one `<name> <sha256 hex|missing>`
/// line per file.
pub(super) fn agent_id(digests: &[(&str, Option<[u8; 32]>)]) -> String {
    let mut list = String::new();
    for (name, digest) in digests {
        list.push_str(name);
        list.push(' ');
        match digest {
            Some(digest) => list.push_str(&hex_lower(digest)),
            None => list.push_str("missing"),
        }
        list.push('\n');
    }
    hex_lower(&Sha256::digest(list.as_bytes())[..8])
}

/// The agent ID in a READY line's text after `READY `: `<host> v3-share2
/// agent=<id>`. Anything but 16 lowercase hex digits counts as no ID.
pub(super) fn ready_agent_id(rest: &str) -> Option<&str> {
    rest.split(' ')
        .find_map(|token| token.strip_prefix("agent="))
        .filter(|id| id.len() == 16 && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
}

/// The agent says an update to its own version waits for the host's Confirm
/// (bvagent.ps1 Get-BvReady: update\pending.txt holds its ID).
pub(super) fn ready_update_pending(rest: &str) -> bool {
    rest.split(' ').any(|token| token == "update=pending")
}

/// Whether the READY agent takes PUTDATA, a chunk whose payload is its own line.
pub(super) fn ready_has_put_data(rest: &str) -> bool {
    rest.split(' ').any(|token| token == "putdata")
}

/// Whether the READY agent moves large shared-folder files in pieces (GETRANGE, LPUT*).
pub(super) fn ready_has_big_files(rest: &str) -> bool {
    rest.split(' ').any(|token| token == "bigfile")
}

/// Whether the READY agent answers an unchanged shared folder with LSSAME (share_listing_same.rs).
pub(super) fn ready_has_listing_same(rest: &str) -> bool {
    rest.split(' ').any(|token| token == "lssame")
}

/// Whether the READY agent has the chunked file verbs (PUTBEG/PUTCHUNK/PUTEND).
pub(super) fn ready_has_file_verbs(rest: &str) -> bool {
    rest.split(' ').any(|token| token == "v3-share2")
}

pub(super) struct AgentFile {
    pub(super) name: &'static str,
    pub(super) bytes: Vec<u8>,
    pub(super) sha256: [u8; 32],
}

/// The app's copy of the agent.
pub(super) struct BundledAgent {
    pub(super) id: String,
    pub(super) files: Vec<AgentFile>,
}

impl BundledAgent {
    pub(super) fn load(dir: &Path) -> Result<Self, String> {
        let mut files = Vec::with_capacity(AGENT_FILES.len());
        for name in AGENT_FILES {
            let path = dir.join(name);
            let size = std::fs::metadata(&path)
                .map_err(|_| format!("missing {name}"))?
                .len();
            if size > MAX_AGENT_FILE_BYTES {
                return Err(format!("too large {name}"));
            }
            let bytes = std::fs::read(&path).map_err(|_| format!("unreadable {name}"))?;
            let sha256 = Sha256::digest(&bytes).into();
            files.push(AgentFile {
                name,
                bytes,
                sha256,
            });
        }
        let digests: Vec<_> = files.iter().map(|f| (f.name, Some(f.sha256))).collect();
        Ok(Self {
            id: agent_id(&digests),
            files,
        })
    }
}

pub(super) const UPDATE_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_AGENT_UPDATE";
pub(super) const UPDATE_MARKER_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_AGENT_UPDATE_MARKER";
pub(super) const UPDATE_SESSION_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_AGENT_UPDATE_SESSION";

/// One folder per agent ID. Only administrators can write below Program Files.
pub(super) const GUEST_ROOT: &str = r"C:\Program Files\BridgeVM\Agent";
/// Boot-time traffic first: the update starts this long after the service.
pub(super) const SETTLE: Duration = Duration::from_secs(20);
/// From Arm to the new agent's READY. The guard rolls back after 180 s.
pub(super) const READY_DEADLINE: Duration = Duration::from_secs(200);

/// The display driver's descriptor (HvfDisplayDriverInstall.prepareCommand):
/// owned by Administrators, SYSTEM and Administrators only, nothing inherited.
const PREPARE_SCRIPT: &str = r#"$ErrorActionPreference = 'Stop'
try {
  # The update re-points the agent's task: an agent started any other way stays as it is.
  if ($null -eq (Get-ScheduledTask -TaskName 'BridgeVM Guest Agent' -ErrorAction SilentlyContinue)) { 'BVUPDATE skipped no-task'; return }
  $root = '@ROOT@'
  $sid = [Security.Principal.SecurityIdentifier]
  $locked = New-Object Security.AccessControl.DirectorySecurity
  $locked.SetSecurityDescriptorSddlForm('O:S-1-5-32-544D:PAI(A;OICI;FA;;;S-1-5-18)(A;OICI;FA;;;S-1-5-32-544)')
  function Test-Locked([string]$Path) {
    $acl = Get-Acl -LiteralPath $Path
    $aces = $acl.GetAccessRules($true, $true, $sid) | ForEach-Object { '{0} {1} {2} {3} {4}' -f $_.IdentityReference, $_.AccessControlType, $_.FileSystemRights, $_.InheritanceFlags, $_.IsInherited }
    $acl.AreAccessRulesProtected -and $acl.GetOwner($sid).Value -eq 'S-1-5-32-544' -and (@($aces | Sort-Object) -join '; ') -eq 'S-1-5-18 Allow FullControl ContainerInherit, ObjectInherit False; S-1-5-32-544 Allow FullControl ContainerInherit, ObjectInherit False'
  }
  [void][IO.Directory]::CreateDirectory((Split-Path $root))
  if (Test-Path -LiteralPath $root) {
    if ([IO.File]::GetAttributes($root) -band [IO.FileAttributes]::ReparsePoint) { throw "$root is a reparse point" }
    if (-not (Test-Locked $root)) { Set-Acl -LiteralPath $root -AclObject $locked }
  } else { [void][IO.Directory]::CreateDirectory($root, $locked) }
  if (-not (Test-Locked $root)) { throw "$root could not be locked to SYSTEM and Administrators" }
  Get-ChildItem -LiteralPath $root -Directory -Filter '*.new' | Remove-Item -Recurse -Force
  [void][IO.Directory]::CreateDirectory((Join-Path $root '@ID@.new'))
  'BVUPDATE ready'
} catch { 'BVUPDATE result=failed detail=' + (($_.Exception.Message -replace '\s+', ' ').Trim()) }
"#;

/// The update script runs only with the SHA-256 the host sent.
const ARM_SCRIPT: &str = r#"$f = '@ROOT@\@ID@.new\bvagent-update.ps1'
if (-not (Test-Path -LiteralPath $f) -or (Get-FileHash -LiteralPath $f -Algorithm SHA256).Hash -ne '@SHA@') { 'BVUPDATE result=failed detail=bvagent-update.ps1 hash'; exit 2 }
& $f -Arm -Id '@ID@' -Expect '@EXPECT@'
exit $LASTEXITCODE
"#;

const CONFIRM_SCRIPT: &str = r#"& '@ROOT@\@ID@\bvagent-update.ps1' -Confirm -Id '@ID@'
exit $LASTEXITCODE
"#;

/// A RUN line for `script` in Windows PowerShell; EncodedCommand needs no
/// quoting under cmd.exe.
pub(super) fn update_command(script: &str) -> String {
    let utf16: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut command = String::from(
        "powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand ",
    );
    base64_encode_into(&utf16, &mut command);
    command
}

fn script_for(template: &str, bundled: &BundledAgent) -> String {
    let expect: Vec<String> = bundled
        .files
        .iter()
        .map(|file| format!("{}={}", file.name, hex_lower(&file.sha256)))
        .collect();
    let update = bundled
        .files
        .iter()
        .find(|file| file.name == "bvagent-update.ps1")
        .map(|file| hex_lower(&file.sha256))
        .unwrap_or_default();
    template
        .replace("@ROOT@", GUEST_ROOT)
        .replace("@EXPECT@", &expect.join(","))
        .replace("@SHA@", &update)
        .replace("@ID@", &bundled.id)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum UpdateStep {
    Prepare,
    Put {
        file: usize,
        next_chunk: usize,
        phase: SharePutPhase,
    },
    Arm,
    Confirm,
}

/// An interrupted PUT starts again at PUTBEG, which truncates the file.
pub(super) fn rewind_agent_update_put(mut req: ServiceReq) -> ServiceReq {
    if let ServiceReq::AgentUpdate(UpdateStep::Put {
        next_chunk, phase, ..
    }) = &mut req
    {
        *next_chunk = 0;
        *phase = SharePutPhase::Beg;
    }
    req
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum UpdatePhase {
    /// Waiting for the service, the settle time and a READY.
    Idle,
    /// Prepare and the PUTs.
    Staging,
    /// Every file is in `<id>.new`; the Arm waits for the gate.
    Staged,
    Arming,
    /// The guard is switching agents; `late` once the deadline was reported.
    Armed { since: Instant, late: bool },
    Confirming,
    /// Nothing more in this run.
    Done,
}

pub(super) struct AgentUpdate {
    pub(super) bundled: BundledAgent,
    pub(super) marker: Option<PathBuf>,
    pub(super) session: String,
    pub(super) phase: UpdatePhase,
    /// The last READY: its agent ID (None for an older agent) and file verbs.
    pub(super) guest: Option<(Option<String>, bool)>,
    /// The agent being replaced, for the log and the marker.
    pub(super) from: String,
    pub(super) service_since: Option<Instant>,
    /// A shutdown went to the guest: nothing is armed after it.
    pub(super) stop_requested: bool,
}

impl AgentUpdate {
    pub(super) fn from_env() -> Option<Self> {
        let dir = std::env::var(UPDATE_ENV).ok().filter(|dir| !dir.is_empty())?;
        let marker = std::env::var(UPDATE_MARKER_ENV)
            .ok()
            .filter(|path| !path.is_empty())
            .map(PathBuf::from);
        let session = std::env::var(UPDATE_SESSION_ENV).unwrap_or_default();
        match BundledAgent::load(Path::new(&dir)) {
            Ok(bundled) => Some(Self::new(bundled, marker, session)),
            Err(reason) => {
                println!("BVAGENT UPDATE unavailable reason={reason}");
                None
            }
        }
    }

    pub(super) fn new(bundled: BundledAgent, marker: Option<PathBuf>, session: String) -> Self {
        Self {
            bundled,
            marker,
            session,
            phase: UpdatePhase::Idle,
            guest: None,
            from: String::new(),
            service_since: None,
            stop_requested: false,
        }
    }

    /// The app's Stop and quit send `shutdown.exe ...` through the control file.
    pub(super) fn note_ctl_command(&mut self, command: &str) {
        let program = command.split_whitespace().next().unwrap_or("");
        let program = program.rsplit(['\\', '/']).next().unwrap_or(program);
        if program.eq_ignore_ascii_case("shutdown") || program.eq_ignore_ascii_case("shutdown.exe") {
            self.stop_requested = true;
        }
    }

    fn marker_field(&self, key: &str) -> Option<String> {
        let bytes = std::fs::read(self.marker.as_ref()?).ok()?;
        let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
        value.get(key)?.as_str().map(str::to_string)
    }

    fn marker_is_for_bundled(&self) -> bool {
        self.marker_field("bundled").as_deref() == Some(self.bundled.id.as_str())
    }

    /// This app session already tried to install this agent.
    pub(super) fn attempted(&self) -> bool {
        self.marker_is_for_bundled()
            && self.marker_field("session").as_deref() == Some(self.session.as_str())
    }

    /// The guest had no agent task for this version: it is left alone, with no traffic.
    pub(super) fn skipped_no_task(&self) -> bool {
        self.marker_is_for_bundled()
            && self.marker_field("state").as_deref() == Some("skipped")
            && self.marker_field("detail").as_deref() == Some("no-task")
    }

    pub(super) fn write_marker(&self, state: &str, detail: &str) {
        let Some(path) = self.marker.as_ref() else {
            return;
        };
        let updated = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let value = serde_json::json!({
            "bundled": self.bundled.id,
            "from": self.from,
            "state": state,
            "detail": detail,
            "session": self.session,
            "updated_unix_ms": updated,
        });
        let temporary = path.with_extension("json.tmp");
        let written = serde_json::to_vec_pretty(&value)
            .map_err(std::io::Error::other)
            .and_then(|bytes| std::fs::write(&temporary, bytes))
            .and_then(|()| std::fs::rename(&temporary, path));
        if let Err(error) = written {
            println!("BVAGENT UPDATE marker-unwritable {error}");
        }
    }
}

/// Guest text for one log line: printable ASCII, at most 160 characters.
fn guest_detail(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    words
        .join(" ")
        .chars()
        .map(|c| if c.is_ascii_graphic() || c == ' ' { c } else { '?' })
        .take(160)
        .collect()
}

/// The last `BVUPDATE ...` line of a command's output.
fn bvupdate_line(output: &str) -> Option<&str> {
    output
        .lines()
        .map(str::trim)
        .rfind(|line| line.starts_with("BVUPDATE "))
}

fn is_update_req(req: &ServiceReq) -> bool {
    matches!(req, ServiceReq::AgentUpdate(_))
}

/// A clipboard image on the move: a Mac image (PUT, then CLIPSETIMG) or a
/// guest image fetch (GET); file copies either way are held the same way.
pub(super) fn is_clip_image_req(req: &ServiceReq) -> bool {
    matches!(
        req,
        ServiceReq::ClipImagePush(_)
            | ServiceReq::ClipImagePull { .. }
            | ServiceReq::ClipFilesPull { .. }
            | ServiceReq::ClipFilesPush(_)
    )
}

/// READY's marker of an agent whose scheduled task keeps running on battery
/// (bvagent-task.ps1).
pub(super) const SURVIVES_BATTERY_MARKER: &str = "onbattery";

impl AgentConsoleHarness {
    /// This boot's agent tells the host power monitor whether Windows may see
    /// the Mac on battery; nothing has said so yet.
    pub fn share_agent_survives_battery(&mut self, flag: &Arc<AtomicBool>) {
        flag.store(false, Ordering::Relaxed);
        self.agent_survives_battery = Arc::clone(flag);
    }

    /// Windows sees the Mac on battery only once the running agent's task
    /// survives it, and not while an update may still put an older task back
    /// (from Arm until the new agent is confirmed): Task Scheduler would not
    /// start that one on battery.
    fn publish_agent_survives_battery(&self) {
        let settled = !self.agent_update.as_ref().is_some_and(|update| {
            matches!(update.phase, UpdatePhase::Armed { .. } | UpdatePhase::Confirming)
        });
        self.agent_survives_battery
            .store(self.ready_survives_battery && settled, Ordering::Relaxed);
    }

    /// READY or re-READY: an Armed update learns whether the new agent came up.
    pub(super) fn note_agent_ready(&mut self, rest: &str, now: Instant) {
        self.ready_survives_battery = rest.split(' ').any(|token| token == SURVIVES_BATTERY_MARKER);
        self.put_data = ready_has_put_data(rest);
        self.share_big = ready_has_big_files(rest);
        self.drops.agent_opens = rest.split(' ').any(|token| token == "openfile");
        // A new agent process may have the display verbs (an update brings them).
        self.display_verbs_refused = false;
        if let Some(share) = self.share.as_mut() {
            let limit = (!self.share_big).then_some(harness_protocol::MAX_AGENT_GET_BYTES as u64);
            share.engine.set_whole_file_limit(limit);
            share.listing_same.ready(ready_has_listing_same(rest));
        }
        let t = self.t_ms(now);
        let Some(update) = self.agent_update.as_mut() else {
            self.publish_agent_survives_battery();
            return;
        };
        let id = ready_agent_id(rest).map(str::to_string);
        let bundled = id.as_deref() == Some(update.bundled.id.as_str());
        // The guest's word decides, not the marker or this session's phase: a
        // reboot after the deadline, an Arm that failed after it registered
        // the guard, an unwritable marker or a new session while the guard
        // waits all end in a confirmed update instead of a silent rollback.
        let pending = bundled && ready_update_pending(rest);
        update.guest = Some((id.clone(), ready_has_file_verbs(rest)));
        match update.phase {
            UpdatePhase::Armed { .. } if !bundled => {
                let to = id.as_deref().unwrap_or("none");
                println!("BVAGENT UPDATE rolled-back to={to} t={t}");
                update.write_marker("rolled-back", to);
                update.phase = UpdatePhase::Done;
            }
            UpdatePhase::Armed { .. } => self.queue_agent_update_confirm(),
            UpdatePhase::Confirming => {}
            _ if pending => self.queue_agent_update_confirm(),
            _ => {}
        }
        self.publish_agent_survives_battery();
    }

    fn queue_agent_update_confirm(&mut self) {
        let Some(update) = self.agent_update.as_mut() else {
            return;
        };
        update.phase = UpdatePhase::Confirming;
        self.queue.retain(|req| !is_update_req(req));
        self.queue
            .push_front(ServiceReq::AgentUpdate(UpdateStep::Confirm));
    }

    /// From the first step to the new agent's READY (or the deadline): the
    /// agent may restart under anything else that holds guest state.
    pub(super) fn agent_update_busy(&self) -> bool {
        self.agent_update.as_ref().is_some_and(|update| {
            matches!(
                update.phase,
                UpdatePhase::Staging
                    | UpdatePhase::Staged
                    | UpdatePhase::Arming
                    | UpdatePhase::Armed { late: false, .. }
                    | UpdatePhase::Confirming
            )
        })
    }

    /// The update's file stream holds the wire, and the guard's switch
    /// restarts the agent under anything in flight: a clipboard image waits
    /// for both (service_pump, handle_guest_clip). The text poll goes on.
    pub(super) fn agent_update_holds_images(&self) -> bool {
        self.agent_update.as_ref().is_some_and(|update| {
            matches!(
                update.phase,
                UpdatePhase::Staging
                    | UpdatePhase::Arming
                    | UpdatePhase::Armed { late: false, .. }
                    | UpdatePhase::Confirming
            )
        })
    }

    /// Why the update waits to begin or to arm; a stop cancels it for this
    /// session. A running Finder drop holds an open guest file that an agent
    /// restart would end, so the update waits for it. A drop that is still
    /// waiting does not count: it cannot start while the update runs
    /// (enqueue_file_drop), so counting it held both until a Stop. A
    /// clipboard image queued or in flight goes first too: images are held
    /// only during the stream and the switch, never at these two points.
    fn agent_update_gate(&self) -> Option<&'static str> {
        if self.agent_update.as_ref()?.stop_requested {
            return Some("stop");
        }
        if self.drops.is_running() || self.any_pending(is_drop_req) {
            return Some("drop");
        }
        if self.any_pending(is_clip_image_req) {
            return Some("clipboard-image");
        }
        if self.big_share_active() {
            return Some("share-transfer");
        }
        None
    }

    pub(super) fn enqueue_agent_update(&mut self, now: Instant) {
        let t = self.t_ms(now);
        let gate = self.agent_update_gate();
        let Some(update) = self.agent_update.as_mut() else {
            return;
        };
        let since = *update.service_since.get_or_insert(now);
        match update.phase {
            UpdatePhase::Idle if now.duration_since(since) >= SETTLE => {
                // No READY (a PONG-only handshake): the agent is unknown.
                let Some((id, file_verbs)) = update.guest.clone() else {
                    return;
                };
                if gate.is_some_and(|gate| gate != "stop") {
                    return;
                }
                update.phase = UpdatePhase::Done;
                if id.as_deref() == Some(update.bundled.id.as_str()) {
                    println!("BVAGENT UPDATE current agent={} t={t}", update.bundled.id);
                    return;
                }
                let skipped = if !file_verbs {
                    Some("agent-too-old")
                } else if update.attempted() {
                    Some("attempted")
                } else if update.skipped_no_task() {
                    Some("no-task")
                } else if gate == Some("stop") {
                    Some("stop")
                } else {
                    None
                };
                if let Some(reason) = skipped {
                    println!("BVAGENT UPDATE skipped reason={reason} t={t}");
                    return;
                }
                update.from = id.unwrap_or_else(|| "none".to_string());
                println!(
                    "BVAGENT UPDATE begin from={} to={} t={t}",
                    update.from, update.bundled.id
                );
                update.write_marker("begin", "");
                update.phase = UpdatePhase::Staging;
                self.queue
                    .push_back(ServiceReq::AgentUpdate(UpdateStep::Prepare));
            }
            UpdatePhase::Staged => match gate {
                Some("stop") => {
                    println!("BVAGENT UPDATE skipped reason=stop t={t}");
                    update.write_marker("stopped", "");
                    update.phase = UpdatePhase::Done;
                }
                Some(_) => {}
                None => {
                    update.phase = UpdatePhase::Arming;
                    self.queue.push_back(ServiceReq::AgentUpdate(UpdateStep::Arm));
                }
            },
            UpdatePhase::Armed { since, late: false }
                if now.duration_since(since) >= READY_DEADLINE =>
            {
                println!("BVAGENT UPDATE failed reason=no-ready t={t}");
                update.write_marker("failed", "no-ready");
                update.phase = UpdatePhase::Armed { since, late: true };
            }
            _ => {}
        }
    }

    /// The app's Stop or quit went to the guest: no update step goes ahead of
    /// it, and an update that is not armed yet ends here. An Arm already on
    /// the wire is the guard's to finish or roll back.
    pub(super) fn stop_agent_update(&mut self, now: Instant) {
        let t = self.t_ms(now);
        self.queue.retain(|req| !is_update_req(req));
        let arm_in_flight = matches!(
            self.in_flight.as_ref(),
            Some((ServiceReq::AgentUpdate(UpdateStep::Arm), _))
        );
        let Some(update) = self.agent_update.as_mut() else {
            return;
        };
        let unarmed = matches!(update.phase, UpdatePhase::Staging | UpdatePhase::Staged)
            || (update.phase == UpdatePhase::Arming && !arm_in_flight);
        if unarmed {
            println!("BVAGENT UPDATE skipped reason=stop t={t}");
            update.write_marker("stopped", "");
            update.phase = UpdatePhase::Done;
        }
    }

    fn agent_update_staging(&self) -> bool {
        self.agent_update
            .as_ref()
            .is_some_and(|update| update.phase == UpdatePhase::Staging)
    }

    fn fail_agent_update(&mut self, reason: &str, detail: &str, now: Instant) {
        let t = self.t_ms(now);
        self.queue.retain(|req| !is_update_req(req));
        let Some(update) = self.agent_update.as_mut() else {
            return;
        };
        let detail = guest_detail(detail);
        println!("BVAGENT UPDATE failed reason={reason} detail={detail} t={t}");
        update.write_marker("failed", &format!("{reason} {detail}"));
        update.phase = UpdatePhase::Done;
    }

    /// An agent restart took this step off the wire.
    pub(super) fn agent_update_interrupted(&mut self, step: UpdateStep, now: Instant) {
        match step {
            // The guard restarts the agent; the reply can be lost under it.
            UpdateStep::Arm | UpdateStep::Confirm => {
                if let Some(update) = self.agent_update.as_mut() {
                    if step == UpdateStep::Arm {
                        update.write_marker("armed", "");
                    }
                    update.phase = UpdatePhase::Armed {
                        since: now,
                        late: false,
                    };
                }
            }
            UpdateStep::Prepare | UpdateStep::Put { .. } => {
                self.fail_agent_update("agent-restarted", "", now)
            }
        }
    }

    pub(super) fn write_agent_update_line(&mut self, step: &UpdateStep) -> bool {
        let Some(update) = self.agent_update.as_ref() else {
            return false;
        };
        let script = match step {
            UpdateStep::Prepare => script_for(PREPARE_SCRIPT, &update.bundled),
            UpdateStep::Arm => script_for(ARM_SCRIPT, &update.bundled),
            UpdateStep::Confirm => script_for(CONFIRM_SCRIPT, &update.bundled),
            UpdateStep::Put {
                file,
                phase: SharePutPhase::Beg,
                ..
            } => {
                let file = &update.bundled.files[*file];
                let path = format!(r"{GUEST_ROOT}\{}.new\{}", update.bundled.id, file.name);
                self.service_line_scratch.push_str("PUTBEG ");
                base64_encode_into(path.as_bytes(), &mut self.service_line_scratch);
                let _ = writeln!(
                    self.service_line_scratch,
                    " {} {}",
                    file.bytes.len(),
                    file.bytes.len().div_ceil(SHARE_PUT_CHUNK_BYTES)
                );
                return true;
            }
            UpdateStep::Put { .. } => return false,
        };
        let command = update_command(&script);
        debug_assert!(command.len() <= super::desktop_switch::MAX_RUN_COMMAND_CHARS);
        write_command_wire_line_into(&command, &mut self.service_line_scratch);
        true
    }

    pub(super) fn handle_agent_update_reply_with_sender(
        &mut self,
        line: &str,
        now: Instant,
        send: &mut dyn FnMut(&[u8]),
    ) {
        let Some((ServiceReq::AgentUpdate(step), _)) = self.in_flight.as_ref() else {
            return;
        };
        let step = step.clone();
        if let UpdateStep::Put { .. } = step {
            self.handle_agent_update_put_reply(line, now, send);
            return;
        }
        let (code, output) = if let Some((code, b64)) = parse_out_line(line) {
            let text = base64_decode(b64).unwrap_or_default();
            (code, String::from_utf8_lossy(&text).into_owned())
        } else if line.starts_with("OUTEND ") {
            (-1, "long output".to_string())
        } else {
            return;
        };
        self.complete_in_flight();
        let reply = bvupdate_line(&output).unwrap_or("");
        let t = self.t_ms(now);
        match step {
            // A stop ended the update while Prepare ran.
            UpdateStep::Prepare if !self.agent_update_staging() => {}
            UpdateStep::Prepare if code == 0 && reply == "BVUPDATE ready" => {
                self.queue.push_back(ServiceReq::AgentUpdate(UpdateStep::Put {
                    file: 0,
                    next_chunk: 0,
                    phase: SharePutPhase::Beg,
                }));
            }
            UpdateStep::Prepare if code == 0 && reply == "BVUPDATE skipped no-task" => {
                if let Some(update) = self.agent_update.as_mut() {
                    println!("BVAGENT UPDATE skipped reason=no-task t={t}");
                    update.write_marker("skipped", "no-task");
                    update.phase = UpdatePhase::Done;
                }
            }
            UpdateStep::Prepare => self.fail_agent_update("prepare", reply, now),
            UpdateStep::Arm if code == 0 && reply.starts_with("BVUPDATE armed") => {
                if let Some(update) = self.agent_update.as_mut() {
                    println!("BVAGENT UPDATE armed to={} t={t}", update.bundled.id);
                    update.write_marker("armed", "");
                    update.phase = UpdatePhase::Armed {
                        since: now,
                        late: false,
                    };
                }
            }
            UpdateStep::Arm => self.fail_agent_update("arm", reply, now),
            UpdateStep::Confirm => {
                let Some(update) = self.agent_update.as_mut() else {
                    return;
                };
                if code == 0 && reply.starts_with("BVUPDATE confirmed") {
                    println!(
                        "BVAGENT UPDATE ok from={} to={} t={t}",
                        if update.from.is_empty() { "earlier" } else { update.from.as_str() },
                        update.bundled.id
                    );
                    update.write_marker("ok", "");
                    update.phase = UpdatePhase::Done;
                } else if code == 0 && reply.starts_with("BVUPDATE rolled-back") {
                    // The guard was already putting the old agent back; its
                    // READY speaks for its task.
                    println!("BVAGENT UPDATE rolled-back to={} t={t}", if update.from.is_empty() { "earlier" } else { update.from.as_str() });
                    update.write_marker("rolled-back", &update.from);
                    update.phase = UpdatePhase::Done;
                    self.ready_survives_battery = false;
                } else if code == 0 && reply.starts_with("BVUPDATE nothing-pending") {
                    // The guest has no update of this version waiting: nothing to report.
                    println!("BVAGENT UPDATE confirm-noop t={t}");
                    update.phase = UpdatePhase::Done;
                } else {
                    // The guard puts the old agent back; its READY reports that.
                    println!(
                        "BVAGENT UPDATE confirm-failed detail={} t={t}",
                        guest_detail(reply)
                    );
                    update.phase = UpdatePhase::Armed {
                        since: now,
                        late: false,
                    };
                }
            }
            UpdateStep::Put { .. } => {}
        }
        self.publish_agent_survives_battery();
    }

    fn handle_agent_update_put_reply(
        &mut self,
        line: &str,
        now: Instant,
        send: &mut dyn FnMut(&[u8]),
    ) {
        let t = self.t_ms(now);
        let Some((
            ServiceReq::AgentUpdate(UpdateStep::Put {
                file,
                next_chunk,
                phase,
            }),
            sent_at,
        )) = self.in_flight.as_mut()
        else {
            return;
        };
        let Some(update) = self.agent_update.as_ref() else {
            return;
        };
        let agent_file = &update.bundled.files[*file];
        let nchunks = agent_file.bytes.len().div_ceil(SHARE_PUT_CHUNK_BYTES);
        let acked = match *phase {
            SharePutPhase::Beg => line == "OK PUTBEG",
            SharePutPhase::Chunk => line
                .strip_prefix("OK PUTCHUNK ")
                .and_then(|seq| seq.parse::<usize>().ok())
                .is_some_and(|seq| seq + 1 == *next_chunk),
            _ => false,
        };
        if acked {
            self.service_line_scratch.clear();
            if *next_chunk < nchunks {
                let start = *next_chunk * SHARE_PUT_CHUNK_BYTES;
                let end = (start + SHARE_PUT_CHUNK_BYTES).min(agent_file.bytes.len());
                let _ = write!(self.service_line_scratch, "PUTCHUNK {next_chunk} ");
                base64_encode_into(&agent_file.bytes[start..end], &mut self.service_line_scratch);
                self.service_line_scratch.push('\n');
                *next_chunk += 1;
                *phase = SharePutPhase::Chunk;
            } else {
                let _ = writeln!(self.service_line_scratch, "PUTEND {nchunks}");
                *phase = SharePutPhase::End;
            }
            *sent_at = now;
            self.last_send = Some(now);
            send(self.service_line_scratch.as_bytes());
            return;
        }
        let file = *file;
        let name = agent_file.name;
        let expected = agent_file.bytes.len();
        if *phase == SharePutPhase::End && line.starts_with("PUTOK ") {
            self.complete_in_flight();
            let written = line.rsplit(' ').next().and_then(|n| n.parse::<usize>().ok());
            if written != Some(expected) {
                self.fail_agent_update("put", &format!("{name} {line}"), now);
            } else if !self.agent_update_staging() {
                // A stop ended the update: no further file goes ahead of it.
            } else if file + 1 < AGENT_FILES.len() {
                self.queue.push_front(ServiceReq::AgentUpdate(UpdateStep::Put {
                    file: file + 1,
                    next_chunk: 0,
                    phase: SharePutPhase::Beg,
                }));
            } else if let Some(update) = self.agent_update.as_mut() {
                let bytes: usize = update.bundled.files.iter().map(|f| f.bytes.len()).sum();
                println!(
                    "BVAGENT UPDATE staged files={} bytes={bytes} t={t}",
                    AGENT_FILES.len()
                );
                update.phase = UpdatePhase::Staged;
            }
        } else if line.starts_with("ERR") || line.starts_with("OUT ") {
            // An agent without the chunked verbs answers OUT 255 unknown token.
            self.complete_in_flight();
            self.fail_agent_update("put", &format!("{name} {line}"), now);
        }
    }
}

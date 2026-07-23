//! Host-share scanning, validation, and request construction.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ShareFileReadError {
    Io,
    TooLarge(u64),
}

pub(super) fn read_share_file_bounded(
    path: &Path,
    max_bytes: u64,
) -> Result<Vec<u8>, ShareFileReadError> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|_| ShareFileReadError::Io)?;
    let size = file.metadata().map_err(|_| ShareFileReadError::Io)?.len();
    if size > max_bytes {
        return Err(ShareFileReadError::TooLarge(size));
    }
    let capacity = usize::try_from(size).map_err(|_| ShareFileReadError::TooLarge(size))?;
    let read_limit = max_bytes.checked_add(1).ok_or(ShareFileReadError::Io)?;
    let mut bytes = Vec::with_capacity(capacity);
    file.take(read_limit)
        .read_to_end(&mut bytes)
        .map_err(|_| ShareFileReadError::Io)?;
    if bytes.len() as u64 > max_bytes {
        return Err(ShareFileReadError::TooLarge(bytes.len() as u64));
    }
    Ok(bytes)
}

/// Short label for a service request, used in the stall-timeout print.
pub(super) fn req_kind(req: &ServiceReq) -> &'static str {
    match req {
        ServiceReq::ClipPoll => "clip-poll",
        ServiceReq::ClipPush(_) => "clip-push",
        ServiceReq::Ctl(_) => "ctl",
        ServiceReq::Ping => "ping",
        ServiceReq::ShareLs => "share-ls",
        ServiceReq::ShareGet { .. } => "share-get",
        ServiceReq::SharePut { .. } => "share-put",
        ServiceReq::ShareDel { .. } => "share-del",
    }
}

pub(super) fn is_share_req(req: &ServiceReq) -> bool {
    matches!(
        req,
        ServiceReq::ShareLs
            | ServiceReq::ShareGet { .. }
            | ServiceReq::SharePut { .. }
            | ServiceReq::ShareDel { .. }
    )
}

pub(super) fn init_share_from_env() -> Option<ShareState> {
    let spec = match std::env::var(SHARE_ENV) {
        Ok(value) if !value.is_empty() => value,
        _ => return None,
    };
    let Some((host, guest)) = parse_share_spec(&spec) else {
        println!("BVAGENT SHARE bad spec");
        return None;
    };
    let interval_ms = env_u64(SHARE_MS_ENV, DEFAULT_SHARE_MS).max(SHARE_MS_FLOOR);
    let max_kb = env_u64(SHARE_MAX_KB_ENV, DEFAULT_SHARE_MAX_KB);
    Some(ShareState {
        engine: ShareSync::new(max_kb),
        host_dir: PathBuf::from(host),
        guest_dir: guest,
        interval: Duration::from_millis(interval_ms),
        last_poll: None,
        host_skip_seen: HashSet::new(),
        guest_ls_scratch: Vec::new(),
        host_scan_scratch: Vec::new(),
    })
}

pub(super) fn parse_share_spec(spec: &str) -> Option<(String, String)> {
    let (host, guest) = spec.split_once("::")?;
    if host.is_empty() || guest.is_empty() {
        return None;
    }
    Some((host.to_string(), guest.to_string()))
}

pub(super) fn scan_share_host_dir(share: &mut ShareState) {
    share.host_scan_scratch.clear();
    let root = &share.host_dir;
    let max_bytes = share.engine.max_bytes();
    let host_skip_seen = &mut share.host_skip_seen;
    scan_share_host_dir_inner(
        root,
        root,
        max_bytes,
        host_skip_seen,
        &mut share.host_scan_scratch,
    );
}

pub(super) fn scan_share_host_dir_inner(
    root: &Path,
    dir: &Path,
    max_bytes: u64,
    host_skip_seen: &mut HashSet<(String, u128, HostSkipKind)>,
    files: &mut Vec<HostFile>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            scan_share_host_dir_inner(root, &path, max_bytes, host_skip_seen, files);
            continue;
        }
        if !meta.is_file() {
            continue;
        }
        let Some(name) = host_rel_path(root, &path) else {
            continue;
        };
        let mtime_ms = file_mtime_ms_from_meta(&meta).unwrap_or(0);
        let size = meta.len();
        if size > max_bytes {
            print_host_skip_once_seen(
                host_skip_seen,
                &name,
                mtime_ms,
                HostSkipKind::TooLarge,
                size,
            );
            continue;
        }
        files.push(HostFile {
            name,
            size,
            mtime_ms,
        });
    }
}

pub(super) fn host_rel_path(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let mut out = String::new();
    for component in rel.components() {
        let component = component.as_os_str().to_string_lossy();
        // A literal backslash is legal in a macOS filename but is a path
        // separator in the Windows guest. Treating it as a normal byte here
        // creates two different sync keys and repeats PUT/tombstone actions.
        if component.contains('\\') {
            return None;
        }
        if !out.is_empty() {
            out.push('/');
        }
        out.push_str(&component);
    }
    Some(out)
}

pub(super) fn file_mtime_ms(path: &Path) -> Option<u128> {
    std::fs::metadata(path)
        .ok()
        .and_then(|meta| file_mtime_ms_from_meta(&meta))
}

pub(super) fn file_mtime_ms_from_meta(meta: &std::fs::Metadata) -> Option<u128> {
    meta.modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis())
}

pub(super) fn print_host_skip_once(
    share: &mut ShareState,
    name: &str,
    mtime_ms: u128,
    kind: HostSkipKind,
    size: u64,
) {
    print_host_skip_once_seen(&mut share.host_skip_seen, name, mtime_ms, kind, size);
}

pub(super) fn print_host_skip_once_seen(
    host_skip_seen: &mut HashSet<(String, u128, HostSkipKind)>,
    name: &str,
    mtime_ms: u128,
    kind: HostSkipKind,
    size: u64,
) {
    if !host_skip_seen.insert((name.to_string(), mtime_ms, kind)) {
        return;
    }
    match kind {
        HostSkipKind::TooLarge => println!("BVAGENT SHARE skip {name} too-large {size}"),
    }
}

pub(super) fn print_guest_skip(name: &str, reason: SkipReason) {
    match reason {
        SkipReason::TooLarge { size } => {
            println!("BVAGENT SHARE skip {name} too-large {size}")
        }
    }
}

pub(super) fn write_share_guest_path_into(dir: &str, name: &str, out: &mut String) {
    out.clear();
    out.reserve(dir.len() + 1 + name.len());
    out.push_str(dir);
    out.push('\\');
    share_sync::append_guest_rel_into(name, out);
}

pub(super) fn share_put_req(name: String, bytes: Vec<u8>, hash: u64) -> ServiceReq {
    let phase = if bytes.len() <= SHARE_PUT_CHUNK_BYTES {
        SharePutPhase::Legacy
    } else {
        SharePutPhase::Beg
    };
    ServiceReq::SharePut {
        name,
        bytes,
        hash,
        next_chunk: 0,
        phase,
    }
}

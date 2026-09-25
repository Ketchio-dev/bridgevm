//! Atomically consume a fixed host stop request without following a link.
use std::fs::{self, Metadata, OpenOptions};
use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

const T17_PREFIX: &[u8] = b"t17-nonce-v1:";
const T17_SIZE: u64 = 46;
const READ_CAP: u64 = 64;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum StopRequest {
    Legacy,
    T17Nonce(String),
}

impl StopRequest {
    pub(super) fn nonce(&self) -> Option<&str> {
        match self {
            Self::Legacy => None,
            Self::T17Nonce(value) => Some(value),
        }
    }
}

fn same_file(left: &Metadata, right: &Metadata) -> bool {
    left.dev() == right.dev()
        && left.ino() == right.ino()
        && left.uid() == right.uid()
        && left.mode() == right.mode()
        && left.nlink() == right.nlink()
        && left.len() == right.len()
        && left.mtime() == right.mtime()
        && left.mtime_nsec() == right.mtime_nsec()
        && left.ctime() == right.ctime()
        && left.ctime_nsec() == right.ctime_nsec()
}

pub(super) fn consume(path: &Path) -> Option<StopRequest> {
    let before = fs::symlink_metadata(path).ok()?;
    if !before.is_file() || before.nlink() != 1 || before.len() > READ_CAP {
        return None;
    }
    let mut file = OpenOptions::new().read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path).ok()?;
    if !same_file(&before, &file.metadata().ok()?) {
        return None;
    }
    let mut bytes = vec![0; before.len().min(READ_CAP) as usize];
    file.read_exact(&mut bytes).ok()?;
    let request = if bytes.starts_with(T17_PREFIX) {
        if before.len() != T17_SIZE || bytes.len() != T17_SIZE as usize
            || bytes[45] != b'\n' || !bytes[13..45].iter().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            || before.mode() & 0o777 != 0o600
            // SAFETY: geteuid has no pointers and only reads the process credentials.
            || before.uid() != unsafe { libc::geteuid() }
        {
            return None;
        }
        StopRequest::T17Nonce(String::from_utf8(bytes[13..45].to_vec()).ok()?)
    } else if bytes.starts_with(b"t17-") {
        return None;
    } else {
        StopRequest::Legacy
    };
    if !same_file(&before, &file.metadata().ok()?)
        || !same_file(&before, &fs::symlink_metadata(path).ok()?)
        || fs::remove_file(path).is_err()
    {
        return None;
    }
    Some(request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    #[test]
    fn typed_nonce_requires_exact_owned_mode_and_legacy_remains_supported() {
        let dir = std::env::temp_dir().join(format!("bridgevm-stop-request-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let request = dir.join("request");
        assert_eq!(consume(&request), None);
        fs::write(&request, b"").unwrap();
        assert_eq!(consume(&request), Some(StopRequest::Legacy));
        fs::write(&request, b"host gate failed\n").unwrap();
        assert_eq!(consume(&request), Some(StopRequest::Legacy));
        fs::write(&request, b"t17-nonce-v1:short\n").unwrap();
        assert_eq!(consume(&request), None);
        fs::remove_file(&request).unwrap();
        fs::write(&request, vec![b'x'; READ_CAP as usize + 1]).unwrap();
        assert_eq!(consume(&request), None);
        fs::remove_file(&request).unwrap();
        let nonce = "a".repeat(32);
        fs::write(&request, format!("t17-nonce-v1:{nonce}\n")).unwrap();
        fs::set_permissions(&request, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(consume(&request), Some(StopRequest::T17Nonce(nonce)));
        assert!(!request.exists());
        let target = dir.join("target");
        fs::write(&target, b"").unwrap();
        symlink(&target, &request).unwrap();
        assert_eq!(consume(&request), None);
        fs::remove_dir_all(&dir).unwrap();
    }
}

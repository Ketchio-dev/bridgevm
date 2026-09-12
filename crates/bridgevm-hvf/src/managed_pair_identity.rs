use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

fn digest(paths: [&Path; 2]) -> String {
    let mut hash = Sha256::new();
    for path in paths {
        let bytes = path.as_os_str().as_bytes();
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

pub(super) fn stable_root(disk: &Path, vars: &Path) -> PathBuf {
    let parent = disk.parent().unwrap();
    let common = parent
        .components()
        .zip(vars.components())
        .take_while(|(a, b)| a == b)
        .count();
    let mut relative_vars = PathBuf::new();
    for _ in parent.components().skip(common) {
        relative_vars.push("..");
    }
    for component in vars.components().skip(common) {
        relative_vars.push(component.as_os_str());
    }
    let id = digest([Path::new(disk.file_name().unwrap()), &relative_vars]);
    parent.join(format!(".bridgevm-pair-v2-{id}"))
}

fn legacy_root(disk: &Path, vars: &Path) -> PathBuf {
    disk.parent()
        .unwrap()
        .join(format!(".bridgevm-pair-{}", digest([disk, vars])))
}

/// Caller owns logical disk/vars leases. Do not infer a binding from one orphan:
/// it could belong to another VM, and guessing would boot the wrong guest state.
pub(super) fn resolve(disk: &Path, vars: &Path) -> io::Result<PathBuf> {
    let stable = stable_root(disk, vars);
    let legacy = legacy_root(disk, vars);
    let has_stable = super::private_directory(&stable, false)?;
    let has_legacy = super::private_directory(&legacy, false)?;
    if has_stable && has_legacy {
        return Err(io::Error::other(
            "conflicting legacy and relative media identities",
        ));
    }
    if has_stable {
        return Ok(stable);
    }
    if has_legacy {
        fs::rename(&legacy, &stable)?;
        File::open(disk.parent().unwrap())?.sync_all()?;
        return Ok(stable);
    }
    for entry in fs::read_dir(disk.parent().unwrap())? {
        if entry?
            .file_name()
            .as_bytes()
            .starts_with(b".bridgevm-pair-")
        {
            return Err(io::Error::other(
                "unbound managed media exists; refusing stale-original fallback",
            ));
        }
    }
    Ok(stable)
}

#[cfg(test)]
#[path = "managed_pair_identity_tests.rs"]
mod tests;

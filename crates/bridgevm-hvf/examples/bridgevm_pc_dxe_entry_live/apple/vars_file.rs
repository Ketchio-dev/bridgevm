use super::aligned_memory::PAGE_ALIGNMENT;
use super::command::Expectation;
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

pub(super) fn load(path: &Path, expectation: Expectation) -> Result<Vec<u8>, String> {
    match expectation {
        Expectation::Written => match fs::symlink_metadata(path) {
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(vec![0xff; PAGE_ALIGNMENT]),
            Err(error) => Err(format!("inspect vars target: {error}")),
            Ok(_) => Err("written-mode vars target already exists".to_string()),
        },
        Expectation::Restored => read_existing(path),
    }
}

pub(super) fn read_existing(path: &Path) -> Result<Vec<u8>, String> {
    let mut options = OpenOptions::new();
    options.read(true).custom_flags(libc::O_NOFOLLOW);
    let mut file = options
        .open(path)
        .map_err(|error| format!("open vars file without following links: {error}"))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("inspect opened vars file: {error}"))?;
    if !metadata.file_type().is_file() || metadata.len() != PAGE_ALIGNMENT as u64 {
        return Err(format!(
            "vars file must be a regular {PAGE_ALIGNMENT}-byte file"
        ));
    }
    let mut bytes = Vec::with_capacity(PAGE_ALIGNMENT);
    file.read_to_end(&mut bytes)
        .map_err(|error| format!("read vars file: {error}"))?;
    if bytes.len() != PAGE_ALIGNMENT {
        return Err("vars file length changed while it was read".to_string());
    }
    Ok(bytes)
}

pub(super) fn persist_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() != PAGE_ALIGNMENT {
        return Err("refusing to persist a vars backing with the wrong size".to_string());
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| "vars target has no file name".to_string())?;
    let (temporary, mut file) = create_temporary(parent, name)?;
    let result = (|| {
        file.write_all(bytes)
            .map_err(|error| format!("write temporary vars file: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("sync temporary vars file: {error}"))?;
        fs::hard_link(&temporary, path)
            .map_err(|error| format!("create vars target without overwrite: {error}"))?;
        fs::remove_file(&temporary)
            .map_err(|error| format!("remove temporary vars link: {error}"))?;
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| format!("sync vars directory: {error}"))
    })();
    let _ = fs::remove_file(&temporary);
    result
}

fn create_temporary(parent: &Path, name: &std::ffi::OsStr) -> Result<(PathBuf, File), String> {
    for attempt in 0..64 {
        let mut temporary_name = name.to_os_string();
        temporary_name.push(format!(".bridgevm-{}.{}.tmp", std::process::id(), attempt));
        let temporary = parent.join(temporary_name);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true).mode(0o600);
        match options.open(&temporary) {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("create temporary vars file: {error}")),
        }
    }
    Err("could not allocate a unique temporary vars file".to_string())
}

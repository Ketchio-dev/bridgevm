use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

pub(super) fn open(path: &Path) -> Result<(File, Vec<u8>), String> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOFOLLOW);
    let mut file = options
        .open(path)
        .map_err(|error| format!("open vars file without following links: {error}"))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("inspect vars file: {error}"))?;
    if !metadata.file_type().is_file() {
        return Err("vars target is not a regular file".to_string());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.read_to_end(&mut bytes)
        .map_err(|error| format!("read vars file: {error}"))?;
    Ok((file, bytes))
}

pub(super) fn persist(file: &mut File, bytes: &[u8]) -> Result<(), String> {
    let metadata = file
        .metadata()
        .map_err(|error| format!("reinspect vars file: {error}"))?;
    if !metadata.file_type().is_file() || metadata.len() != bytes.len() as u64 {
        return Err("vars file changed type or size during the VM run".to_string());
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|error| format!("seek vars file: {error}"))?;
    file.write_all(bytes)
        .map_err(|error| format!("persist vars file: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("sync vars file: {error}"))
}

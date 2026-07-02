use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{ErrorKind, Write},
    path::Path,
};

pub(crate) fn reject_symlinked_evidence_component(
    evidence_dir: &Path,
    relative_path: &Path,
) -> Result<()> {
    reject_symlinked_component(evidence_dir)?;
    let mut current = evidence_dir.to_path_buf();
    for component in relative_path.components() {
        current.push(component.as_os_str());
        reject_symlinked_component(&current)?;
    }
    Ok(())
}

pub(crate) fn write_new_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("failed to create evidence file {}", path.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("failed to write evidence file {}", path.display()))
}

pub(crate) fn sha256_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let digest = Sha256::digest(&bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    Ok(hex)
}

fn reject_symlinked_component(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => bail!(
            "Task 5af evidence path component {} is a symlink; refusing to write row evidence",
            path.display()
        ),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| {
            format!(
                "failed to inspect Task 5af evidence path component {}",
                path.display()
            )
        }),
    }
}

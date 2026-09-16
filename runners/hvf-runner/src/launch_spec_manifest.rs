//! Legacy manifest input and the owned mode's bounded ordinary-file admission.
use anyhow::{bail, Context, Result};
use std::{fs::OpenOptions, io::Read, os::unix::fs::OpenOptionsExt};
pub(super) fn read_text(spec: &str) -> Result<String> {
    let text = if spec == "-" {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .context("read launch manifest from stdin")?;
        text
    } else {
        std::fs::read_to_string(spec).with_context(|| format!("read launch manifest {spec}"))?
    };
    Ok(text)
}
pub(super) fn read_owned(path: &str) -> Result<Vec<u8>> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > 65536 {
        bail!("owned manifest must be a bounded regular file");
    }
    let mut bytes = Vec::new();
    (&mut file).take(65537).read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        bail!("owned manifest exceeds limit");
    }
    Ok(bytes)
}

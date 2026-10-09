//! Stable cooperative path and inode keys.
use super::*;

pub(super) fn resource_keys(path: &Path) -> io::Result<Vec<String>> {
    let canonical = match fs::canonicalize(path) {
        Ok(path) => path,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let name = path
                .file_name()
                .ok_or_else(|| io::Error::other("media path has no name"))?;
            fs::canonicalize(
                path.parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or(Path::new(".")),
            )?
            .join(name)
        }
        Err(error) => return Err(error),
    };
    let mut bytes = b"path:".to_vec();
    bytes.extend_from_slice(canonical.as_os_str().as_bytes());
    let mut keys = vec![digest_key(&bytes)];
    match fs::metadata(&canonical) {
        Ok(metadata) if metadata.is_file() => {
            let identity = format!("inode:{}:{}", metadata.dev(), metadata.ino());
            keys.push(digest_key(identity.as_bytes()));
        }
        Ok(_) => return Err(io::Error::other("media is not a regular file")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    Ok(keys)
}

fn digest_key(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

//! Map portable operative paths to owned copies without opening old roots.

use crate::*;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub(crate) fn invalid(message: impl Into<String>) -> StorageError {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message.into()).into()
}

pub(crate) struct ImportPaths {
    pub(crate) staging: PathBuf,
    pub(crate) output: PathBuf,
    roots: Vec<PathBuf>,
}

impl ImportPaths {
    pub(crate) fn new(input: &Path, staging: &Path, output: &Path) -> Result<Self, StorageError> {
        let mut roots = vec![absolutize(input.to_path_buf()), fs::canonicalize(input)?];
        if let Some(export) =
            read_json_file::<VmExportMetadata>(&staging.join("metadata/export.json"))?
        {
            if !export.source.is_absolute() || export.source.parent().is_none() {
                return Err(invalid("invalid export source root for portable import"));
            }
            roots.push(clean_absolute(&export.source)?);
        }
        roots.sort();
        roots.dedup();
        Ok(Self {
            staging: staging.to_path_buf(),
            output: output.to_path_buf(),
            roots,
        })
    }

    pub(crate) fn relative(&self, path: &Path) -> Result<PathBuf, StorageError> {
        if path.is_absolute() {
            let path = clean_absolute(path)?;
            let mut matches = self
                .roots
                .iter()
                .filter_map(|root| path.strip_prefix(root).ok())
                .map(Path::to_path_buf)
                .collect::<Vec<_>>();
            matches.sort();
            matches.dedup();
            if matches.len() != 1 {
                return Err(invalid(format!(
                    "unresolved or ambiguous bundle import path: {}",
                    path.display()
                )));
            }
            safe_relative(&matches[0])
        } else {
            safe_relative(path)
        }
    }

    pub(crate) fn relocate(&self, path: &Path) -> Result<PathBuf, StorageError> {
        Ok(self.output.join(self.relative(path)?))
    }

    pub(crate) fn backing(&self, disk: &Path, backing: &Path) -> Result<PathBuf, StorageError> {
        if backing.is_absolute() {
            self.relative(backing)
        } else {
            let parent = disk
                .parent()
                .ok_or_else(|| invalid("disk has no bundle parent"))?;
            let absolute = clean_absolute(&self.staging.join(parent).join(backing))?;
            let relative = absolute
                .strip_prefix(&self.staging)
                .map_err(|_| invalid("qcow2 backing escapes imported bundle"))?;
            safe_relative(relative)
        }
    }

    pub(crate) fn exists(&self, original: &Path) -> Result<bool, StorageError> {
        Ok(self.staging.join(self.relative(original)?).is_file())
    }
}

pub(crate) fn safe_relative(path: &Path) -> Result<PathBuf, StorageError> {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => result.push(value),
            Component::CurDir => {}
            _ => return Err(invalid("portable bundle path must not escape its root")),
        }
    }
    if result.as_os_str().is_empty() {
        return Err(invalid("empty portable bundle path"));
    }
    Ok(result)
}

fn clean_absolute(path: &Path) -> Result<PathBuf, StorageError> {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir | Component::Normal(_) => result.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() {
                    return Err(invalid("bundle path traverses above filesystem root"));
                }
            }
            Component::Prefix(_) => return Err(invalid("unsupported portable path prefix")),
        }
    }
    Ok(result)
}

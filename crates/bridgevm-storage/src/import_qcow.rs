//! Validate a copied disk graph before changing only owned qcow2 headers.

use crate::import_image_info::{command, info};
use crate::import_metadata::DiskFiles;
use crate::import_paths::{invalid, ImportPaths};
use crate::*;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const MAX_DISKS: usize = 1024;
const MAX_DEPTH: usize = 64;

fn collect(directory: &Path, root: &Path, files: &mut DiskFiles) -> Result<(), StorageError> {
    if !directory.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(&path, root, files)?;
        } else if path.extension().and_then(|v| v.to_str()) == Some("qcow2") {
            files
                .formats
                .entry(path.strip_prefix(root).unwrap().to_path_buf())
                .or_insert("qcow2".into());
        }
    }
    Ok(())
}

fn check_graph(
    node: &Path,
    links: &BTreeMap<PathBuf, PathBuf>,
    visited: &mut BTreeSet<PathBuf>,
) -> Result<(), StorageError> {
    if visited.len() >= MAX_DEPTH || !visited.insert(node.to_path_buf()) {
        return Err(invalid("cyclic or excessively deep imported backing graph"));
    }
    if let Some(backing) = links.get(node) {
        check_graph(backing, links, visited)?;
    }
    visited.remove(node);
    Ok(())
}

fn relative_backing(disk: &Path, backing: &Path) -> PathBuf {
    let parent = disk.parent().unwrap();
    let left = parent.components().collect::<Vec<_>>();
    let right = backing.components().collect::<Vec<_>>();
    let common = left.iter().zip(&right).take_while(|(a, b)| a == b).count();
    let mut relative = PathBuf::new();
    for _ in common..left.len() {
        relative.push("..");
    }
    for component in &right[common..] {
        relative.push(component.as_os_str());
    }
    relative
}

pub(crate) fn relocate(paths: &ImportPaths, mut files: DiskFiles) -> Result<(), StorageError> {
    collect(&paths.staging.join("disks"), &paths.staging, &mut files)?;
    let mut inspected = BTreeMap::new();
    let mut links = BTreeMap::new();
    loop {
        if files.formats.len() > MAX_DISKS {
            return Err(invalid("too many imported disk dependencies"));
        }
        let next = files
            .formats
            .iter()
            .find(|(path, _)| !inspected.contains_key(*path))
            .map(|(p, f)| (p.clone(), f.clone()));
        let Some((path, format)) = next else {
            break;
        };
        let owned = paths.staging.join(&path);
        if !owned.exists() {
            inspected.insert(path, Value::Null);
            continue;
        }
        if !matches!(format.as_str(), "raw" | "qcow2") {
            return Err(invalid("unsupported portable disk format"));
        }
        if format == "qcow2" {
            crate::import_qcow_header::validate(&owned)?;
        }
        let value = info(&owned, &format)?;
        if let Some(backing) = value["backing-filename"].as_str() {
            let target = paths.backing(&path, Path::new(backing))?;
            if !paths.staging.join(&target).is_file() {
                return Err(invalid("qcow2 backing is not a copied bundle file"));
            }
            let backing_format = value["backing-filename-format"]
                .as_str()
                .ok_or_else(|| invalid("qcow2 backing has no explicit format"))?;
            if let Some(expected) = files.formats.get(&target) {
                if expected != backing_format {
                    return Err(invalid("qcow2 backing format disagrees with copied graph"));
                }
            } else {
                files.formats.insert(target.clone(), backing_format.into());
            }
            links.insert(path.clone(), target);
        }
        if let Some(expected) = files.backings.get(&path) {
            if links.get(&path) != Some(expected) {
                return Err(invalid(
                    "qcow2 header disagrees with snapshot backing identity",
                ));
            }
        }
        inspected.insert(path, value);
    }
    for node in links.keys() {
        check_graph(node, &links, &mut BTreeSet::new())?;
    }
    // Every target was byte-compared to its input before any header mutation.
    // Only the name changes: never open a former absolute backing dependency.
    for (disk, backing) in &links {
        command(vec![
            "rebase".into(),
            "-u".into(),
            "-f".into(),
            "qcow2".into(),
            "-F".into(),
            files.formats[backing].clone(),
            "-b".into(),
            relative_backing(disk, backing).display().to_string(),
            paths.staging.join(disk).display().to_string(),
        ])?;
    }
    for (disk, before) in inspected {
        if before.is_null() || files.formats[&disk] != "qcow2" {
            continue;
        }
        let owned = paths.staging.join(&disk);
        let after = info(&owned, "qcow2")?;
        let after_backing = after["backing-filename"]
            .as_str()
            .map(|v| paths.backing(&disk, Path::new(v)))
            .transpose()?;
        if before["virtual-size"] != after["virtual-size"]
            || after_backing.as_ref() != links.get(&disk)
        {
            return Err(invalid("relocated qcow2 graph identity changed"));
        }
        command(vec![
            "check".into(),
            "-f".into(),
            "qcow2".into(),
            "--output=json".into(),
            owned.display().to_string(),
        ])?;
    }
    Ok(())
}

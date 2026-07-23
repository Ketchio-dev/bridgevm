//! Split out of lib.rs by responsibility.

use crate::*;

pub fn list_shares(store: &VmStore, name: &str) -> Result<SharedFolderListRecord, String> {
    let (_, manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    Ok(shared_folder_list(name, &manifest.shared_folders))
}

pub fn add_share(
    store: &VmStore,
    name: &str,
    share: String,
    host_path: String,
    read_only: bool,
    host_path_token: Option<String>,
) -> Result<SharedFolderListRecord, String> {
    let (bundle, mut manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    manifest.shared_folders.push(SharedFolder {
        name: share,
        host_path,
        read_only,
        host_path_token,
    });
    manifest.validate().map_err(|error| error.to_string())?;
    if let Some(folder) = manifest.shared_folders.last_mut() {
        folder.host_path = canonical_share_host_path(&folder.host_path)?;
    }
    manifest
        .shared_folders
        .sort_by(|left, right| left.name.cmp(&right.name));
    manifest
        .write(&bundle.join("manifest.yaml"))
        .map_err(|error| error.to_string())?;
    Ok(shared_folder_list(name, &manifest.shared_folders))
}

pub(crate) fn canonical_share_host_path(host_path: &str) -> Result<String, String> {
    let requested = Path::new(host_path);
    if !requested.is_absolute() {
        return Err("shared folder hostPath must be an absolute path".to_string());
    }
    if requested
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("shared folder hostPath cannot contain '..' components".to_string());
    }
    reject_symlink_components(requested)?;
    let canonical = std::fs::canonicalize(host_path).map_err(|error| {
        format!("shared folder hostPath '{host_path}' is not accessible: {error}")
    })?;
    if !canonical.is_dir() {
        return Err(format!(
            "shared folder hostPath '{}' must be an existing directory",
            canonical.display()
        ));
    }
    canonical
        .into_os_string()
        .into_string()
        .map_err(|_| "shared folder hostPath must be valid UTF-8".to_string())
}

pub(crate) fn reject_symlink_components(path: &Path) -> Result<(), String> {
    let mut current = PathBuf::new();
    let mut normal_components = 0usize;
    for component in path.components() {
        current.push(component.as_os_str());
        if matches!(component, Component::Normal(_)) {
            normal_components += 1;
        }
        let metadata = fs::symlink_metadata(&current).map_err(|error| {
            format!(
                "shared folder hostPath '{}' is not accessible: {error}",
                path.display()
            )
        })?;
        if metadata.file_type().is_symlink() {
            if normal_components <= 1 {
                continue;
            }
            return Err(format!(
                "shared folder hostPath '{}' cannot traverse symlink '{}'",
                path.display(),
                current.display()
            ));
        }
    }
    Ok(())
}

pub fn remove_share(
    store: &VmStore,
    name: &str,
    share: &str,
) -> Result<SharedFolderListRecord, String> {
    let (bundle, mut manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    let initial_len = manifest.shared_folders.len();
    manifest
        .shared_folders
        .retain(|folder| folder.name != share);
    if manifest.shared_folders.len() == initial_len {
        return Err(format!("shared folder '{share}' is not configured"));
    }
    manifest
        .write(&bundle.join("manifest.yaml"))
        .map_err(|error| error.to_string())?;
    Ok(shared_folder_list(name, &manifest.shared_folders))
}

pub(crate) fn shared_folder_list(
    name: &str,
    shared_folders: &[SharedFolder],
) -> SharedFolderListRecord {
    SharedFolderListRecord {
        vm: name.to_string(),
        shared_folders: shared_folders
            .iter()
            .map(|folder| SharedFolderRecord {
                name: folder.name.clone(),
                host_path: folder.host_path.clone(),
                read_only: folder.read_only,
                host_path_token: folder.resolved_host_path_token(),
            })
            .collect(),
    }
}

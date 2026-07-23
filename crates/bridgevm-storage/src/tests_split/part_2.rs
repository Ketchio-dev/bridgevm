//! Split test module.

use crate::*;
use std::fs;

use super::helpers::*;

#[test]
fn metadata_only_delete_preserves_bundle_manifest_and_hides_vm() {
    let store = temp_store();
    let bundle = store.create_vm(&manifest("dev")).unwrap();
    fs::write(bundle.join("disks").join("primary.img"), "disk").unwrap();
    fs::write(bundle.join("logs").join("serial.log"), "log").unwrap();

    let deletion = store.delete_vm_metadata_only("dev").unwrap();

    assert_eq!(deletion.vm, "dev");
    assert!(deletion.metadata_only);
    assert_eq!(deletion.bundle, bundle);
    assert!(bundle.exists());
    assert!(bundle.join("manifest.yaml").exists());
    assert!(bundle.join("disks").join("primary.img").exists());
    assert!(bundle.join("logs").join("serial.log").exists());
    assert!(bundle
        .join("metadata")
        .join("deleted-manifest.yaml")
        .exists());
    assert!(bundle.join("metadata").join("deletion.json").exists());
    assert!(store.list_vms().unwrap().is_empty());
    assert!(matches!(
        store.get_vm("dev").unwrap_err(),
        StorageError::NotFound(name) if name == "dev"
    ));
}

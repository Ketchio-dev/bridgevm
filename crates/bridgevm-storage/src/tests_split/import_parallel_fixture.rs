//! Tiny independent archives avoid involving the separately repaired export path.
use super::helpers::{manifest, temp_store};
use crate::*;
use std::fs;
use std::path::PathBuf;

pub(super) struct Fixture {
    pub root: PathBuf,
    pub archives: Vec<(PathBuf, Vec<u8>)>,
}
impl Fixture {
    pub(super) fn new() -> Self {
        let root = temp_store().root().to_path_buf();
        fs::create_dir(&root).unwrap();
        let mut archives = Vec::new();
        for index in 0..4 {
            let name = format!("original-{index}");
            let mut config = manifest(&name);
            config.storage.primary.path = "disks/root.raw".into();
            config.storage.primary.format = "raw".into();
            let path = root.join(format!("input-{index}.tar"));
            let mut builder = tar::Builder::new(fs::File::create(&path).unwrap());
            let yaml = serde_yaml::to_string(&config).unwrap();
            let marker = format!("metadata/only-{index}.txt");
            for (name, bytes) in [
                ("manifest.yaml", yaml.as_bytes()),
                ("disks/root.raw", name.as_bytes()),
                ("metadata/origin.txt", name.as_bytes()),
                (marker.as_str(), name.as_bytes()),
            ] {
                let mut header = tar::Header::new_gnu();
                header.set_size(bytes.len() as u64);
                header.set_mode(0o600);
                header.set_cksum();
                builder.append_data(&mut header, name, bytes).unwrap();
            }
            builder.finish().unwrap();
            archives.push((path.clone(), fs::read(path).unwrap()));
        }
        Self { root, archives }
    }

    pub(super) fn target(&self, round: usize, index: usize) -> VmStore {
        let target = VmStore::new(self.root.join(format!("target-{round}-{index}")));
        target.ensure().unwrap();
        target
    }

    pub(super) fn verify(&self, index: usize, metadata: &VmImportMetadata) {
        let expected = format!("original-{index}");
        assert_eq!(metadata.original_name, expected);
        assert_eq!(metadata.source, self.archives[index].0);
        assert_eq!(
            fs::read(metadata.output.join("disks/root.raw")).unwrap(),
            expected.as_bytes()
        );
        assert_eq!(
            fs::read(metadata.output.join("metadata/origin.txt")).unwrap(),
            expected.as_bytes()
        );
        let mut expected_files = vec![
            "disks/root.raw".to_string(),
            "manifest.yaml".to_string(),
            format!("metadata/only-{index}.txt"),
            "metadata/origin.txt".to_string(),
        ];
        expected_files.sort();
        let mut actual = metadata.copied_files.clone();
        actual.sort();
        assert_eq!(actual, expected_files);
    }

    pub(super) fn verify_sources(&self) {
        for (path, bytes) in &self.archives {
            assert_eq!(&fs::read(path).unwrap(), bytes);
        }
    }
}

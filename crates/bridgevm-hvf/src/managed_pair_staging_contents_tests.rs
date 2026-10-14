use super::*;
use crate::snapshot_pair::managed::private_directory;
use crate::snapshot_pair::snapshot_pair_tests::Scratch;
use std::os::unix::fs::symlink;

#[test]
fn unfamiliar_entries_preserve_all_staging_contents() {
    for kind in ["foreign-file", "nested-directory", "member-symlink"] {
        let scratch = Scratch::new(&format!("managed-stage-shape-{kind}"));
        let staged = scratch.path("staging");
        private_directory(&staged, true).unwrap();
        fs::write(staged.join("disk.raw"), b"known partial copy").unwrap();
        let foreign = scratch.write("outside", b"foreign bytes");
        let name = match kind {
            "foreign-file" => {
                fs::write(staged.join("keep.txt"), b"keep").unwrap();
                "keep.txt"
            }
            "nested-directory" => {
                fs::create_dir(staged.join("vars.fd")).unwrap();
                "vars.fd"
            }
            _ => {
                symlink(&foreign, staged.join("vars.fd")).unwrap();
                "vars.fd"
            }
        };
        let before = fs::symlink_metadata(staged.join(name)).unwrap().file_type();
        let error = clear(&staged).unwrap_err();
        assert!(error.to_string().contains("it was left intact"));
        assert_eq!(
            fs::read(staged.join("disk.raw")).unwrap(),
            b"known partial copy"
        );
        assert_eq!(
            fs::symlink_metadata(staged.join(name)).unwrap().file_type(),
            before
        );
        assert_eq!(fs::read(&foreign).unwrap(), b"foreign bytes");
    }
}

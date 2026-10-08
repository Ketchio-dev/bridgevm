use super::super::NvmeController;
use super::export_staging::Fixture;
use std::os::unix::fs::{symlink, MetadataExt};
use std::{fs, process::Command};

#[test]
fn occupied_generated_names_are_never_adopted() {
    const CHILD: &str = "BRIDGEVM_TEST_NVME_EXPORT_COLLISION_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let status = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "nvme::tests::export_collision::occupied_generated_names_are_never_adopted",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    let f = Fixture::new();
    let candidate = |n| {
        f.0.join(format!(
            ".bridgevm-nvme-export-{}-{n}.tmp",
            std::process::id()
        ))
    };
    let sentinel = f.0.join("sentinel");
    fs::write(&sentinel, b"foreign-sentinel").unwrap();
    fs::write(candidate(0), b"foreign-ordinary").unwrap();
    fs::hard_link(&sentinel, candidate(1)).unwrap();
    symlink(f.0.join("missing"), candidate(2)).unwrap();
    let before: Vec<_> = (0..3)
        .map(|n| fs::symlink_metadata(candidate(n)).unwrap().ino())
        .collect();
    let mut controller = NvmeController::with_disk_image(vec![0xe1; 512]);
    let output = f.0.join("output.raw");
    controller.export_disk_image(&output).unwrap();
    assert_eq!(fs::read(&output).unwrap(), vec![0xe1; 512]);
    assert_eq!(fs::read(candidate(0)).unwrap(), b"foreign-ordinary");
    assert_eq!(fs::read(&sentinel).unwrap(), b"foreign-sentinel");
    assert_eq!(fs::read_link(candidate(2)).unwrap(), f.0.join("missing"));
    for (n, inode) in before.iter().enumerate() {
        assert_eq!(fs::symlink_metadata(candidate(n)).unwrap().ino(), *inode);
    }
    assert!(!candidate(3).exists());
}

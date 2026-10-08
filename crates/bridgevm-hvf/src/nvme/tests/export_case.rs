use super::super::NvmeController;
use super::export_staging::Fixture;
use std::{fs, process::Command};

#[test]
fn uppercase_destination_is_never_used_as_temporary_file() {
    const CHILD: &str = "BRIDGEVM_TEST_NVME_EXPORT_CASE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let status = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "nvme::tests::export_case::uppercase_destination_is_never_used_as_temporary_file",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    // First allocator use in this exact child fixes the generated sequence at zero.
    let f = Fixture::new();
    let name = format!(".bridgevm-nvme-export-{}-0.tmp", std::process::id());
    let lower = f.0.join(&name);
    let destination = f.0.join(name.to_ascii_uppercase());
    fs::write(&lower, b"case-probe").unwrap();
    let case_aliases = destination.exists();
    fs::remove_file(&lower).unwrap();
    let image = vec![0xb1; 512];
    let mut controller = NvmeController::with_disk_image(image.clone());
    controller.export_disk_image(&destination).unwrap();
    assert!(
        destination.exists(),
        "reported success deleted destination; case_aliases={case_aliases}"
    );
    assert_eq!(fs::read(destination).unwrap(), image);
}

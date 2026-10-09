use super::*;
use std::process::Command;

#[test]
fn uppercase_destination_is_not_runtime_staging() {
    const CHILD: &str = "BRIDGEVM_TEST_RUNTIME_STAGING_CASE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let status = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "snapshot_pair::managed::runtime::tests::persistence::case::uppercase_destination_is_not_runtime_staging", "--nocapture"])
            .env(CHILD, "1").status().unwrap();
        assert!(status.success());
        return;
    }
    let s = Scratch::new("runtime-staging-case");
    let name = format!(".bridgevm-write-{}-0.tmp", std::process::id());
    let lower = s.path(&name);
    let output = s.path(&name.to_ascii_uppercase());
    fs::write(&lower, b"case-probe").unwrap();
    let case_aliases = output.exists();
    fs::remove_file(lower).unwrap();
    let mut media = VirtBootMediaConfig::qemu_defaults();
    media.flash_vars = WritableMedia::new(s.path("unused-vars")).with_snapshot_path(Some(&output));
    let mut guard = acquire(&mut media).unwrap();
    guard
        .persist(RuntimeMediaSlot::Vars, b"saved-vars")
        .unwrap();
    assert!(
        output.exists(),
        "reported success lost output; case_aliases={case_aliases}"
    );
    assert_eq!(fs::read(output).unwrap(), b"saved-vars");
}

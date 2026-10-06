//! Process exit around a real empty-directory rename, not a power-loss test.

use super::*;
use crate::snapshot_pair::staging_path;
use std::path::PathBuf;
use std::process::Command;

const CASE_ENV: &str = "BRIDGEVM_TEST_EMPTY_PUBLISH_CRASH";
const ROOT_ENV: &str = "BRIDGEVM_TEST_EMPTY_PUBLISH_ROOT";
const CASES: [(&str, i32); 2] = [("before-rename", 96), ("after-rename", 97)];

#[test]
fn empty_rename_crash_child() {
    let Ok(case) = std::env::var(CASE_ENV) else {
        return;
    };
    let exit = CASES.iter().find(|(name, _)| *name == case).unwrap().1;
    let root = PathBuf::from(std::env::var_os(ROOT_ENV).expect("owned fixture root"));
    let dest = root.join("dest");
    publish_using(
        &staging_path(&dest),
        &dest,
        |_, _| panic!("unexpected exchange"),
        |a, b| {
            if case == "before-rename" {
                std::process::exit(exit);
            }
            fs::rename(a, b)?;
            std::process::exit(exit)
        },
        crate::snapshot_pair::sync_dir,
    )
    .unwrap();
    panic!("selected process-exit boundary must not return");
}

#[test]
fn hard_exit_before_and_after_empty_rename_preserves_pair_and_retry() {
    for (case, exit) in CASES {
        let scratch = Scratch::new(&format!("publish-empty-crash-{case}"));
        let disk = scratch.write("source-disk", b"new-disk");
        let vars = scratch.write("source-vars", b"new-vars");
        let dest = scratch.path("dest");
        let stage = staging_path(&dest);
        fs::create_dir(&dest).unwrap();
        let new = create_snapshot(&disk, &vars, &stage, "new", false, 1024).unwrap();
        let status = Command::new(std::env::current_exe().unwrap())
            .arg("snapshot_pair::snapshot_publish::tests::empty_destination_tests::interruption_tests::empty_rename_crash_child")
            .args(["--exact", "--nocapture"])
            .env(CASE_ENV, case).env(ROOT_ENV, &scratch.0).status().unwrap();
        assert_eq!(status.code(), Some(exit));
        if case == "before-rename" {
            assert_eq!(fs::read_dir(&dest).unwrap().count(), 0);
            assert_eq!(verify_snapshot(&stage).unwrap(), new);
        } else {
            assert_eq!(verify_snapshot(&dest).unwrap(), new);
            assert!(!stage.exists());
        }
        let retry = create_snapshot(&disk, &vars, &dest, "retry", false, 1024).unwrap();
        assert_eq!(verify_snapshot(&dest).unwrap(), retry);
        assert!(!stage.exists());
        assert_eq!(fs::read(&disk).unwrap(), b"new-disk");
        assert_eq!(fs::read(&vars).unwrap(), b"new-vars");
    }
}

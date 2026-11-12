#![cfg(target_os = "macos")]
#[path = "../examples/hvf_gic_boot_probe/parent_watch.rs"]
mod parent_watch;
use std::{
    fs,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[test]
#[allow(clippy::zombie_processes)] // The test deliberately SIGKILLs this parent; launchd reaps its child.
fn worker() {
    let Ok(root) = std::env::var("BV_PARENT_WATCH_TEST") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    if std::env::var_os("BV_WATCH_CHILD").is_none() {
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "worker", "--nocapture"])
            .env("BV_WATCH_CHILD", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        fs::write(root.join("pid"), child.id().to_string()).unwrap();
        loop {
            thread::sleep(Duration::from_millis(10));
        }
    }
    fs::write(root.join("ready"), "ready").unwrap();
    if std::env::var_os("BV_WATCH_RACE").is_some() {
        while !root.join("go").exists() {
            thread::sleep(Duration::from_millis(5));
        }
    }
    let exited = parent_watch::wait_for_runner_exit();
    fs::write(
        root.join("hook"),
        if exited { "shutdown" } else { "unwatched" },
    )
    .unwrap();
}

fn wait(path: &std::path::Path) {
    let until = Instant::now() + Duration::from_secs(3);
    while !path.exists() && Instant::now() < until {
        thread::sleep(Duration::from_millis(5));
    }
    assert!(path.exists(), "timed out waiting for {}", path.display());
}
fn run(marked: bool, race: bool) {
    let root = std::env::temp_dir().join(format!(
        "bv-parent-watch-{}-{}-{race}",
        std::process::id(),
        marked
    ));
    fs::create_dir_all(&root).unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "worker", "--nocapture"])
        .env("BV_PARENT_WATCH_TEST", &root)
        .env_remove("BRIDGEVM_RESET_GENERATION")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if marked {
        command.env("BRIDGEVM_RESET_GENERATION", "0");
    }
    if race {
        command.env("BV_WATCH_RACE", "1");
    }
    let mut parent = command.spawn().unwrap();
    wait(&root.join("pid"));
    wait(&root.join("ready"));
    let child: i32 = fs::read_to_string(root.join("pid"))
        .unwrap()
        .parse()
        .unwrap();
    // SAFETY: this PID is the exact parent retained by this test.
    unsafe {
        libc::kill(parent.id() as i32, libc::SIGKILL);
    }
    parent.wait().unwrap();
    fs::write(root.join("go"), "go").unwrap();
    let until = Instant::now() + Duration::from_secs(3);
    while !root.join("hook").exists() && Instant::now() < until {
        thread::sleep(Duration::from_millis(5));
    }
    let hook = fs::read_to_string(root.join("hook")).unwrap_or_default();
    let until = Instant::now() + Duration::from_secs(3);
    // SAFETY: observation only; this is the child this test spawned.
    while unsafe { libc::kill(child, 0) } == 0 && Instant::now() < until {
        thread::sleep(Duration::from_millis(5));
    }
    let exited = unsafe { libc::kill(child, 0) } != 0;
    if !exited {
        // SAFETY: cleanup of the test's retained child on failure.
        unsafe {
            libc::kill(child, libc::SIGKILL);
        }
    }
    fs::remove_dir_all(root).unwrap();
    assert!(exited, "shutdown hook must be followed by process exit");
    assert_eq!(hook, if marked { "shutdown" } else { "unwatched" });
}
#[test]
fn runner_sigkill_runs_shutdown_hook() {
    run(true, false);
}
#[test]
fn already_gone_parent_runs_shutdown_hook() {
    run(true, true);
}
#[test]
fn direct_launch_does_not_watch_parent() {
    run(false, false);
}

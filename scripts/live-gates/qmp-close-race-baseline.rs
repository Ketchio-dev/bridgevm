//! Deterministic negative control for the one-shot QMP fake-server close race.

use std::fs;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::Duration;

fn main() {
    let iterations: usize = std::env::args().nth(1).unwrap().parse().unwrap();
    let mut matched = 0;
    for iteration in 0..iterations {
        let path = PathBuf::from(format!(
            "/tmp/bvqmp-baseline-{}-{iteration}.sock",
            std::process::id()
        ));
        let _ = fs::remove_file(&path);
        let listener = UnixListener::bind(&path).unwrap();
        let client = UnixStream::connect(&path).unwrap();
        let server = std::thread::spawn(move || drop(listener.accept().unwrap()));
        server.join().unwrap();
        let first = client
            .set_read_timeout(Some(Duration::from_secs(30)))
            .unwrap_err();
        let second = UnixStream::connect(&path).unwrap_err();
        let _ = fs::remove_file(&path);
        matched +=
            usize::from(first.raw_os_error() == Some(22) && second.raw_os_error() == Some(61));
    }
    println!("baseline_iterations={iterations}");
    println!("baseline_einval_then_econnrefused={matched}");
    assert_eq!(matched, iterations);
}

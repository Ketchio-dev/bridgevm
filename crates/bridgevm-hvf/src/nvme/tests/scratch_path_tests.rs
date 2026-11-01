use super::{path_at, temp_path};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Barrier};

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let path = temp_path("same-tick-contract");
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn same_tick_concurrent_callers_own_disjoint_files() {
    const CALLERS: usize = 32;
    const FIXED_NANOS: u128 = 123_456_789;
    const NAMES: [&str; 4] = [
        "shutdown-primary",
        "shutdown-secondary",
        "flush-primary",
        "flush-secondary",
    ];
    let directory = Directory::new();
    let barrier = Arc::new(Barrier::new(CALLERS));
    let callers: Vec<_> = (0..CALLERS)
        .map(|owner| {
            let (directory, barrier) = (directory.0.clone(), barrier.clone());
            std::thread::spawn(move || {
                barrier.wait();
                NAMES.map(|name| (owner, path_at(&directory, name, FIXED_NANOS)))
            })
        })
        .collect();
    let files: Vec<_> = callers
        .into_iter()
        .flat_map(|caller| caller.join().unwrap())
        .collect();
    for (owner, path) in &files {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .expect("same-tick callers must never share a scratch path");
        file.write_all(&owner.to_le_bytes()).unwrap();
    }
    for (_, path) in files.iter().filter(|(owner, _)| owner % 2 == 0) {
        fs::remove_file(path).unwrap();
    }
    for (owner, path) in files.iter().filter(|(owner, _)| owner % 2 != 0) {
        assert_eq!(fs::read(path).unwrap(), owner.to_le_bytes());
    }
}

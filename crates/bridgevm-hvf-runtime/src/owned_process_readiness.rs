//! Failure-only context, captured before the existing fixture and child cleanup.
use super::Fixture;
use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;
use std::time::{Duration, Instant};

const STDERR_LIMIT: u64 = 4096;

impl Fixture {
    pub(crate) fn stderr(&self) -> File {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(self.root.join("stderr"))
            .unwrap()
    }
    pub(crate) fn wait_file(&self, name: &str) {
        self.wait_file_context(name, || "child status not sampled".into());
    }
    pub(crate) fn wait_file_with(
        &self,
        name: &str,
        status: impl FnOnce() -> io::Result<Option<ExitStatus>>,
    ) {
        self.wait_file_context(name, || match status() {
            Ok(Some(exit)) => format!(
                "child exited code={:?} signal={:?}",
                exit.code(),
                exit.signal()
            ),
            Ok(None) => "child still running at nonblocking observation".into(),
            Err(error) => format!(
                "child status error kind={:?} errno={:?}",
                error.kind(),
                error.raw_os_error()
            ),
        });
    }
    fn wait_file_context(&self, name: &str, context: impl FnOnce() -> String) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !self.root.join(name).exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            self.root.join(name).exists(),
            "fixture readiness deadline: {}; {}",
            context(),
            self.readiness_context(name)
        );
    }
    fn readiness_context(&self, name: &str) -> String {
        let metadata = |path: &std::path::Path| match path.symlink_metadata() {
            Ok(value) => format!(
                "dev={} ino={} mode={:o} size={}",
                value.dev(),
                value.ino(),
                value.mode(),
                value.len()
            ),
            Err(error) => format!("kind={:?} errno={:?}", error.kind(), error.raw_os_error()),
        };
        format!(
            "root={:?} [{}]; marker={:?} [{}]; stderr={}",
            self.root,
            metadata(&self.root),
            name,
            metadata(&self.root.join(name)),
            self.stderr_context()
        )
    }
    fn stderr_context(&self) -> String {
        let read = || -> io::Result<(Vec<u8>, u64)> {
            let file = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                .open(self.root.join("stderr"))?;
            let metadata = file.metadata()?;
            if !metadata.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "not a regular file",
                ));
            }
            let mut bytes = Vec::new();
            file.take(STDERR_LIMIT + 1).read_to_end(&mut bytes)?;
            Ok((bytes, metadata.len()))
        };
        match read() {
            Ok((mut bytes, size)) => {
                let truncated = bytes.len() as u64 > STDERR_LIMIT;
                bytes.truncate(STDERR_LIMIT as usize);
                format!(
                    "size={size} truncated={truncated} prefix={:?}",
                    String::from_utf8_lossy(&bytes)
                )
            }
            Err(error) => format!(
                "unavailable kind={:?} errno={:?}",
                error.kind(),
                error.raw_os_error()
            ),
        }
    }
}

#[path = "owned_process_readiness_tests.rs"]
mod tests;

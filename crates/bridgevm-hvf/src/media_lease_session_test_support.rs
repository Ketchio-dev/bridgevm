use super::*;
pub(super) struct Fixture(PathBuf);
impl Fixture {
    pub(super) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "bv-lease-session-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("disk"), b"disk").unwrap();
        fs::write(root.join("vars"), b"vars").unwrap();
        Self(root)
    }
    pub(super) fn disk(&self) -> PathBuf {
        self.0.join("disk")
    }
    pub(super) fn vars(&self) -> PathBuf {
        self.0.join("vars")
    }
    pub(super) fn assert_available(&self) {
        LockedPair::open(&self.disk(), &self.vars()).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub(super) struct Contender<'a> {
    pub(super) fixture: &'a Fixture,
    pub(super) command: &'a [u8],
}
impl Read for Contender<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let error = LockedPair::open(&self.fixture.disk(), &self.fixture.vars())
            .err()
            .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        self.command.read(bytes)
    }
}

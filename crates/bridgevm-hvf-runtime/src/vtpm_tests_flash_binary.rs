//! The fake swtpm that binds its sockets and then exits like a bad key.

use std::path::{Path, PathBuf};

/// Write an executable stand-in for swtpm into `dir` and return its path.
///
/// It binds every `path=` socket the way swtpm does, then exits non-zero.
/// The binds must happen in a process that exits: the previous fixture
/// backgrounded one `nc -lU` per socket, so every run reparented two
/// listeners to launchd that nothing ever reaped. 226 had accumulated on the
/// Studio host and exhausted `kern.maxprocperuid`, which killed an unrelated
/// live lane with `fork: Resource temporarily unavailable`.
pub(super) fn write_flash_swtpm(dir: &Path) -> PathBuf {
    let script = dir.join("flash-swtpm.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\n\
         paths=\n\
         for arg in \"$@\"; do\n\
         \x20 case \"$arg\" in\n\
         \x20   *path=*) p=${arg#*path=}; paths=\"$paths ${p%%,*}\";;\n\
         \x20 esac\n\
         done\n\
         python3 -c 'import socket,sys,time\n\
         h=[socket.socket(socket.AF_UNIX) for _ in sys.argv[1:]]\n\
         [(s.bind(p),s.listen(1)) for s,p in zip(h,sys.argv[1:])]\n\
         time.sleep(0.05)' $paths\n\
         exit 1\n",
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
    std::fs::set_permissions(&script, permissions).unwrap();
    script
}

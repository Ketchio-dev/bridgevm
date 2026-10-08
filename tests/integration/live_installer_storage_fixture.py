"""Copy real storage helpers; override only their statvfs result in owned tests."""
from pathlib import Path
import sys


def prepare(fixture, source, free_gib):
    for name in ("live_storage_capacity.py", "installer-storage-preflight.sh", "render-live-launchagent.py"):
        (fixture.scripts / name).write_bytes((source.parent / name).read_bytes())
    python = fixture.bin / "python3"
    python.write_text(f'''#!{sys.executable}
import os,runpy,sys,types
args=sys.argv[1:]
while args and args[0] in ("-I","-B"): args.pop(0)
if args and args[0].endswith("/live_storage_capacity.py"):
    os.fstatvfs=lambda fd: types.SimpleNamespace(f_frsize=1024**3,f_bavail={free_gib},f_blocks=4096,f_flag=0)
    sys.argv=args
    runpy.run_path(args[0],run_name="__main__")
else:
    os.execv({sys.executable!r}, [{sys.executable!r}] + sys.argv[1:])
''')
    python.chmod(0o700)

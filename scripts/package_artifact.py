#!/usr/bin/env python3
"""Create and verify native archives before atomic, non-overwriting publication."""
import hashlib
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile

from package_publication import publish_exclusive


def identity(info):
    return info.st_dev, info.st_ino


def require_absent(output):
    try:
        output.lstat()
    except FileNotFoundError:
        return
    raise FileExistsError(f"refusing existing output: {output}")


def require_outside_app(app, output):
    app_identity = identity(app.stat())
    # Check both the spelling and its resolved ancestors. A link inside the app
    # may lead outside it, and a link outside may lead to an app subdirectory.
    for parent in (output.parent, output.parent.resolve()):
        for ancestor in (parent, *parent.parents):
            try:
                if identity(ancestor.stat()) == app_identity:
                    raise ValueError(f"output must be outside the input app: {output}")
            except FileNotFoundError:
                continue


def run(*arguments):
    subprocess.run([str(argument) for argument in arguments], check=True)


def build_verified(kind, app, archive, work):
    if kind == "dmg":
        run("cp", "-R", app, str(work) + "/")
        (work / "Applications").symlink_to("/Applications")
        run("hdiutil", "create", "-quiet", "-volname", "BridgeVM", "-srcfolder", work,
            "-format", "UDZO", archive)
        run("hdiutil", "verify", "-quiet", archive)
    else:
        # Keep the native tar implementation and its macOS metadata handling.
        run("tar", "-czf", archive, "-C", app.parent, "--", app.name)
        run("tar", "-xzf", archive, "-C", work)
        run("codesign", "--verify", "--deep", "--strict", work / app.name)


def package(kind, app_argument, output_argument):
    app = Path.cwd() / app_argument
    output = Path.cwd() / output_argument
    require_absent(output)
    if not app.is_dir() or app.name in ("", ".", ".."):
        raise ValueError(f"app bundle not found: {app}")
    require_outside_app(app, output)
    parent = output.parent.resolve()
    parent.mkdir(parents=True, exist_ok=True)
    require_outside_app(app, output)
    parent_fd = os.open(parent, os.O_RDONLY | os.O_DIRECTORY)
    try:
        run("codesign", "--verify", "--deep", "--strict", app)
        with tempfile.TemporaryDirectory(prefix=".bridgevm-package-", dir=parent) as stage:
            archive = Path(stage) / ("artifact.dmg" if kind == "dmg" else "artifact.tar.gz")
            with tempfile.TemporaryDirectory(prefix="work-", dir=stage) as work:
                build_verified(kind, app, archive, Path(work))
            if not stat.S_ISREG(archive.lstat().st_mode):
                raise ValueError("packager did not create a regular archive")
            digest = hashlib.sha256()
            with archive.open("rb") as source:
                for chunk in iter(lambda: source.read(1024 * 1024), b""):
                    digest.update(chunk)
            require_outside_app(app, output)
            if identity(output.parent.stat()) != identity(os.fstat(parent_fd)):
                raise ValueError("output parent changed while packaging")
            publish_exclusive(archive, output.name, parent_fd)
        print(f"{digest.hexdigest()}  {output_argument}")
        print(f"{kind}: PASS ({output_argument})")
    finally:
        os.close(parent_fd)


def main():
    if len(sys.argv) != 4 or sys.argv[1] not in ("dmg", "tarball"):
        print("usage: package_artifact.py dmg|tarball APP OUTPUT", file=sys.stderr)
        return 2
    try:
        package(*sys.argv[1:])
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

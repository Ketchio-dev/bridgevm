#!/usr/bin/env python3
"""Read canonical Mesa notice bytes directly from a pinned Git tree."""

from __future__ import annotations

import hashlib
import os
from pathlib import Path, PurePosixPath
import subprocess
import tempfile
import zipfile


OVERVIEW_PATH = "docs/license.rst"
LICENCE_PREFIX = "licenses/"
ZIP_PREFIX = "Mesa-upstream-licenses/"
FIXED_TIMESTAMP = (1980, 1, 1, 0, 0, 0)


class NoticeObjectError(RuntimeError):
    """The pinned notice tree cannot be read without ambiguity."""


def _git_environment() -> dict[str, str]:
    environment = {
        key: value
        for key, value in os.environ.items()
        if key
        not in {
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_COMMON_DIR",
            "GIT_DIR",
            "GIT_INDEX_FILE",
            "GIT_OBJECT_DIRECTORY",
            "GIT_REPLACE_REF_BASE",
            "GIT_WORK_TREE",
        }
    }
    environment["GIT_NO_REPLACE_OBJECTS"] = "1"
    environment["LC_ALL"] = "C"
    return environment


def _git(repo: Path, *arguments: str, stdin: bytes | None = None) -> bytes:
    try:
        result = subprocess.run(
            ["git", "-C", str(repo), *arguments],
            input=stdin,
            check=True,
            capture_output=True,
            env=_git_environment(),
        )
    except (OSError, subprocess.CalledProcessError) as error:
        detail = getattr(error, "stderr", b"").decode("utf-8", "replace").strip()
        suffix = f": {detail}" if detail else ""
        raise NoticeObjectError(f"Git object read failed{suffix}") from error
    return result.stdout


def _safe_relative_licence(path: str) -> str:
    if not path.startswith(LICENCE_PREFIX):
        raise NoticeObjectError(f"unexpected Mesa notice path: {path}")
    relative = path.removeprefix(LICENCE_PREFIX)
    parsed = PurePosixPath(relative)
    if (
        not relative
        or parsed.is_absolute()
        or any(part in ("", ".", "..") for part in parsed.parts)
        or "\\" in relative
    ):
        raise NoticeObjectError(f"unsafe Mesa licence path: {path}")
    return relative


def _tree_blobs(repo: Path, revision: str) -> tuple[bytes, list[tuple[str, bytes]]]:
    if len(revision) != 40 or any(char not in "0123456789abcdef" for char in revision):
        raise NoticeObjectError("Mesa revision is not an exact lowercase SHA-1")
    resolved = _git(repo, "rev-parse", "--verify", f"{revision}^{{commit}}").strip()
    if resolved != revision.encode("ascii"):
        raise NoticeObjectError("Mesa revision does not resolve to the registered commit")
    listing = _git(
        repo,
        "ls-tree",
        "-r",
        "-z",
        "--full-tree",
        revision,
        "--",
        OVERVIEW_PATH,
        LICENCE_PREFIX,
    )
    overview: bytes | None = None
    licences: list[tuple[str, bytes]] = []
    seen: set[str] = set()
    for raw_entry in listing.split(b"\0"):
        if not raw_entry:
            continue
        try:
            metadata, raw_path = raw_entry.split(b"\t", 1)
            mode, kind, object_id = metadata.decode("ascii").split(" ")
            path = raw_path.decode("utf-8")
        except (UnicodeDecodeError, ValueError) as error:
            raise NoticeObjectError("Mesa notice tree entry is malformed") from error
        if mode != "100644" or kind != "blob":
            raise NoticeObjectError(f"Mesa notice path is not a regular blob: {path}")
        if path in seen:
            raise NoticeObjectError(f"duplicate Mesa notice path: {path}")
        seen.add(path)
        payload = _git(repo, "cat-file", "blob", object_id)
        if path == OVERVIEW_PATH:
            overview = payload
        else:
            licences.append((_safe_relative_licence(path), payload))
    if overview is None:
        raise NoticeObjectError("Mesa licence overview blob is missing")
    if not licences:
        raise NoticeObjectError("Mesa upstream licence blob set is empty")
    licences.sort(key=lambda item: item[0])
    return overview, licences


def write_canonical_zip(entries: list[tuple[str, bytes]], destination: Path) -> None:
    if not entries:
        raise NoticeObjectError("Mesa upstream licence blob set is empty")
    names = [name for name, _ in entries]
    if names != sorted(names) or len(names) != len(set(names)):
        raise NoticeObjectError("Mesa upstream licence blob names are not unique and sorted")
    with zipfile.ZipFile(destination, "w", compression=zipfile.ZIP_STORED) as archive:
        for relative, payload in entries:
            safe_relative = _safe_relative_licence(f"{LICENCE_PREFIX}{relative}")
            info = zipfile.ZipInfo(f"{ZIP_PREFIX}{safe_relative}", FIXED_TIMESTAMP)
            info.compress_type = zipfile.ZIP_STORED
            info.create_system = 3
            info.external_attr = 0o100644 << 16
            archive.writestr(info, payload)


def write_pinned_notices(
    repo: Path, revision: str, overview_destination: Path, zip_destination: Path
) -> None:
    overview, licences = _tree_blobs(repo, revision)
    overview_destination.write_bytes(overview)
    write_canonical_zip(licences, zip_destination)


def _run(repo: Path, *arguments: str, stdin: bytes | None = None) -> bytes:
    return _git(repo, *arguments, stdin=stdin)


def _expect_failure(action, marker: str) -> None:
    try:
        action()
    except NoticeObjectError as error:
        if marker not in str(error):
            raise AssertionError(f"wrong failure: {error}") from error
        return
    raise AssertionError(f"mutation was accepted: {marker}")


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="bridgevm-mesa-git-notices.") as temp:
        repo = Path(temp)
        (repo / "docs").mkdir()
        (repo / "licenses" / "exceptions").mkdir(parents=True)
        (repo / ".gitattributes").write_text(
            "docs/license.rst text eol=crlf\nlicenses/** text eol=crlf\n",
            encoding="ascii",
        )
        overview_bytes = b"individual files may have their own licenses\n"
        licence_bytes = b"fixture licence line one\nfixture licence line two\n"
        exception_bytes = b"fixture exception\n"
        (repo / OVERVIEW_PATH).write_bytes(overview_bytes)
        (repo / "licenses" / "MIT").write_bytes(licence_bytes)
        (repo / "licenses" / "exceptions" / "note").write_bytes(exception_bytes)
        _run(repo, "init", "-q")
        _run(repo, "config", "user.name", "BridgeVM Notice Test")
        _run(repo, "config", "user.email", "notice-test@invalid.example")
        _run(repo, "add", ".")
        _run(repo, "commit", "-q", "-m", "fixture")
        revision = _run(repo, "rev-parse", "HEAD").decode("ascii").strip()
        (repo / OVERVIEW_PATH).unlink()
        (repo / "licenses" / "MIT").unlink()
        _run(repo, "checkout", "--", OVERVIEW_PATH, "licenses/MIT")
        assert b"\r\n" in (repo / OVERVIEW_PATH).read_bytes()
        assert b"\r\n" in (repo / "licenses" / "MIT").read_bytes()
        first_overview = repo / "first.rst"
        first_zip = repo / "first.zip"
        second_zip = repo / "second.zip"
        write_pinned_notices(repo, revision, first_overview, first_zip)
        _, entries = _tree_blobs(repo, revision)
        write_canonical_zip(entries, second_zip)
        assert first_overview.read_bytes() == overview_bytes
        assert hashlib.sha256(first_zip.read_bytes()).digest() == hashlib.sha256(
            second_zip.read_bytes()
        ).digest()
        with zipfile.ZipFile(first_zip) as archive:
            members = archive.infolist()
            assert [member.filename for member in members] == [
                "Mesa-upstream-licenses/MIT",
                "Mesa-upstream-licenses/exceptions/note",
            ]
            assert all(member.compress_type == zipfile.ZIP_STORED for member in members)
            assert all(member.create_system == 3 for member in members)
            assert all(member.date_time == FIXED_TIMESTAMP for member in members)
            assert all(member.external_attr >> 16 == 0o100644 for member in members)
        _expect_failure(
            lambda: _tree_blobs(repo, "0" * 40), "Git object read failed"
        )
        link_blob = _run(repo, "hash-object", "-w", "--stdin", stdin=b"MIT").strip()
        _run(
            repo,
            "update-index",
            "--add",
            "--cacheinfo",
            f"120000,{link_blob.decode('ascii')},licenses/link",
        )
        _run(repo, "commit", "-q", "-m", "symlink fixture")
        symlink_revision = _run(repo, "rev-parse", "HEAD").decode("ascii").strip()
        _expect_failure(
            lambda: _tree_blobs(repo, symlink_revision), "not a regular blob"
        )
        _run(repo, "update-index", "--force-remove", "licenses/link", OVERVIEW_PATH)
        _run(repo, "commit", "-q", "-m", "missing overview fixture")
        missing_overview = _run(repo, "rev-parse", "HEAD").decode("ascii").strip()
        _expect_failure(
            lambda: _tree_blobs(repo, missing_overview), "overview blob is missing"
        )
        _run(repo, "checkout", revision, "--", OVERVIEW_PATH)
        _run(repo, "rm", "-q", "-r", "-f", "licenses")
        _run(repo, "commit", "-q", "-m", "empty licences fixture")
        empty_licences = _run(repo, "rev-parse", "HEAD").decode("ascii").strip()
        _expect_failure(
            lambda: _tree_blobs(repo, empty_licences), "blob set is empty"
        )

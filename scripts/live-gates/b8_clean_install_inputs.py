"""Offline B8 release and manifest contracts; no publication or clean-host claim."""
from __future__ import annotations

import hashlib
import base64
import json
import os
from pathlib import Path, PurePosixPath
import plistlib
import re
import stat
import tarfile
import tempfile
import unicodedata

from b8_clean_install_files import (_appledouble, _canonical, _copy_tar, _hash,
                                    read_committed_blob, read_regular)

SHA = re.compile(r"[0-9a-f]{64}\Z")
COMMIT = re.compile(r"[0-9a-f]{40}\Z")
TAG = re.compile(r"v[0-9]+(?:\.[0-9]+){2}(?:[.-][0-9A-Za-z]+)*\Z")
MANIFEST_KEYS = {"schema", "source_commit", "release_tag", "release_contract_sha256",
                 "sha256s_sha256", "tarball_sha256", "clean_host_attestation",
                 "clean_host_attestation_sha256"}
SYSTEM_PATH = "/usr/bin:/bin:/usr/sbin:/sbin"
MAX_TARBALL = 2_000_000_000
MAX_EXPANDED = 4_000_000_000
MAX_MEMBERS = 20_000
ROOT = Path(__file__).resolve().parents[2]


def _unique(pairs):
    value = {}
    for key, item in pairs:
        if key in value:
            raise ValueError("B8 JSON has a duplicate key")
        value[key] = item
    return value


def _constant(_):
    raise ValueError("B8 JSON has a nonfinite constant")


def _parse_json(raw: bytes) -> dict:
    try:
        value = json.loads(raw.decode("utf-8"),
                           object_pairs_hook=_unique, parse_constant=_constant)
    except (UnicodeError, json.JSONDecodeError) as error:
        raise ValueError("malformed B8 JSON") from error
    if not isinstance(value, dict):
        raise ValueError("B8 JSON must be an object")
    return value


def _json(path: Path, limit: int = 65_536) -> dict:
    return _parse_json(read_regular(path, limit))


def load_manifest(path: Path, expected_commit: str) -> dict:
    value = _json(path, 8192)
    if set(value) != MANIFEST_KEYS or value["schema"] != "bridgevm.b8-clean-install-inputs.v1":
        raise ValueError("B8 manifest field set differs")
    if (type(value["source_commit"]) is not str or not COMMIT.fullmatch(value["source_commit"])
            or value["source_commit"] != expected_commit or type(value["release_tag"]) is not str
            or not TAG.fullmatch(value["release_tag"])):
        raise ValueError("B8 source or release tag differs")
    for key in ("release_contract_sha256", "sha256s_sha256", "tarball_sha256",
                "clean_host_attestation_sha256"):
        if type(value[key]) is not str or not SHA.fullmatch(value[key]):
            raise ValueError("B8 manifest hash malformed: " + key)
    if type(value["clean_host_attestation"]) is not str:
        raise ValueError("B8 attestation path differs")
    attestation = Path(value["clean_host_attestation"])
    if hashlib.sha256(read_regular(attestation, 8192)).hexdigest() != value["clean_host_attestation_sha256"]:
        raise ValueError("B8 attestation file differs")
    return value  # Hashing a statement does not establish a freshly installed OS.


def safe_installer_env(home: Path, temporary: Path) -> dict[str, str]:
    for path in (home, temporary):
        _canonical(path)
        details = os.lstat(path)
        if not stat.S_ISDIR(details.st_mode) or details.st_uid != os.getuid() or details.st_mode & 0o022:
            raise ValueError("B8 installer environment directory is unsafe")
    if os.lstat(temporary).st_mode & 0o077:
        raise ValueError("B8 installer temporary directory must be private")
    return {"PATH": SYSTEM_PATH, "HOME": str(home), "TMPDIR": str(temporary), "LANG": "C"}


def _sums(raw: bytes) -> dict[str, str]:
    rows = {}
    try:
        lines = raw.decode("ascii").splitlines()
    except UnicodeError as error:
        raise ValueError("B8 release checksums are not ASCII") from error
    for line in lines:
        fields = line.split()
        if (len(fields) != 2 or not SHA.fullmatch(fields[0]) or not re.fullmatch(r"[A-Za-z0-9._-]+", fields[1])
                or fields[1] in rows):
            raise ValueError("B8 release checksum row differs")
        rows[fields[1]] = fields[0]
    if not rows:
        raise ValueError("B8 release checksums are empty")
    return rows


def _member_path(name: str) -> tuple[str, ...]:
    if (not name or name.startswith("/") or "\\" in name or "\x00" in name
            or len(name) > 512 or any(ord(char) < 32 for char in name)):
        raise ValueError("B8 archive path is unsafe")
    clean = name[:-1] if name.endswith("/") else name
    parts = clean.split("/")
    if not parts or parts[0] != "BridgeVM.app" or any(part in ("", ".", "..") for part in parts):
        raise ValueError("B8 archive path escapes app")
    return tuple(parts)


def _link_target(parts: tuple[str, ...], target: str) -> None:
    if (not target or target.startswith("/") or "\\" in target or "\x00" in target
            or len(target) > 512 or "//" in target or any(ord(char) < 32 for char in target)):
        raise ValueError("B8 archive link is unsafe")
    joined = PurePosixPath(*parts[:-1], target)
    stack = []
    for part in joined.parts:
        if part == "..":
            if not stack:
                raise ValueError("B8 archive link escapes app")
            stack.pop()
        elif part not in ("", "."):
            stack.append(part)
    if not stack or stack[0] != "BridgeVM.app":
        raise ValueError("B8 archive link escapes app")


def _members(archive: tarfile.TarFile) -> list[tarfile.TarInfo]:
    members, exact, folded, kinds, metadata, provenance = [], set(), set(), {}, {}, {}
    expanded = entries = 0
    for member in archive:
        entries += 1
        pax_keys = set(member.pax_headers)
        allowed_pax = {"mtime", "atime", "ctime", "LIBARCHIVE.xattr.com.apple.provenance",
                       "SCHILY.xattr.com.apple.provenance"}
        if (entries > MAX_MEMBERS or not pax_keys <= allowed_pax
                or sum(len(str(value)) for value in member.pax_headers.values()) > 65_536):
            raise ValueError("B8 archive member count or PAX differs")
        provenance_keys = {"LIBARCHIVE.xattr.com.apple.provenance",
                           "SCHILY.xattr.com.apple.provenance"}
        if pax_keys & provenance_keys:
            if not provenance_keys <= pax_keys:
                raise ValueError("B8 archive provenance PAX pair differs")
            encoded = member.pax_headers["LIBARCHIVE.xattr.com.apple.provenance"]
            plain = member.pax_headers["SCHILY.xattr.com.apple.provenance"]
            try:
                raw = base64.b64decode(encoded + "=" * (-len(encoded) % 4), validate=True)
                equivalent = plain.encode("utf-8", "surrogateescape")
            except (ValueError, UnicodeError) as error:
                raise ValueError("B8 archive provenance PAX encoding differs") from error
            if (not 0 < len(raw) <= 64 or base64.b64encode(raw).decode().rstrip("=") != encoded
                    or raw != equivalent):
                raise ValueError("B8 archive provenance PAX values differ")
            provenance[member.name] = raw
        for clock in ("mtime", "atime", "ctime"):
            if clock in pax_keys and not re.fullmatch(r"-?[0-9]{1,20}(?:\.[0-9]{1,20})?", member.pax_headers[clock]):
                raise ValueError("B8 archive timestamp PAX value differs")
        if member.name == "._BridgeVM.app":
            name, apple_target, parts = member.name, "BridgeVM.app", None
        else:
            parts = _member_path(member.name)
            name = "/".join(parts)
            apple_target = ("/".join((*parts[:-1], parts[-1][2:]))
                            if parts[-1].startswith("._") and len(parts[-1]) > 2 else None)
        alias = unicodedata.normalize("NFD", name).casefold()
        if name in exact or alias in folded:
            raise ValueError("B8 archive has a duplicate or aliased path")
        exact.add(name); folded.add(alias)
        if apple_target is not None:
            if pax_keys & provenance_keys:
                raise ValueError("B8 AppleDouble sidecar carries unexpected PAX")
            source = archive.extractfile(member) if member.isfile() and 50 <= member.size <= 8_000_000 else None
            if source is None:
                raise ValueError("B8 AppleDouble metadata differs")
            with source:
                raw = source.read(member.size + 1)
            if len(raw) != member.size:
                raise ValueError("B8 AppleDouble metadata size differs")
            metadata[name] = (apple_target, raw)
            expanded += member.size
            if expanded > MAX_EXPANDED:
                raise ValueError("B8 archive metadata exceeds bound")
            continue
        if not (member.isdir() or member.isfile() or member.issym()) or member.mode & ~0o777:
            raise ValueError("B8 archive has an unsupported entry type or mode")
        if member.issym():
            _link_target(parts, member.linkname)
        if member.isfile():
            expanded += member.size
            if member.size < 0 or member.size > MAX_TARBALL or expanded > MAX_EXPANDED:
                raise ValueError("B8 archive expansion exceeds bound")
        if any(kinds.get("/".join(parts[:index])) == "link" for index in range(1, len(parts))):
            raise ValueError("B8 archive descends through a link")
        kinds[name] = "link" if member.issym() else "dir" if member.isdir() else "file"
        members.append(member)
    if kinds.get("BridgeVM.app") != "dir" or not members:
        raise ValueError("B8 archive lacks one app root")
    for target, raw in metadata.values():
        if target not in kinds or target not in provenance:
            raise ValueError("B8 AppleDouble metadata lacks its app provenance")
        _appledouble(raw, provenance[target])
    for name in kinds:
        parts = name.split("/")
        if any(kinds.get("/".join(parts[:index])) in ("link", "file")
               for index in range(1, len(parts))):
            raise ValueError("B8 archive has a non-directory ancestor")
    return members


def _extract(archive: tarfile.TarFile, members: list[tarfile.TarInfo], root: Path) -> Path:
    for member in members:
        parts = _member_path(member.name)
        target = root.joinpath(*parts)
        if member.isdir():
            target.mkdir(parents=True, exist_ok=True)
        elif member.isfile():
            target.parent.mkdir(parents=True, exist_ok=True)
            source = archive.extractfile(member)
            if source is None:
                raise ValueError("B8 archive member cannot be read")
            descriptor = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
            with os.fdopen(descriptor, "wb") as output, source:
                remaining = member.size
                while remaining:
                    block = source.read(min(1024 * 1024, remaining))
                    if not block:
                        raise ValueError("B8 archive member is truncated")
                    output.write(block); remaining -= len(block)
        else:
            target.parent.mkdir(parents=True, exist_ok=True)
            os.symlink(member.linkname, target)
    for member in members:
        if member.issym():
            target = root.joinpath(*_member_path(member.name))
            resolved = target.resolve(strict=True)
            if not resolved.is_relative_to(root / "BridgeVM.app"):
                raise ValueError("B8 archive link leaves extracted app")
        elif not member.issym():
            os.chmod(root.joinpath(*_member_path(member.name)), member.mode & 0o777)
    return root / "BridgeVM.app"


def app_tree_hash(app: Path) -> str:
    _canonical(app)
    if not stat.S_ISDIR(os.lstat(app).st_mode):
        raise ValueError("B8 installed app is not a real directory")
    rows, total = [], 0
    def visit(directory: Path):
        nonlocal total
        for child in sorted(directory.iterdir(), key=lambda path: path.name):
            details = os.lstat(child)
            relative = child.relative_to(app).as_posix()
            if len(relative) > 512 or any(ord(char) < 32 for char in relative):
                raise ValueError("B8 app tree has an unsafe name")
            if stat.S_ISDIR(details.st_mode):
                rows.append(["D", relative, details.st_mode & 0o777]); visit(child)
            elif stat.S_ISREG(details.st_mode):
                total += details.st_size
                if total > MAX_EXPANDED:
                    raise ValueError("B8 app tree exceeds size bound")
                rows.append(["F", relative, details.st_mode & 0o777, details.st_size,
                             _hash(child, MAX_TARBALL)])
            elif stat.S_ISLNK(details.st_mode):
                target = os.readlink(child)
                _link_target(tuple(("BridgeVM.app", *child.relative_to(app).parts)), target)
                if not child.resolve(strict=True).is_relative_to(app):
                    raise ValueError("B8 app link leaves bundle")
                rows.append(["L", relative, target])
            else:
                raise ValueError("B8 app tree has a special entry")
            if len(rows) > MAX_MEMBERS:
                raise ValueError("B8 app tree entry count exceeds bound")
    visit(app)
    return hashlib.sha256(json.dumps(sorted(rows), separators=(",", ":"), ensure_ascii=False).encode()).hexdigest()


def _app_identity(app: Path) -> tuple[str, str]:
    info = plistlib.loads(read_regular(app / "Contents/Info.plist", 65_536))
    if not isinstance(info, dict) or info.get("CFBundleIdentifier") != "dev.bridgevm.control":
        raise ValueError("B8 app bundle identity differs")
    name = info.get("CFBundleExecutable")
    if not isinstance(name, str) or not re.fullmatch(r"[A-Za-z0-9._-]{1,128}", name):
        raise ValueError("B8 app executable name differs")
    executable = app / "Contents/MacOS" / name
    if not os.lstat(executable).st_mode & 0o111:
        raise ValueError("B8 app executable is not executable")
    return name, _hash(executable, MAX_TARBALL)


def verify_release(manifest: dict, assets: Path) -> dict:
    commit_sha = manifest.get("source_commit")
    tag = manifest.get("release_tag")
    if (type(commit_sha) is not str or not COMMIT.fullmatch(commit_sha)
            or type(tag) is not str or not TAG.fullmatch(tag)):
        raise ValueError("B8 release source commit or tag malformed")
    registry = _parse_json(read_committed_blob(ROOT, commit_sha, "capabilities/windows-hvf.json", 1_000_000))
    criteria = {item.get("id"): item for item in registry.get("criteria", []) if isinstance(item, dict)}
    a9 = criteria.get("A9", {})
    if (registry.get("product_state") != "ENGINEERING_PREVIEW"
            or a9.get("release_blocking") is not True or "3D-off" not in a9.get("statement", "")
            or type(registry.get("reviewed")) is not str
            or not re.fullmatch(r"[0-9]{4}-[0-9]{2}-[0-9]{2}", registry["reviewed"])
            or type(registry.get("tested_commit")) is not str
            or not COMMIT.fullmatch(registry["tested_commit"])):
        raise ValueError("B8 exact-source registry lacks preview boundary")
    canonical = json.dumps(registry, ensure_ascii=False, separators=(",", ":"), sort_keys=True).encode()
    expected_registry = {"path": "capabilities/windows-hvf.json", "reviewed": registry["reviewed"],
                         "tested_commit": registry["tested_commit"],
                         "canonical_json_sha256": hashlib.sha256(canonical).hexdigest()}
    _canonical(assets)
    if not stat.S_ISDIR(os.lstat(assets).st_mode):
        raise ValueError("B8 offline asset root is unsafe")
    release = _json(assets / "github-release.json")
    commit = _json(assets / "github-commit.json")
    contract_raw = read_regular(assets / "BridgeVM-release.json", 65_536)
    sums_raw = read_regular(assets / "SHA256SUMS", 65_536)
    contract = _parse_json(contract_raw)
    tar_name = "BridgeVM-" + tag + ".tar.gz"
    listed = release.get("assets")
    names = [item.get("name") for item in listed] if isinstance(listed, list) and all(isinstance(item, dict) for item in listed) else []
    if (release.get("tag_name") != tag or release.get("draft") is not False
            or release.get("prerelease") is not False
            or any(type(name) is not str for name in names) or len(names) != len(set(names))
            or not {"BridgeVM-release.json", "SHA256SUMS", tar_name} <= set(names)
            or commit.get("sha") != manifest["source_commit"]):
        raise ValueError("B8 release metadata differs from exact source")
    graphics = contract.get("windows_graphics")
    if (set(contract) != {"schema_version", "project", "version", "source_commit", "channel",
                         "product_state", "macos", "windows_graphics", "capability_registry"}
            or type(contract["schema_version"]) is not int or contract["schema_version"] != 1
            or contract["project"] != "BridgeVM" or contract["version"] != tag
            or contract["source_commit"] != manifest["source_commit"]
            or contract["channel"] != "general-preview"
            or contract["product_state"] != "ENGINEERING_PREVIEW"
            or contract["capability_registry"] != expected_registry
            or not isinstance(graphics, dict) or graphics.get("install_mode") != "3d-off"
            or graphics.get("kernel_driver_included") is not False
            or graphics.get("test_signing_required") is not False
            or graphics.get("product_injection_available") is not False
            or contract.get("macos") != {"developer_id_signed": False, "notarized": False}):
        raise ValueError("B8 release contract differs")
    tar_path = assets / tar_name
    if (hashlib.sha256(contract_raw).hexdigest() != manifest["release_contract_sha256"]
            or hashlib.sha256(sums_raw).hexdigest() != manifest["sha256s_sha256"]):
        raise ValueError("B8 release asset hash differs from sealed manifest")
    sums = _sums(sums_raw)
    if sums.get("BridgeVM-release.json") != manifest["release_contract_sha256"] or sums.get(tar_name) != manifest["tarball_sha256"]:
        raise ValueError("B8 release checksum rows differ")
    with tempfile.TemporaryDirectory(prefix="b8-offline-release-") as temporary:
        root = Path(temporary).resolve()
        pinned_tar = root / tar_name
        _copy_tar(tar_path, pinned_tar, manifest["tarball_sha256"], MAX_TARBALL)
        with tarfile.open(pinned_tar, "r:gz") as archive:
            app = _extract(archive, _members(archive), root)
        name, executable_sha = _app_identity(app)
        tree_sha = app_tree_hash(app)
    return {"tarball_sha256": manifest["tarball_sha256"], "app_tree_sha256": tree_sha,
            "app_executable_sha256": executable_sha, "app_executable_name": name}


def verify_installed(expected: dict, app: Path) -> None:
    name, executable_sha = _app_identity(app)
    if (name != expected["app_executable_name"] or executable_sha != expected["app_executable_sha256"]
            or app_tree_hash(app) != expected["app_tree_sha256"]):
        raise ValueError("B8 installed app differs from sealed release tarball")

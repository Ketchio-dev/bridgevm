"""Owned synthetic media and authenticated lane bytes; no guest or keys."""
import hashlib
import json
from pathlib import Path
import shutil
import struct
import sys
import uuid
import zlib

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
from native_snapshot_restore_artifacts import RELATIONS, tree_hash
from t22_pair_provenance import file_hash, load_writer

COMMIT, JOB, NONCE = "b" * 40, "owned-fixture", "a" * 64
ESP = "c12a7328-f81f-11d2-ba4b-00a0c93ec93b"
DATA = "ebd0a0a2-b9e5-4433-87c0-68b6b72699c7"


def write_json(path, value):
    data = json.dumps(value, sort_keys=True).encode()
    readonly = path.exists() and not path.stat().st_mode & 0o222
    if readonly: path.chmod(0o600)
    path.write_bytes(data)
    if readonly: path.chmod(0o400)
    return hashlib.sha256(data).hexdigest()


def gpt(path, *, oem=b"NTFS    ", unknown=False, overlap=False):
    size, count = 64 << 20, 128
    fat, ntfs = bytearray(512), bytearray(512)
    fat[82:90], fat[510:] = b"FAT32   ", b"\x55\xaa"
    ntfs[3:11], ntfs[510:] = oem, b"\x55\xaa"
    entries = [(ESP, 2048, 4095, fat), (str(uuid.uuid4()) if unknown else DATA,
               3000 if overlap else 6144, 100000, ntfs)]
    table = bytearray(count * 128)
    for i, (kind, first, last, _) in enumerate(entries):
        struct.pack_into("<16s16sQQQ", table, i * 128, uuid.UUID(kind).bytes_le,
                         uuid.uuid4().bytes_le, first, last, 0)
    header = bytearray(92)
    struct.pack_into("<8sIIIIQQQQ16sQIII", header, 0, b"EFI PART", 0x10000, 92, 0, 0,
                     1, size // 512 - 1, 34, size // 512 - 34, uuid.uuid4().bytes_le,
                     2, count, 128, zlib.crc32(table))
    struct.pack_into("<I", header, 16, zlib.crc32(header))
    with path.open("wb") as stream:
        stream.truncate(size); stream.seek(512); stream.write(header)
        stream.seek(1024); stream.write(table)
        for _, first, _, body in entries:
            stream.seek(first * 512); stream.write(body)


def volume(letter="C:", filesystem="NTFS", ordinal=1):
    value = {"device_id": "\\\\?\\Volume{" + f"00000000-0000-0000-0000-{ordinal:012d}" + "}\\",
             "drive_letter": letter, "filesystem": filesystem, "drive_type": 3}
    for key in ("conversion_return", "conversion_status", "encryption_return", "encryption_method", "protection_return", "protection_status"):
        value[key] = 0 if filesystem == "NTFS" else None
    return value


def report(nonce="1" * 32, script_hash="2" * 64):
    return {"schema": "bridgevm.t22-pair-admission.v1", "nonce": nonce,
            "script_sha256": script_hash, "system_drive": "C:",
            "volumes": [volume(), volume(None, ordinal=2), volume(None, "FAT32", 3)]}


class PairFixture:
    def __init__(self, directory, tier="T17"):
        self.root = directory
        self.selected = directory / "selected-media"; self.selected.mkdir()
        self.disk, self.vars = self.selected / "disk.raw", self.selected / "vars.fd"
        gpt(self.disk)
        with self.vars.open("wb") as stream:
            stream.truncate(64 << 20)
        self.app = directory / "Fixture.app"
        for relative in RELATIONS.values():
            path = self.app / relative; path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"synthetic packaged asset")
        resources = self.app / "Contents/Resources"
        firmware = resources / "firmware/edk2-aarch64-secure-code.fd"
        firmware.parent.mkdir()
        shutil.copyfile(ROOT / "crates/bridgevm-hvf/firmware/edk2-aarch64-secure-code.fd", firmware)
        self.wrapper = resources / "scripts/run-hvf-windows-installed-boot.sh"
        self.wrapper.parent.mkdir(); self.wrapper.write_text("#!/bin/bash\nexit 1\n")
        shutil.copyfile(ROOT / "scripts/run-hvf-windows-scripted-install-policy.sh",
                        resources / "scripts/run-hvf-windows-scripted-install-policy.sh")
        writer = load_writer(tier)
        self.result = {"schema_version": writer.LANE_SCHEMA, "job_id": JOB, "commit": COMMIT,
                       "campaign_mode": "pilot", "lane": 1, "nonce": NONCE,
                       "three_d_injection": False, "ui_frontend_automated": True,
                       "failure_code": "none", "failure_detail": "", "cleanup_verified": True,
                       **dict.fromkeys(writer.LANE_STAGES, True), **dict.fromkeys(writer.LANE_HASHES, "c" * 64)}
        if tier == "T17": self.result["installer_source_path"] = "/synthetic/unused.raw"
        self.result.update({"final_disk_sha256": file_hash(self.disk), "final_vars_sha256": file_hash(self.vars)})
        slug = ("bridgevm-t17-lane" if tier == "T17" else "bridgevm-a9-import-lane") + "-1-" + NONCE[:12]
        title = "BridgeVM T17 Lane" if tier == "T17" else "BridgeVM A9 Import Lane"
        self.request = {"schema_version": "bridgevm." + ("windows-hvf-3d-off" if tier == "T17" else "windows-hvf-import") + "-product-e2e-request." + ("v2" if tier == "T17" else "v1"),
                        "job_id": JOB, "commit": COMMIT, "campaign_mode": "pilot", "lane": 1,
                        "nonce": NONCE, "vm_name": title + " 1 " + NONCE[:12], "vm_slug": slug,
                        "three_d_injection": False, **{k: "/never-open-keys/" + k for k in writer.REQUEST_PATHS}}
        self.pair = {"format_version": 1, "vm_id": slug, "disk_bytes": self.disk.stat().st_size,
                     "disk_sha256": self.result["final_disk_sha256"], "vars_bytes": 64 << 20,
                     "vars_sha256": self.result["final_vars_sha256"]}
        self.request_path, self.result_path, self.stamp_path = (directory / (k + ".json") for k in ("request", "result", "stamp"))
        request_hash = write_json(self.request_path, self.request)
        result_hash = write_json(self.result_path, self.result)
        self.stamp = {"schema_version": "bridgevm." + ("windows-hvf-3d-off" if tier == "T17" else "windows-hvf-import") + "-product-e2e-host-stamp.v1",
                      "job_id": JOB, "commit": COMMIT, "lane": 1, "nonce": NONCE,
                      "request_sha256": request_hash, "result_sha256": result_hash}
        self.pair_path = self.selected / "manifest.json"
        write_json(self.pair_path, self.pair); write_json(self.stamp_path, self.stamp)
        self.origin = {"schema": "bridgevm.t22-retained-origin.v1", "tier": tier,
                       "job_id": JOB, "commit": COMMIT, "mode": "pilot", "lane": 1}
        self.origin_path = directory / "origin.json"
        self.manifest = directory / "input.tsv"
        self.refresh()
        self.disk.chmod(0o400); self.vars.chmod(0o400)
        self.pair_path.chmod(0o400)
        self.selected.chmod(0o500)

    def refresh(self):
        for name, path in (("request", self.request_path), ("result", self.result_path),
                           ("stamp", self.stamp_path), ("selected_manifest", self.pair_path)):
            self.origin[name] = {"path": str(path), "sha256": file_hash(path)}
        self.origin_hash = write_json(self.origin_path, self.origin)
        self.rows = {"source_commit": [COMMIT], "app_profile": ["release"], "binary_profile": ["release"],
                     "binary_features": ["venus"], "rust_toolchain": ["1.97.0"],
                     "app_bundle": [str(self.app), tree_hash(self.app)],
                     "image": [str(self.disk), file_hash(self.disk)], "vars": [str(self.vars), file_hash(self.vars)]}
        self.rows.update({k: [str(self.app / relative), file_hash(self.app / relative)] for k, relative in RELATIONS.items()})
        data = "".join("\t".join((k, *v)) + "\n" for k, v in self.rows.items()).encode()
        self.manifest.write_bytes(data)
        self.manifest_hash = hashlib.sha256(data).hexdigest()

    def args(self):
        return self.manifest, self.manifest_hash, self.origin_path, self.origin_hash, COMMIT

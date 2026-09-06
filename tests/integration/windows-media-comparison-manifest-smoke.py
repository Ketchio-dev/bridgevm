#!/usr/bin/env python3
"""Fixture and rejection tests for the diagnostic-only media contract."""
import importlib.util
from pathlib import Path
import tempfile

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("media_manifest", ROOT / "scripts/live-gates/windows-media-comparison-manifest.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

with tempfile.TemporaryDirectory(prefix="media comparison ") as directory:
    root = Path(directory)
    firmware = root / module.FIRMWARE
    firmware.parent.mkdir(parents=True)
    firmware.write_bytes(b"firmware")
    rows = [f"{key}\t{value}" for key, value in module.FIXED.items()]
    for key in module.ASSETS:
        path = firmware if key == "firmware" else root / key
        if key == "viogpu_dir":
            path.mkdir()
            (path / "driver.sys").write_bytes(b"driver")
            digest = module.tree_seal(path)
        else:
            path.write_bytes(key.encode())
            digest = module.seal(path)
        rows.append(f"{key}\t{path}\t{digest}")
    original = "\n".join(rows) + "\n"
    manifest = root / "inputs.tsv"

    def accepts(text):
        manifest.write_text(text)
        try:
            result = module.verify(manifest, root, root / "binary")
            assert result["claim_eligible"] is False
            return True
        except (ValueError, OSError):
            return False

    assert accepts(original)
    mutations = [original + rows[-1] + "\n", original + "unknown\tvalue\n",
                 original.replace("diagnostic-only", "release"),
                 original.replace("claim_eligible\tfalse", "claim_eligible\ttrue"),
                 original.replace("sample_count\t2", "sample_count\t1"),
                 original.replace("original,reinjected", "reinjected,original"),
                 original.replace(str(root / "vars"), "relative-vars"),
                 original.replace(str(root / "vars"), str(root / "original"))]
    mutations.extend("\n".join(row for row in rows if not row.startswith(key + "\t")) for key in module.ASSETS)
    for text in mutations:
        assert not accepts(text), "invalid manifest accepted"
    (root / "binary").write_bytes(b"changed")
    assert not accepts(original), "changed sealed binary accepted"
    (root / "binary").write_bytes(b"binary")
    firmware.write_bytes(b"changed")
    assert not accepts(original), "changed worktree firmware accepted"
    firmware.write_bytes(b"firmware")
    link = root / "viogpu_dir/link"
    link.symlink_to(root / "original")
    assert not accepts(original), "symlink in driver tree accepted"
    link.unlink()
    assert accepts(original)
print("PASS: diagnostic media manifest fixtures and 19 rejection cases")

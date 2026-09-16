"""Reconstruct fixed complete signed bundles; never sign after sealing."""
import os
from pathlib import Path
import plistlib
import subprocess
import sys

from app_ui_host_bundle import bundle_metadata
from app_ui_host_diagnostic import RESOURCES
from app_ui_host_v2_manifest import FILES, copy_input, verify_input

ROLES = {
    "host": ("BridgeVMAppUIHost.app", "BridgeVMControl", "dev.bridgevm.app-ui-host", "binary"),
    "driver": ("BridgeVMAppUIDriver.app", "AppUIHostLauncher", "dev.bridgevm.app-ui-driver", "launcher"),
}


def driver_bundle_metadata():
    return {"CFBundleExecutable": "AppUIHostLauncher", "CFBundlePackageType": "APPL",
            "CFBundleIdentifier": "dev.bridgevm.app-ui-driver", "CFBundleName": "BridgeVM UI Driver",
            "CFBundleVersion": "1", "CFBundleShortVersionString": "1.0",
            "LSMinimumSystemVersion": "14.0", "LSUIElement": True, "NSPrincipalClass": "NSApplication"}


def verify_metadata(bundle, role):
    info = plistlib.loads((bundle / "Contents/Info.plist").read_bytes())
    expected = bundle_metadata(ROLES[role][2]) if role == "host" else driver_bundle_metadata()
    if info != expected:
        raise ValueError("fixed signed bundle metadata differs")


def verify_bundles(private, fields, signatures=True):
    for role, (name, executable, _, key) in ROLES.items():
        bundle = private / name
        for field, path in ((key, bundle / "Contents/MacOS" / executable),
                            (role + "_info", bundle / "Contents/Info.plist"),
                            (role + "_resources", bundle / "Contents/_CodeSignature/CodeResources")):
            verify_input(field, path, fields[field][1])
        verify_metadata(bundle, role)
        if signatures:
            if sys.platform != "darwin":
                raise ValueError("strict bundle verification requires macOS")
            subprocess.run(["/usr/bin/codesign", "--verify", "--deep", "--strict", str(bundle)],
                           check=True, timeout=10, capture_output=True)


def reconstruct_bundles(private, output, repo, commit, fields):
    if not private.is_dir() or private != private.resolve(strict=True):
        raise ValueError("private directory is not canonical")
    for role, (name, executable, _, key) in ROLES.items():
        bundle = private / name
        bundle.mkdir(mode=0o700)
        for part in ("Contents", "Contents/MacOS", "Contents/_CodeSignature"):
            (bundle / part).mkdir(mode=0o700)
        for field, destination in ((key, bundle / "Contents/MacOS" / executable),
                                   (role + "_info", bundle / "Contents/Info.plist"),
                                   (role + "_resources", bundle / "Contents/_CodeSignature/CodeResources")):
            copy_input(field, output / FILES[field], destination, fields[field][1])
    resources = private / ROLES["host"][0] / "Contents/Resources/BridgeVMApp_BridgeVMControl.bundle"
    resources.parent.mkdir(mode=0o700)
    resources.mkdir(mode=0o700)
    for name in RESOURCES:
        reference = f"{commit}:apps/macos/Sources/BridgeVMControl/Resources/{name}"
        size = int(subprocess.check_output(["git", "-C", str(repo), "cat-file", "-s", reference], timeout=5))
        if not 0 < size <= 4 * 1024 * 1024:
            raise ValueError("sealed resource exceeds bounds")
        data = subprocess.check_output(["git", "-C", str(repo), "cat-file", "blob", reference], timeout=5)
        if len(data) != size:
            raise ValueError("sealed resource length differs")
        with (resources / name).open("xb") as target:
            os.fchmod(target.fileno(), 0o600)
            target.write(data)
    verify_bundles(private, fields)

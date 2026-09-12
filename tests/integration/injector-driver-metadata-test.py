#!/usr/bin/env python3
"""Native copyfile metadata regression; this is not a WinPE boot test."""
import os
from pathlib import Path
import platform
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


@unittest.skipUnless(platform.system() == "Darwin", "native macOS copyfile semantics required")
class DriverMetadataTests(unittest.TestCase):
    def test_production_copy_preserves_bytes_without_transferring_metadata(self):
        builder = (ROOT / "scripts/build-hvf-windows-driver-injector.sh").read_text()
        statement = '/bin/cp -X "$src"/* "$DST_VOL/drivers/$name/"; /usr/sbin/dot_clean -m "$DST_VOL/drivers/$name"'
        self.assertEqual([line.strip() for line in builder.splitlines()
                          if line.strip().startswith("/bin/cp -X ")], [statement])
        with tempfile.TemporaryDirectory(prefix="BridgeVM metadata ") as temporary:
            root = Path(temporary)
            source = root / "source"
            source.mkdir()
            volume = root / "volume"
            target = volume / "drivers/netkvm"
            target.mkdir(parents=True)
            attribute = "com.bridgevm.copyfile-regression"
            payloads = {"netkvm.inf": b"[Version]\r\nSignature=\"$Windows NT$\"\r\n",
                        "netkvm.cat": b"catalog-fixture\x00", "netkvm.sys": b"driver-fixture\xff"}
            for name, payload in payloads.items():
                path = source / name
                path.write_bytes(payload)
                subprocess.run(["xattr", "-w", attribute, "host-metadata-not-guest-payload", str(path)], check=True)
            (source / "._stale.inf").write_bytes(b"excluded source sidecar")
            environment = dict(os.environ)
            environment.pop("COPYFILE_DISABLE", None)
            control = root / "control.inf"
            subprocess.run(["/bin/cp", str(source / "netkvm.inf"), str(control)], env=environment, check=True)
            self.assertEqual(subprocess.check_output(["xattr", "-p", attribute, str(control)]).strip(), b"host-metadata-not-guest-payload")
            environment.update(src=str(source), DST_VOL=str(volume), name="netkvm")
            subprocess.run(["bash", "-c", statement], env=environment, check=True)
            self.assertEqual({path.name for path in target.iterdir()}, set(payloads))
            for name, payload in payloads.items():
                self.assertEqual((target / name).read_bytes(), payload)
                self.assertNotIn(attribute, subprocess.check_output(["xattr", str(target / name)], text=True).splitlines())
                self.assertEqual(subprocess.check_output(["xattr", "-p", attribute, str(source / name)]).strip(), b"host-metadata-not-guest-payload")


if __name__ == "__main__":
    unittest.main()

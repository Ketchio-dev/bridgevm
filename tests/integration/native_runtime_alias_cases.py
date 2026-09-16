"""Use real Darwin system aliases, even when the outer test output is under /Users."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


class NativeRuntimeAliasCases:
    def check_system_alias(self, parent):
        previous = self.library, self.identity, self.namespace
        with tempfile.TemporaryDirectory(prefix="bridgevm-runtime-alias-", dir=parent) as temporary:
            self.library = Path(temporary).resolve() / "library"
            self.library.mkdir(mode=0o700)
            info = self.library.stat()
            self.identity = dict(canonicalPath=str(self.library), device=info.st_dev,
                                 inode=info.st_ino, uid=os.geteuid())
            binding = f"bridgevm-native-runtime-v1\n{os.geteuid()}\n{info.st_dev}\n{info.st_ino}\n"
            key = hashlib.sha256(binding.encode()).hexdigest()[:32]
            self.namespace = Path(f"/private/tmp/bridgevm-app-{os.geteuid()}") / key
            try:
                self.assertTrue(str(self.library).startswith("/private/"))
                alias = Path(str(self.library).removeprefix("/private"))
                self.assertEqual(alias.resolve(strict=True), self.library)
                with self.owner():
                    for spelling in (self.library, alias):
                        with self.subTest(spelling=str(spelling)):
                            result = self.call("query", spelling)
                            self.assertEqual(result.returncode, 0, result.stderr)
                            self.assertEqual(json.loads(result.stdout)["library"], self.identity)
            finally:
                if self.namespace.exists():
                    shutil.rmtree(self.namespace)
                self.library, self.identity, self.namespace = previous

    def test_private_var_and_var_bind_the_same_physical_identity(self):
        parent = Path(subprocess.check_output(["/usr/bin/getconf", "DARWIN_USER_TEMP_DIR"], text=True).strip()).resolve()
        self.assertTrue(str(parent).startswith("/private/var/"))
        self.check_system_alias(parent)

    def test_private_tmp_and_tmp_bind_the_same_physical_identity(self):
        self.check_system_alias(Path("/private/tmp"))

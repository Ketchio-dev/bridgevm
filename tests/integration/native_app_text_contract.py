"""Terminal text is escaped while inventory JSON and saved bytes remain exact."""
import json
import tempfile
from pathlib import Path


def check_text(binary, run, tree):
    with tempfile.TemporaryDirectory(prefix="bridgevm-native-text-") as temporary:
        root = Path(temporary)
        library = root / "library\x1b[2J\nlibrary-line"
        entry = library / "synthetic"
        entry.mkdir(parents=True)
        config = {
            "id": "synthetic", "name": "synthetic", "displayName": "개발 VM\u202e\u009b",
            "backendKind": "hvf-engine\x1b[Hbackend-line",
            "bundlePath": str(entry) + "\nRuntime state: running\x1b]0;title\x07",
            "runnerPath": "", "launchSpecPath": "", "handoffPath": "",
            "sshKeyPath": "", "sshUser": "", "leasesPath": "", "guestName": "synthetic",
            "displayWidth": 1280, "displayHeight": 800,
        }
        (entry / "vm.json").write_text(json.dumps(config), encoding="utf-8")
        before = tree(root)
        for command in ("list", "inspect"):
            args = [command] + (["synthetic"] if command == "inspect" else [])
            args += ["--library", str(library)]
            output, error = run(binary, args, 0)
            text = output.decode("utf-8")
            assert not error
            assert not any(c in text for c in ("\x1b", "\x07", "\u009b", "\u202e"))
            assert "\nRuntime state: running" not in text and "\nlibrary-line" not in text
            assert "개발 VM" in text and "Runtime state: unobserved" in text
            output, error = run(binary, args + ["--json"], 0)
            value = json.loads(output)
            assert not error and value["libraryPath"] == str(library)
            record = value["records"][0]
            for key in ("displayName", "backendKind", "bundlePath"):
                assert record[key] == config[key], key
        assert tree(root) == before, "Text escaping changed persisted configuration"

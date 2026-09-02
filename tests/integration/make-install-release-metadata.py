#!/usr/bin/env python3
"""Write GitHub release/tag metadata fixtures for installer smoke tests."""

import json
import pathlib
import sys


def main() -> int:
    if len(sys.argv) != 6:
        raise SystemExit("usage: make-install-release-metadata.py DIR VERSION TARBALL MANIFEST COMMIT")
    root = pathlib.Path(sys.argv[1])
    version, tarball, manifest, commit = sys.argv[2:]
    release = {
        "tag_name": version,
        "draft": False,
        "prerelease": False,
        "assets": [
            {"name": tarball},
            {"name": manifest},
            {"name": "SHA256SUMS"},
        ],
    }
    (root / "github-release.json").write_text(json.dumps(release))
    (root / "github-commit.json").write_text(json.dumps({"sha": commit}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

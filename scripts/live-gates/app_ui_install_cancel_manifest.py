"""Separate fixed install cancellation scenario; shared signed bundle layout."""
import sys

import app_ui_host_v2_manifest as sealed

TIER = "d6-app-ui-install-cancel-v1"
FORMAT = "bridgevm-app-ui-install-cancel-v1"
FILES = sealed.FILES
LAUNCHER_KIND = "native-app-ui-install-cancel-launcher-v1"
LAUNCHER_FILE = "install-cancel-launcher-observations.json"


def parse_manifest(path, commit):
    return sealed.parse_manifest(path, commit, expected_format=FORMAT)


def validate_manifest(path, commit):
    return sealed.validate_manifest(path, commit, expected_format=FORMAT)


def seal_inputs(path, commit, staging):
    return sealed.seal_inputs(path, commit, staging, expected_format=FORMAT)


if __name__ == "__main__":
    try:
        if len(sys.argv) == 4 and sys.argv[1] == "validate-manifest":
            validate_manifest(*sys.argv[2:])
        elif len(sys.argv) == 5 and sys.argv[1] == "seal-inputs":
            seal_inputs(*sys.argv[2:])
        else:
            raise ValueError("expected validate-manifest or seal-inputs")
    except (OSError, ValueError) as error:
        print(f"install cancellation inputs refused: {error}", file=sys.stderr)
        sys.exit(2)

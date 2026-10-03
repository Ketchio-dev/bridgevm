"""Admitted D10 archive entry; no output precedes exact sealed receipt validation."""
import json
from pathlib import Path
import sys

from t22_pair_queue_archive import read

if __name__ == "__main__":
    try:
        value = read(Path(sys.argv[1]), sys.argv[2])
        if value["commit"] != sys.argv[3]: raise ValueError("D10 archive reader source differs from the job")
        json.dump(value, sys.stdout, indent=2, sort_keys=True); sys.stdout.write("\n")
    except (OSError, UnicodeError, ValueError, IndexError) as error:
        raise SystemExit(f"receipt refused: {error}") from None

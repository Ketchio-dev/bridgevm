"""Exit 0 when receipt redaction would publish this job id, 1 when it would not.

Every receipt carries its job id, and redact-receipt.py withholds a receipt
whose values look like a disk image, a vars file, a home path, or a vTPM or key
word. bridgevm-live submit asks here before burning an id, so such a job is
refused up front instead of running to the end with its receipt withheld. The
rule is redact-receipt.py's own, loaded from beside this file.
"""

import importlib.util
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
SPEC = importlib.util.spec_from_file_location("redact_receipt", HERE / "redact-receipt.py")
REDACTION = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(REDACTION)

if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(2)
    sys.exit(1 if REDACTION._value_is_secret(sys.argv[1]) else 0)

"""Allocation and retained request/result paths for import regression suites."""
import tempfile
from pathlib import Path
import product_e2e_identity_fixtures as fixture
from product_e2e_work_fixture import allocate


def prepare(case, prefix):
    case.temporary = tempfile.TemporaryDirectory(prefix=prefix)
    case.root = allocate(case.temporary.name)
    case.request_path, case.result_path = fixture.import_request(case.root)
    case.stamp = case.root.parent / "stamp.json"

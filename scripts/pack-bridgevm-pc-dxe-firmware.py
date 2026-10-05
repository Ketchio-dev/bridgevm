#!/usr/bin/env python3
"""Place the pinned vector and FV in the erased development flash layout."""
from pathlib import Path
import sys

artifact, vector_path, fv_path = map(Path, sys.argv[1:4])
size, offset, fv_size = map(int, sys.argv[4:])
vector, fv = vector_path.read_bytes(), fv_path.read_bytes()
assert len(fv) == fv_size and len(vector) <= offset
with artifact.open("wb") as stream:
    stream.write(b"\xff" * size)
    stream.seek(0)
    stream.write(vector)
    stream.seek(offset)
    stream.write(fv)

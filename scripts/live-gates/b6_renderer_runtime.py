"""Packaging prerequisites only; never proof of a working Venus device."""
import os

from b6_cell_inputs import small_bytes


def verify_renderer_runtime(records, environment=None):
    environment = os.environ if environment is None else environment
    if "RENDER_SERVER_EXEC_PATH" in environment:
        raise ValueError("renderer server environment override refused")
    server = records["render_server"][0]
    metadata = server.stat()
    if metadata.st_mode & 0o222 or not os.access(server, os.X_OK):
        raise ValueError("renderer server must be immutable and executable")
    encoded = os.fsencode(server)
    library = small_bytes(records["virglrenderer"][0], 128 * 1024 * 1024)
    if b"\0" + encoded + b"\0" not in b"\0" + library:
        raise ValueError("renderer library does not embed the sealed server path")

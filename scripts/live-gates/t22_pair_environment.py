"""Use system tools without inherited shell, loader or helper overrides."""
from contextlib import contextmanager
import os


def controlled_env():
    excluded = {"BASH_ENV", "ENV", "SHELLOPTS", "BASHOPTS", "CDPATH", "GLOBIGNORE", "IFS", "CARGO_TARGET_DIR"}
    prefixes = ("BRIDGEVM", "BASH_FUNC_", "DYLD_", "LD_", "PYTHON", "GIT_")
    env = {k: v for k, v in os.environ.items() if k not in excluded and not k.startswith(prefixes)}
    env.update({"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "PYTHONNOUSERSITE": "1"})
    return env


@contextmanager
def operations_environment():
    previous = dict(os.environ)
    try:
        current = controlled_env()
        os.environ.clear(); os.environ.update(current)
        yield
    finally:
        os.environ.clear(); os.environ.update(previous)

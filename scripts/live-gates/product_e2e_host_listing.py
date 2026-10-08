"""Bounded host-state observation; failures retain cleanup fences."""
import subprocess
import t17_guest_setup_harvest as HARVEST

def listing(*argv: str) -> bytes | None:
    """Tool output, or None when the tool fails or outlives the release bound."""
    try:
        completed = subprocess.run(argv, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                   stderr=subprocess.DEVNULL, env=HARVEST.TOOL_ENV,
                                   timeout=HARVEST.RELEASE_SECONDS, check=False)
    except (OSError, subprocess.TimeoutExpired):
        return None
    return completed.stdout if completed.returncode == 0 else None

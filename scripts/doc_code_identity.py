"""Recognize an explicitly labeled code-directory hash, not a Git reference."""
import re

LABEL = re.compile(r"\bCDHash[ \t]*(?:[:=][ \t]*)?(?:\n[ \t]*)?`?$", re.I)


def is_cdhash_reference(lines: list[str], number: int, offset: int) -> bool:
    """Only the immediate label may type this hash; nearby prose cannot."""
    previous = lines[number - 2] + "\n" if number > 1 else ""
    prefix = previous + lines[number - 1][:offset]
    return LABEL.search(prefix) is not None

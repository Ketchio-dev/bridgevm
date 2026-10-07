"""Generation attribution from the authenticated terminal report tail."""
from t17_terminal_report_tail import terminal_report

def observed_generation(detail: str, tail: bytes, tail_offset: int) -> int | None:
    parsed = terminal_report(tail, detail, tail_offset)
    return parsed[0] if parsed is not None else None

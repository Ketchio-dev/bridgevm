#!/usr/bin/env python3
"""Shared diagnostic sequence and strict guest application receipt contracts."""
import base64
import hashlib
import json
import os
import stat

FIRST = "BridgeVM"
SECOND = "\uD55C\uAE00\U0001F642"
EXPECTED_HASH = hashlib.sha256((FIRST + "\r\n" + SECOND).encode()).hexdigest()


def regular_bytes(path, limit):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    with os.fdopen(fd, "rb") as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            raise ValueError("not a regular diagnostic file")
        data = stream.read(limit + 1)
    if len(data) > limit:
        raise ValueError("diagnostic file exceeds bound")
    return data


def ready_coordinates(value, nonce):
    if (not isinstance(value, dict)
            or value.get("schema") != "bridgevm.input-sink-ready.v1"
            or value.get("nonce") != nonce or value.get("foreground") is not True
            or value.get("first_text") != FIRST
            or value.get("second_text_base64") != base64.b64encode(SECOND.encode()).decode()):
        raise ValueError("invalid foreground ready receipt")
    coords = (value.get("button_x"), value.get("button_y"))
    if any(type(n) is not int or not 0 <= n <= 32767 for n in coords):
        raise ValueError("invalid button coordinates")
    return coords


def check_result(value, nonce):
    if not isinstance(value, dict):
        raise ValueError("invalid result object")
    exact = {"schema": "bridgevm.input-sink.v1", "nonce": nonce,
             "passed": True, "clicked": True, "text_matches": True,
             "enter_count": 1, "enter_saw_first_text": True,
             "focus_lost": False, "actual_text_sha256": EXPECTED_HASH,
             "reason": "clicked"}
    if any(type(value.get(k)) is not type(v) or value.get(k) != v for k, v in exact.items()):
        raise ValueError("guest application input proof failed")


def input_sequence(x, y):
    if any(type(n) is not int or not 0 <= n <= 32767 for n in (x, y)):
        raise ValueError("invalid sequence coordinates")
    return [("TEXTINPUT", FIRST, len(FIRST.encode("utf-16-le"))),
            ("KEYINPUT", "enter", 2),
            ("TEXTINPUT", SECOND, len(SECOND.encode("utf-16-le"))),
            ("POINTERINPUT", "click:" + str(x) + "x" + str(y), 2)]


if __name__ == "__main__":
    print(json.dumps(input_sequence(123, 32767)))

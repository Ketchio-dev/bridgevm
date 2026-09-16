"""Actual CLI against bounded synthetic owner: transport evidence, not live app evidence."""
import json
from native_app_status_fixture import StatusFixture


def check_status(binary, library, run, tree):
    before = tree(library)
    args = ["status", "개발-vm", "--library", str(library)]
    output, error = run(binary, [*args, "--json"], 1)
    unavailable = json.loads(output)
    assert unavailable["schema"] == "bridgevm.app-runtime.v1" and not error
    assert unavailable["scope"] == "app-observation" and not unavailable["complete"]
    assert unavailable["runtimeState"] == "unobserved" and unavailable["sessions"] == []
    fixture = StatusFixture(library)
    try:
        output, error = run(binary, [*args, "--json"], 0)
        result = json.loads(output)
        assert not error and result["complete"] and result["runtimeState"] == "observed"
        assert result["appInstanceID"] == fixture.instance
        assert result["savedConfiguration"]["state"] == "present"
        assert result["sessions"][0]["ownership"] == "owned"
        assert result["sessions"][0]["configurationMatch"] == "same"
        assert b"private-key-path-must-not-be-rendered" not in output
        output, error = run(binary, args, 0)
        assert b"Session: owned, connection: booting" in output and not error
        missing = ["status", "removed-vm", "--library", str(library), "--json"]
        output, error = run(binary, missing, 0)
        result = json.loads(output)
        assert result["complete"] and not error
        assert result["savedConfiguration"]["state"] == "missing"
        assert result["sessions"][0]["configurationMatch"] == "unknown"
        assert [value["vmID"] for value in fixture.requests] == ["개발-vm", "개발-vm", "removed-vm"]
        assert len({value["requestID"] for value in fixture.requests}) == 3
    finally:
        fixture.close()
    assert tree(library) == before, "Status queries changed the native library"
    absent = library.parent / "absent-status-library"
    output, error = run(binary, ["status", "missing", "--library", str(absent), "--json"], 1)
    assert not json.loads(output)["complete"] and not error and not absent.exists()
    run(binary, ["status", "../bad", "--library", str(library)], 2)

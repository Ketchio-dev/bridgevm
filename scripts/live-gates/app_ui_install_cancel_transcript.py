"""Bind each fixed action and state capture to the independent AX reply bytes."""
from app_ui_diagnostic import digest
from app_ui_install_cancel_json import protocol_record

PHASES = ("idle", "validating", "cancelling", "cancelled", "retryFailed")
MUTATIONS = {"validating": "start", "cancelling": "cancel", "retryFailed": "retry"}
FAILURE = "합성 UI 검증 종료"
LABELS = ("대기", "설치 입력 확인", "취소 중…", "취소됨", "실패")


def completed(phase, values):
    index = PHASES.index(phase)
    return (values["installVisible"] and values["startVisible"]
        and values["startEnabled"] == (index in (0, 3, 4))
        and values["cancelVisible"] == (index in (1, 2))
        and values["cancelEnabled"] == (index == 1) and values["stage"] == LABELS[index]
        and (index in (1, 2) or values["title"] == ("설치 시작" if index == 0 else "다시 시도"))
        and values.get("failure") == (FAILURE if index == 4 else None))


def verify_values(value):
    booleans = {"installVisible", "startVisible", "startEnabled", "cancelVisible", "cancelEnabled"}
    if (not isinstance(value, dict) or not booleans | {"stage", "title"} <= set(value)
            or set(value) - booleans - {"stage", "title", "failure"}
            or any(type(value[key]) is not bool for key in booleans)
            or any(not isinstance(value[key], str) or len(value[key].encode()) > 1024 or "\0" in value[key] for key in ("stage", "title"))
            or "failure" in value and value["failure"] != FAILURE):
        raise ValueError("install AX state schema differs")


def verify_transcript(private, session, session_hash, count):
    expected_names = {f"{sequence:06d}.json" for sequence in range(1, count + 1)}
    for folder in ("driver-requests", "driver-replies"):
        path = private / folder
        if path != path.resolve(strict=True) or {entry.name for entry in path.iterdir()} != expected_names:
            raise ValueError("install AX transcript is incomplete or has extra records")
    phase_index, mutations, completed_reads, deadlines = 0, [], {}, {}
    phase_complete = False
    for sequence in range(1, count + 1):
        if phase_index == 4 and phase_complete:
            raise ValueError("install AX transcript continues after terminal retry read")
        path = private / "driver-requests" / f"{sequence:06d}.json"
        request = protocol_record(path, "native-app-ui-install-cancel-request",
            {"sessionSHA256", "nonce", "sequence", "phase", "phaseDeadlineUptime", "window", "operation"})
        reply = protocol_record(private / "driver-replies" / path.name, "native-app-ui-install-cancel-reply",
            {"sessionSHA256", "nonce", "sequence", "requestSHA256", "hostPID", "driverPID", "operation", "outcome"},
            {"values", "failureCode", "axError"})
        for record in (request, reply):
            if (record["sessionSHA256"] != session_hash or record["nonce"] != session["nonce"]
                    or type(record["sequence"]) is not int or record["sequence"] != sequence):
                raise ValueError("install AX transcript binding differs")
        phase = request["phase"]
        if phase not in PHASES or PHASES.index(phase) not in (phase_index, phase_index + 1):
            raise ValueError("install AX phases are out of order")
        if PHASES.index(phase) != phase_index:
            if not phase_complete:
                raise ValueError("install AX phase advanced without observed completion")
            phase_index += 1
            phase_complete = False
        operation = request["operation"]
        deadline = deadlines.setdefault(phase, request["phaseDeadlineUptime"])
        if (request["window"] != "main" or operation not in ("readState", MUTATIONS.get(phase))
                or type(request["phaseDeadlineUptime"]) not in (int, float)
                or not session["startedUptime"] < request["phaseDeadlineUptime"] <= session["deadlineUptime"]
                or request["phaseDeadlineUptime"] != deadline
                or reply["requestSHA256"] != digest(path) or reply["operation"] != operation
                or any(type(reply[key]) is not int or reply[key] != session[role]["pid"]
                       for key, role in (("hostPID", "host"), ("driverPID", "driver")))
                or reply.get("failureCode") is not None or reply.get("axError") is not None):
            raise ValueError("install AX operation identity or result differs")
        if operation == "readState":
            if reply["outcome"] != "observed" or phase in MUTATIONS and MUTATIONS[phase] not in mutations:
                raise ValueError("install AX read was not observed")
            verify_values(reply.get("values"))
            phase_complete = completed(phase, reply["values"])
            if phase_complete:
                completed_reads[sequence] = phase
        else:
            if reply["outcome"] != "performed" or reply.get("values") is not None or operation in mutations:
                raise ValueError("install AX mutation was refused or replayed")
            mutations.append(operation)
            phase_complete = False
    if mutations != ["start", "cancel", "retry"] or set(completed_reads.values()) != set(PHASES) or completed_reads.get(count) != "retryFailed":
        raise ValueError("install AX fixed scenario is incomplete")
    return completed_reads

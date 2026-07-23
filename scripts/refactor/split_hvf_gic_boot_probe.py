"""Split the HVF GIC boot probe example into responsibility modules.

This is deliberately narrower than regroup.py: the input is an example crate
root plus one nested facade, and every parsed item/method is assigned by its
original source line. Function and method bodies are emitted verbatim. The only
textual edits widen implementation-only visibility so sibling modules can use
moved state without exposing a public API from this binary example.
"""
from __future__ import annotations

import importlib.util
import os
import re
from pathlib import Path

ROOT = Path("crates/bridgevm-hvf/examples/hvf_gic_boot_probe.rs")
DIR = ROOT.with_suffix("")
AGENT = DIR / "agent_console.rs"


def load_parser():
    path = Path("scripts/refactor/regroup.py").resolve()
    source = path.read_text().split("header, parsed = [], []")[0]
    source = source.replace("D, SPEC = sys.argv[1], sys.argv[2]", "D = SPEC = None")
    source = source.replace("APPLY = '--apply' in sys.argv", "APPLY = False")
    source = source.replace(
        "spec = json.load(open(SPEC))",
        "spec = {'targets': {}, 'items': {}, 'methods': {}}",
    )
    scope = {"__name__": "split_hvf_gic_boot_probe", "__file__": str(path)}
    exec(source, scope)
    return scope["parse"]


parse = load_parser()


def line_starts(path: Path):
    lines = path.read_text().splitlines()
    return lines, {id(line): i + 1 for i, line in enumerate(lines)}


def start_line(all_lines: list[str], body: list[str]) -> int:
    needle = body[0]
    candidates = [i + 1 for i, line in enumerate(all_lines) if line == needle]
    if len(candidates) == 1:
        return candidates[0]
    joined = "\n".join(body[: min(3, len(body))])
    for line in candidates:
        if "\n".join(all_lines[line - 1 : line - 1 + min(3, len(body))]) == joined:
            return line
    raise RuntimeError(f"cannot locate item beginning {needle!r}")


def widen_item(lines: list[str], *, fields: bool = True) -> list[str]:
    out = list(lines)
    for i, line in enumerate(out):
        stripped = line.lstrip()
        indent = line[: len(line) - len(stripped)]
        if indent or stripped.startswith(("#", "//")) or not stripped:
            continue
        if re.match(r"(?:unsafe\s+)?impl\b", stripped) or stripped.startswith("pub "):
            break
        if re.match(r"(?:unsafe\s+)?(?:fn|struct|enum|const|static|type|union)\b", stripped):
            prefix = "unsafe " if stripped.startswith("unsafe ") else ""
            rest = stripped[len(prefix) :]
            out[i] = f"pub(crate) {prefix}{rest}"
            break
    if fields:
        depth = 0
        for i, line in enumerate(out):
            depth += line.count("{") - line.count("}")
            if depth != 1:
                continue
            if re.match(r"^    [A-Za-z_]\w*\s*:", line):
                out[i] = "    pub(crate) " + line[4:]
    return out


def widen_method(lines: list[str], visibility: str) -> list[str]:
    out = list(lines)
    for i, line in enumerate(out):
        if re.match(r"^    (?:default )?(?:const )?(?:async )?(?:unsafe )?(?:extern \"[^\"]*\" )?fn\b", line):
            out[i] = "    " + visibility + " " + line[4:]
            return out
        if re.match(r"^    pub(?:\([^)]*\))? fn\b", line):
            return out
    return out


def root_target(line: int) -> str | None:
    ranges = [
        (143, 364, "guest_memory.rs"),
        (365, 518, "hvf_abi.rs"),
        (519, 705, "smp_trace.rs"),
        (706, 1050, "vcpu_coordination.rs"),
        (1051, 1856, "secondary_vcpu.rs"),
        (1857, 1978, "exception_trace.rs"),
        (1979, 2037, "probe_env.rs"),
        (2038, 2211, "reboot_watchdog.rs"),
        (2212, 2304, "vcpu_debug.rs"),
        (2305, 2558, "guest_diagnostics.rs"),
        (2559, 2737, "storage_reporting.rs"),
        (2738, 3128, "boot_telemetry.rs"),
        (3129, 3376, "interrupt_delivery.rs"),
        (3377, 3593, "wfi_diagnostics.rs"),
        (3594, 3625, "probe_env.rs"),
        (3626, 3661, "interrupt_delivery.rs"),
        (3662, 3702, "host_support.rs"),
        (3703, 3726, "exception_trace.rs"),
    ]
    return next((name for lo, hi, name in ranges if lo <= line <= hi), None)


def emit_root_modules() -> None:
    lines = ROOT.read_text().splitlines()
    _, items = parse(str(ROOT))
    buckets: dict[str, list[str]] = {}
    assigned = []
    for kind, name, body, methods, attrs in items:
        line = start_line(lines, attrs + body if kind == "impl" and attrs else body)
        target = root_target(line)
        if target is None:
            continue
        assigned.append((line, name, target))
        if kind == "item":
            text = widen_item(body, fields=any(re.match(r"^(?:pub )?struct\b", line) for line in body))
        else:
            inherent = " for " not in name
            widened_methods = [
                widen_method(mbody, "pub(crate)") if inherent else mbody
                for _, mbody in methods
            ]
            text = attrs + body + [line for method in widened_methods for line in method] + ["}"]
        if target == "hvf_abi.rs" and name is None:
            text = [re.sub(r"^(    )fn ", r"\1pub(crate) fn ", value) for value in text]
        buckets.setdefault(target, []).append("\n".join(text))

    expected = [
        (start_line(lines, attrs + body if kind == "impl" and attrs else body), name)
        for kind, name, body, methods, attrs in items
        if 143 <= start_line(lines, attrs + body if kind == "impl" and attrs else body) <= 3726
    ]
    if len(assigned) != len(expected):
        raise RuntimeError(f"root assignment mismatch: {len(assigned)} != {len(expected)}")

    docs = {
        "guest_memory.rs": "Guest RAM mappings and GPU shared-memory mapping.",
        "hvf_abi.rs": "Hypervisor.framework ABI declarations, constants, and lifetime guards.",
        "smp_trace.rs": "SMP lock and vCPU progress tracing.",
        "vcpu_coordination.rs": "Shared vCPU lifecycle, automation, and pre-run coordination.",
        "secondary_vcpu.rs": "Secondary vCPU creation, PSCI handling, and run-loop execution.",
        "exception_trace.rs": "Exception syndrome and trapped system-register diagnostics.",
        "probe_env.rs": "Probe environment parsing and report interval configuration.",
        "reboot_watchdog.rs": "Reboot policy, terminal PSCI actions, and boot watchdogs.",
        "vcpu_debug.rs": "vCPU register reset, context, and watchpoint helpers.",
        "guest_diagnostics.rs": "Guest memory, reset snapshot, and stage-1 translation diagnostics.",
        "storage_reporting.rs": "Storage persistence and block/network trace reporting.",
        "boot_telemetry.rs": "Serial milestone scanning and boot timer telemetry.",
        "interrupt_delivery.rs": "Pending interrupt draining, delivery, and run-loop accounting.",
        "wfi_diagnostics.rs": "WFI/WFE instruction and wake diagnostics.",
        "host_support.rs": "Host file mapping, clock, and serial stop helpers.",
    }
    for target, chunks in buckets.items():
        text = f"//! {docs[target]}\n\nuse crate::*;\n\n" + "\n\n".join(chunks) + "\n"
        (DIR / target).write_text(text)

    kept = lines[:142] + lines[3726:]
    declarations = []
    for target in buckets:
        module = target[:-3]
        declarations += [f'#[path = "hvf_gic_boot_probe/{target}"]', f"mod {module};"]
    declarations += [f"pub(crate) use {target[:-3]}::*;" for target in buckets]
    ROOT.write_text("\n".join(kept[:142] + declarations + [""] + kept[142:]) + "\n")


def agent_item_target(line: int) -> str | None:
    ranges = [
        (23, 274, "state.rs"),
        (1659, 1698, "service_wake.rs"),
        (1699, 1898, "protocol.rs"),
        (1899, 1913, "control_file.rs"),
        (1914, 2146, "share.rs"),
        (2147, 2209, "clipboard.rs"),
        (2210, 2238, "config.rs"),
        (2239, 3232, "tests.rs"),
    ]
    return next((name for lo, hi, name in ranges if lo <= line <= hi), None)


PROTOCOL_METHODS = {
    "t_ms", "service_wake_needed", "per_exit_tick_needed", "desktop_ready", "from_env", "tick",
    "handle_line", "handle_reply_line", "handle_unlabelled_get_fragment", "send_next_command_or_done",
    "next_command_line", "begin_get", "accum_get_chunk", "finish_get", "take_finished_get",
    "begin_out", "accum_out_chunk", "finish_out",
}


def emit_agent_modules() -> None:
    lines = AGENT.read_text().splitlines()
    header, items = parse(str(AGENT))
    buckets: dict[str, list[str]] = {}
    assigned_items = 0
    assigned_methods = 0
    total_methods = 0

    for kind, name, body, methods, attrs in items:
        line = start_line(lines, attrs + body if kind == "impl" and attrs else body)
        if line <= 22:
            continue
        if kind == "item":
            target = agent_item_target(line)
            if target is None:
                raise RuntimeError(f"unassigned agent item {name} at {line}")
            assigned_items += 1
            widened = widen_item(body, fields=any(re.match(r"^(?:pub )?struct\b", value) for value in body))
            # The harness itself is re-exported publicly, but its implementation
            # state is only shared within the agent_console module tree.
            if name == "AgentConsoleHarness":
                widened = [re.sub(r"^pub\(crate\) struct ", "pub struct ", value) for value in widened]
                widened = [re.sub(r"^    pub\(crate\) ", "    pub(super) ", value) for value in widened]
            elif name != "ServiceWake":
                widened = [re.sub(r"^pub\(crate\) ", "pub(super) ", value) for value in widened]
                widened = [re.sub(r"^    pub\(crate\) ", "    pub(super) ", value) for value in widened]
            buckets.setdefault(target, []).append("\n".join(widened))
            continue

        total_methods += len(methods)
        grouped: dict[str, list[list[str]]] = {}
        inherent = " for " not in name
        for method_name, method_body in methods:
            if name == "AgentConsoleHarness":
                target = "harness_protocol.rs" if method_name in PROTOCOL_METHODS else "resident_service.rs"
            elif name == "ServiceWake":
                target = "service_wake.rs"
            else:
                target = "state.rs"
            visibility = "pub(super)"
            grouped.setdefault(target, []).append(
                widen_method(method_body, visibility) if inherent else method_body
            )
            assigned_methods += 1
        for target, method_groups in grouped.items():
            block = attrs + body + [value for method in method_groups for value in method] + ["}"]
            buckets.setdefault(target, []).append("\n".join(block))

    if assigned_methods != total_methods:
        raise RuntimeError(f"agent method assignment mismatch: {assigned_methods} != {total_methods}")

    docs = {
        "state.rs": "Agent console state and request model.",
        "harness_protocol.rs": "Agent console scripted protocol and command response handling.",
        "resident_service.rs": "Resident clipboard, control, and shared-folder service scheduling.",
        "service_wake.rs": "Host-driven vCPU wake scheduling for resident services.",
        "protocol.rs": "Line framing, base64, and command wire encoding.",
        "control_file.rs": "Bounded control-file ingestion.",
        "share.rs": "Host-share scanning, validation, and request construction.",
        "clipboard.rs": "Clipboard normalization and synchronization decisions.",
        "config.rs": "Agent console environment and command parsing.",
        "tests.rs": "Agent console protocol and service regression tests.",
    }
    subdir = DIR / "agent_console"
    subdir.mkdir(exist_ok=True)
    for target, chunks in buckets.items():
        import_parent = "" if target in {"clipboard.rs", "protocol.rs"} else "use super::*;\n\n"
        text = f"//! {docs[target]}\n\n{import_parent}" + "\n\n".join(chunks) + "\n"
        output = DIR / "agent_console_tests.rs" if target == "tests.rs" else subdir / target
        output.write_text(text)

    facade = lines[:22]
    declarations = []
    for target in buckets:
        module = "agent_console_tests" if target == "tests.rs" else target[:-3]
        path = "agent_console_tests.rs" if target == "tests.rs" else f"agent_console/{target}"
        if target == "tests.rs":
            declarations.append("#[cfg(test)]")
        declarations += [f'#[path = "{path}"]', f"mod {module};"]
    declarations += [
        "pub use service_wake::ServiceWake;",
        "pub use state::AgentConsoleHarness;",
        "use clipboard::*;",
        "use config::*;",
        "use control_file::*;",
        "use protocol::*;",
        "use share::*;",
        "use state::*;",
    ]
    AGENT.write_text("\n".join(facade + [""] + declarations) + "\n")


if __name__ == "__main__":
    emit_root_modules()
    emit_agent_modules()
    for path in [ROOT, AGENT, *sorted(DIR.glob("*.rs")), *sorted((DIR / "agent_console").glob("*.rs"))]:
        count = len(path.read_text().splitlines())
        print(f"{count:4} {path}")

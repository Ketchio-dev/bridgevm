"""The HVF runtime's clean guest power-off stop record, exactly as printed.

final_report.rs writes `stop: {stop_reason}` and probe_runtime.rs formats the
reason from the hvf_abi.rs PSCI_SYSTEM_OFF function ID;
tests/integration/hvf-stop-line-contract.py rebuilds this value from that source.
"""

SYSTEM_OFF = "stop: PSCI 0x84000008 (system off)"

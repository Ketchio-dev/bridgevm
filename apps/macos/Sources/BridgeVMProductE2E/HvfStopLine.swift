/// Stop records as the HVF runtime prints them: final_report.rs writes
/// `stop: {stop_reason}` and probe_runtime.rs formats PSCI terminal reasons from
/// the hvf_abi.rs function IDs. tests/integration/hvf-stop-line-contract.py
/// rebuilds both values from that source.
enum HvfStopLine {
    static let systemOff = "stop: PSCI 0x84000008 (system off)"
    /// Every SYSTEM_RESET stop, including process recreation (exit 42).
    static let systemResetPrefix = "stop: PSCI 0x84000009 "

    /// Only the final report's host-framed stop record counts (HvfTerminalReport).
    static func systemOffObserved(in text: String) -> Bool {
        HvfTerminalReport.stop(in: text)?.line == systemOff
    }
}

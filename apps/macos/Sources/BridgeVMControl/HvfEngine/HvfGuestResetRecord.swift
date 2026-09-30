/// Guest reset records as the HVF runtime prints them into run.log. Product runs
/// set BRIDGEVM_EXIT_ON_RESET=1, so the helper prints `processRecreation`, exits
/// 42 and the next generation appends to the same log; the installed-boot
/// wrapper reboots in process instead. tests/integration/hvf-reset-record-contract.py
/// rebuilds these values from the Rust format strings. Guest serial also reaches
/// run.log, so only whole records match.
enum HvfGuestResetRecord {
    static let processRecreation = "stop: PSCI 0x84000009 exiting for process recreation (exit 42)"
    /// Followed by `{reboot}/{max}`.
    static let inProcessRebootPrefix = "PSCI SYSTEM_RESET: reboot "
    /// Around `{max}` when the reboot limit stops the run.
    static let rebootLimitPrefix = "stop: PSCI 0x84000009 max reboot count "
    static let rebootLimitSuffix = " reached"

    /// One trailing CR is ignored, as the detectors already do.
    static func matches(_ raw: String) -> Bool {
        let line = raw.hasSuffix("\r") ? raw.dropLast() : Substring(raw)
        if line == processRecreation { return true }
        if line.hasPrefix(inProcessRebootPrefix) {
            let counts = line.dropFirst(inProcessRebootPrefix.count)
                .split(separator: "/", omittingEmptySubsequences: false)
            return counts.count == 2 && counts.allSatisfy(isCount)
        }
        guard line.hasPrefix(rebootLimitPrefix), line.hasSuffix(rebootLimitSuffix) else { return false }
        return isCount(line.dropFirst(rebootLimitPrefix.count).dropLast(rebootLimitSuffix.count))
    }

    private static func isCount(_ text: Substring) -> Bool {
        !text.isEmpty && text.utf8.allSatisfy { $0 >= 0x30 && $0 <= 0x39 }
    }
}

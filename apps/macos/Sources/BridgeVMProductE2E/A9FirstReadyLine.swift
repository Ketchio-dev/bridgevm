/// The run-log line that proves the imported guest's first boot reached the agent. The host prints the proactive
/// PONG when the guest's one-shot READY was dropped, so both count, as for T17's first boot, the journey's later
/// boots and the receipt verifier.
enum A9FirstReadyLine {
    static func first(in lines: [String]) -> String? {
        lines.first { $0.hasPrefix("BVAGENT READY") || $0.hasPrefix("BVAGENT PONG (proactive)") }
    }
}

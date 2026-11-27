import Foundation

/// The relay rebuilds its agent harness per boot generation and starts tailing
/// the control file at its current end, so a line appended before
/// `BVAGENT SERVICE start` can be skipped and its reply never arrives.
enum HvfAgentServiceGate {
    static let serviceNotStartedMessage = "Wait for Guest Tools to start in Windows, then try the command again."
}

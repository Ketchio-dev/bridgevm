import Foundation

protocol VMOwnedLaunchingBackend: VMBackend {
    /// A conforming backend's nil plan is a refusal, never a legacy fallback.
    func ownedLaunchPlan() -> GenericOwnedLaunchPlan?
}

enum GenericOwnedProcessOutput: Equatable, Sendable {
    case discard
    /// Truncate at launch; stdout and stderr share this destination.
    case file(String)
}

struct GenericOwnedProcessSpec: Equatable, Sendable {
    let executable: String
    let arguments: [String]
    let environmentOverrides: [String: String]
    let output: GenericOwnedProcessOutput

    init(executable: String, arguments: [String], environmentOverrides: [String: String] = [:],
         output: GenericOwnedProcessOutput = .discard) {
        self.executable = executable; self.arguments = arguments
        self.environmentOverrides = environmentOverrides; self.output = output
    }
}

struct GenericOwnedTPMPreparation: Equatable, Sendable {
    let process: GenericOwnedProcessSpec
    /// A matching existing dependency is reusable but never owned or signalled.
    let reusePattern: String
    /// Applies after either reuse or fresh helper launch.
    let launchDelay: TimeInterval

    init(process: GenericOwnedProcessSpec, reusePattern: String, launchDelay: TimeInterval = 1.5) {
        self.process = process; self.reusePattern = reusePattern; self.launchDelay = launchDelay
    }
}

struct GenericOwnedLaunchPlan: Equatable, Sendable {
    let primary: GenericOwnedProcessSpec
    let directories: [String]
    let tpm: GenericOwnedTPMPreparation?

    init(primary: GenericOwnedProcessSpec, directories: [String] = [], tpm: GenericOwnedTPMPreparation? = nil) {
        self.primary = primary; self.directories = directories; self.tpm = tpm
    }
}

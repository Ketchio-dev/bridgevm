#if (DEBUG && BRIDGEVM_APP_UI_HOST) || BRIDGEVM_APP_UI_DRIVER
import Foundation

enum AppUIDriverOperation: String, Codable, CaseIterable {
    case welcomeControls, pressCreate, createForm, cancelCreate
    case pressImport, importForm, pressOverview, overviewCards
    case setSearchIndigo, clearSearch, searchState
}

enum AppUIDriverWindow: String, Codable { case main, creationSheet }
enum AppUIDriverPhase: String, Codable, CaseIterable {
    case welcome, creation, cancellation, importing, overview, filtering, clearing
}
enum AppUIDriverOutcome: String, Codable { case observed, performed, refused }
enum AppUIDriverFailure: String, Codable, Error {
    case accessibilityUntrusted = "accessibility-untrusted"
    case identityMismatch = "identity-mismatch"
    case invalidSession = "invalid-session"
    case invalidRequest = "invalid-request"
    case invalidReply = "invalid-reply"
    case invalidFile = "invalid-file"
    case fileExists = "file-exists"
    case ioFailure = "io-failure"
    case cancelled, deadlineExceeded = "deadline-exceeded"
    case outOfOrder = "out-of-order"
    case replayedMutation = "replayed-mutation"
    case protocolLimit = "protocol-limit"
    case axFailure = "ax-failure"
    case elementNotFound = "element-not-found"
    case ambiguousElement = "ambiguous-element"
    case elementDisabled = "element-disabled"
    case invalidElement = "invalid-element"
    case textReadbackMismatch = "text-readback-mismatch"
    case windowUnavailable = "window-unavailable"
    case internalFailure = "internal-failure"
}

enum AppUIDriverConstants {
    static let maximumBytes = 8_192
    static let maximumSequence: UInt32 = 1_024
    static let mainWindowIdentifier = "bridgevm.app-ui-host.main"
    static let sheetWindowIdentifier = "bridgevm.app-ui-host.creation-sheet"
    static let hostBundleIdentifier = "dev.bridgevm.app-ui-host"
    static let driverBundleIdentifier = "dev.bridgevm.app-ui-driver"
}

struct AppUIDriverProcessIdentity: Codable, Equatable {
    let pid: Int32
    let launchDate: Double
    let bundleIdentifier: String
    let bundlePath: String
    let executablePath: String
    let executableSHA256: String
}

struct AppUIDriverSession: Codable, Equatable {
    var schemaVersion = 1
    var kind = "native-app-ui-driver-session"
    let nonce: String
    let startedUptime: Double
    let deadlineUptime: Double
    let host: AppUIDriverProcessIdentity
    let driver: AppUIDriverProcessIdentity
}

struct AppUIDriverRequest: Codable, Equatable {
    var schemaVersion = 1
    var kind = "native-app-ui-driver-request"
    let sessionSHA256: String
    let nonce: String
    let sequence: UInt32
    let phase: AppUIDriverPhase
    let phaseDeadlineUptime: Double
    let window: AppUIDriverWindow
    let operation: AppUIDriverOperation
}

struct AppUIDriverReply: Codable, Equatable {
    var schemaVersion = 1
    var kind = "native-app-ui-driver-reply"
    let sessionSHA256: String
    let nonce: String
    let sequence: UInt32
    let requestSHA256: String
    let hostPID: Int32
    let driverPID: Int32
    let operation: AppUIDriverOperation
    let outcome: AppUIDriverOutcome
    let values: AppUIDriverValues?
    let failureCode: AppUIDriverFailure?
    let axError: Int32?
}

struct AppUIDriverReady: Codable, Equatable {
    var schemaVersion = 1
    var kind = "native-app-ui-driver-ready"
    let sessionSHA256: String
    let nonce: String
    let driver: AppUIDriverProcessIdentity
    let trusted: Bool
    let failureCode: AppUIDriverFailure?
    let codeRequirement: String?
    let codeRequirementUnavailableReason: String?
}
#endif

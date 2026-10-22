import Foundation
import Combine

@MainActor
final class ControlModel: ObservableObject {
    static let terminalLogCharacterLimit = 200_000
    static let softwareLogCharacterLimit = 100_000
    // Status
    @Published var running = false { didSet { finishRetainedObservationIfTerminal() } }
    @Published var ip: String = "—"
    @Published var memGiB: Double = 8
    @Published var cpu: Int = 4
    @Published var busy = false { didSet { finishRetainedObservationIfTerminal() } }
    @Published private(set) var lifecycleBusy = false { didSet { finishRetainedObservationIfTerminal() } }
    @Published var statusNote = ""

    // Resource editor (pending values until Apply)
    @Published var pendingMemGiB: Double = 8
    @Published var pendingCPU: Double = 4

    // Terminal
    @Published var terminalInput = ""
    @Published var terminalLog = "BridgeVM Control — 게스트 터미널\n준비되면 명령을 입력하세요 (예: uname -a, df -h).\n\n"

    // Software install output
    @Published var softwareLog = ""

    private(set) var config: VMConfig
    let backend: VMBackend
    var workAdmission: LibraryWorkAdmission?
    var legacyStopAdmission: LibraryWorkAdmission?
    var acceptedResourceConfiguration: VMConfig?
    var onResourceUpdateFailed: (@MainActor () -> Void)?
    var onConfigurationPersisted: (@MainActor () -> Void)?
    var timer: Timer?
    var startConfirmationDeadline: Date?
    var refreshGeneration: UInt64 = 0
    var tick = 0
    var ownedRuntime: GenericOwnedRuntime?
    var retainedObservationEnded = false
    private(set) var isRetained = false

    func retainForRecovery() { isRetained = true }
    func setLifecycleBusy(_ value: Bool) { lifecycleBusy = value }

    func publishSavedResources(memoryMiB: Int, cpuCount: Int) {
        invalidateRefreshes()
        config.memMiB = memoryMiB; config.cpuCount = cpuCount
        acceptedResourceConfiguration = nil
        memGiB = Double(memoryMiB) / 1024.0; cpu = cpuCount
        onConfigurationPersisted?()
    }

    init(config: VMConfig, backend: VMBackend? = nil, startsAutomatically: Bool = true) {
        self.config = config
        self.backend = backend ?? config.makeBackend()
        if startsAutomatically {
            refresh()
            startPolling()
        }
    }

    deinit { timer?.invalidate() }

    var displayName: String { backend.displayName }
    var hasAcceptedOperation: Bool { lifecycleBusy || busy || startConfirmationDeadline != nil }

}

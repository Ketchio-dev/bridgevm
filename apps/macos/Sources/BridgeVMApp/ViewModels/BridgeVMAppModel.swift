import Foundation

protocol StoreDoctorInspecting {
    func inspectStoreDoctor() async throws -> StoreDoctorReport
}

@MainActor
final class BridgeVMAppModel: ObservableObject {
    enum StoreDoctorState: Equatable {
        case idle
        case checking
        case ready(StoreDoctorReport)
        case failed(String)
    }

    let settings: AppSettings
    let dashboardModel: DashboardViewModel
    @Published private(set) var storeDoctorState: StoreDoctorState = .idle
    @Published private(set) var bundledDaemonLaunchReport: BundledDaemonLaunchReport?
    private let clientFactory: @MainActor (AppSettings) -> VirtualMachineClient
    private let doctorClientFactory: (AppSettings) -> any StoreDoctorInspecting
    private var storeDoctorGeneration = 0

    init(
        settings: AppSettings = AppSettings(),
        clientFactory: @escaping @MainActor (AppSettings) -> VirtualMachineClient = BridgeVMAppModel.makeClient,
        doctorClientFactory: @escaping (AppSettings) -> any StoreDoctorInspecting = { (settings: AppSettings) -> any StoreDoctorInspecting in
            BridgeVMAppModel.makeDoctorClient(settings: settings)
        }
    ) {
        self.settings = settings
        self.clientFactory = clientFactory
        self.doctorClientFactory = doctorClientFactory
        self.bundledDaemonLaunchReport = BundledDaemonSupervisor.shared.startIfNeeded(settings: settings)
        self.dashboardModel = DashboardViewModel(
            client: clientFactory(settings)
        )
    }

    func applySettings() {
        bundledDaemonLaunchReport = BundledDaemonSupervisor.shared.startIfNeeded(settings: settings)
        dashboardModel.updateClient(clientFactory(settings))
        settings.markApplied()
        storeDoctorGeneration += 1
        storeDoctorState = .idle
        Task {
            await dashboardModel.load()
        }
    }

    func checkStoreDoctor() {
        guard settings.hasValidDaemonSettings else {
            storeDoctorState = .failed("Enter a daemon Unix socket path or enable mock inventory.")
            return
        }

        storeDoctorState = .checking
        storeDoctorGeneration += 1
        let generation = storeDoctorGeneration
        let client = doctorClientFactory(settings)
        Task {
            do {
                let report = try await client.inspectStoreDoctor()
                guard generation == storeDoctorGeneration else { return }
                storeDoctorState = .ready(report)
            } catch {
                guard generation == storeDoctorGeneration else { return }
                storeDoctorState = .failed(error.localizedDescription)
            }
        }
    }

    private static func makeClient(settings: AppSettings) -> VirtualMachineClient {
        let mock = MockVirtualMachineClient()
        guard !settings.useMockInventory else {
            return mock
        }

        return DaemonVirtualMachineClient(
            endpoint: DaemonEndpoint(socketPath: settings.effectiveDaemonSocketPath)
        )
    }

    private nonisolated static func makeDoctorClient(settings: AppSettings) -> VirtualMachineClient {
        if settings.useMockInventory {
            return MockVirtualMachineClient()
        }
        return DaemonVirtualMachineClient(
            endpoint: DaemonEndpoint(socketPath: settings.effectiveDaemonSocketPath)
        )
    }
}

final class AppSettings: ObservableObject {
    struct Snapshot: Equatable {
        var daemonSocketPath: String
        var useMockInventory: Bool
        var allowAppleVzRealStart: Bool
    }

    private enum Key {
        static let daemonSocketPath = "bridgevm.daemonSocketPath"
        static let useMockInventory = "bridgevm.useMockInventory"
        static let allowAppleVzRealStart = "bridgevm.allowAppleVzRealStart"
    }

    private let defaults: UserDefaults
    @Published private var appliedSnapshot: Snapshot

    @Published var daemonSocketPath: String {
        didSet {
            defaults.set(daemonSocketPath, forKey: Key.daemonSocketPath)
        }
    }

    @Published var useMockInventory: Bool {
        didSet {
            defaults.set(useMockInventory, forKey: Key.useMockInventory)
        }
    }

    @Published var allowAppleVzRealStart: Bool {
        didSet {
            defaults.set(allowAppleVzRealStart, forKey: Key.allowAppleVzRealStart)
        }
    }

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        let daemonSocketPath = defaults.string(forKey: Key.daemonSocketPath)
            ?? DaemonEndpoint.local.socketPath
        let useMockInventory = defaults.object(forKey: Key.useMockInventory) as? Bool ?? false
        let allowAppleVzRealStart =
            defaults.object(forKey: Key.allowAppleVzRealStart) as? Bool ?? false
        self.daemonSocketPath = daemonSocketPath
        self.useMockInventory = useMockInventory
        self.allowAppleVzRealStart = allowAppleVzRealStart
        self.appliedSnapshot = Snapshot(
            daemonSocketPath: daemonSocketPath,
            useMockInventory: useMockInventory,
            allowAppleVzRealStart: allowAppleVzRealStart
        )
    }

    var hasPendingChanges: Bool {
        appliedSnapshot != currentSnapshot
    }

    var effectiveDaemonSocketPath: String {
        let trimmed = daemonSocketPath.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? DaemonEndpoint.local.socketPath : trimmed
    }

    var hasValidDaemonSettings: Bool {
        useMockInventory || !effectiveDaemonSocketPath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    var daemonModeSummary: String {
        if useMockInventory {
            return "Using mock inventory. Daemon socket settings are kept but not used."
        }
        if allowAppleVzRealStart {
            return "Using bridgevmd at \(effectiveDaemonSocketPath). Apple VZ live starts are enabled."
        }
        if daemonSocketPath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return "Using default bridgevmd socket at \(effectiveDaemonSocketPath)."
        }
        return "Using bridgevmd at \(effectiveDaemonSocketPath). Connection errors are shown in the dashboard; enable mock inventory explicitly for demo data."
    }

    func resetDaemonSocketPath() {
        daemonSocketPath = DaemonEndpoint.local.socketPath
    }

    func markApplied() {
        appliedSnapshot = currentSnapshot
    }

    private var currentSnapshot: Snapshot {
        Snapshot(
            daemonSocketPath: daemonSocketPath,
            useMockInventory: useMockInventory,
            allowAppleVzRealStart: allowAppleVzRealStart
        )
    }
}

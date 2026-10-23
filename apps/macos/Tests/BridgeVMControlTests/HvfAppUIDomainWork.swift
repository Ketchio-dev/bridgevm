import Foundation
@testable import BridgeVMControl

/// The XCTest-tier copy of the host's domain-work accounting (AppUIHostDomainWork):
/// fatal tripwires stay zero; only one control model over an inert backend per
/// seeded fixture VM is admitted, none before seeding.
@MainActor
final class HvfAppUIDomainWork {
    private(set) var tripwires = Dictionary(uniqueKeysWithValues: ["runtime_creations", "install_creations",
        "file_jobs", "backend_calls"].map { ($0, 0) })
    private(set) var beforeSeeding = 0
    private(set) var afterSeeding = 0
    private var seeded: Set<String>?
    private var created: Set<String> = []

    func trip(_ name: String) {
        precondition(tripwires[name] != nil || name == "model_creations", "Unknown domain-work tripwire")
        tripwires[name]? += 1
    }

    func seed(_ configs: [VMConfig]) -> [VMConfig] {
        precondition(seeded == nil, "Overview fixture VMs are seeded once")
        seeded = Set(configs.map(\.slug))
        return configs
    }

    func admit(_ config: VMConfig) -> Bool {
        guard let seeded else { beforeSeeding += 1; return false }
        afterSeeding += 1
        return seeded.contains(config.slug) && created.insert(config.slug).inserted
    }

    var modelCreations: [String: Int] {
        ["before_seeding": beforeSeeding, "after_seeding": afterSeeding, "seeded_fixture_vms": seeded?.count ?? 0]
    }
}

/// Every method would reach a process, launch, Keychain or file in production.
final class HvfAppUIInertBackend: VMBackend, Sendable {
    private let refuse: @Sendable () -> Never
    init(refuse: @escaping @Sendable () -> Never) { self.refuse = refuse }
    let displayName = "Owned UI fixture"
    let kind = "hvf-engine"
    let supportsGuestCommands = false, supportsPackageInstall = false, supportsClipboard = false
    let supportsSSH = false, supportsResourceChanges = false
    func isRunning() -> Bool { refuse() }
    func currentIP() -> String? { refuse() }
    func start() -> Bool { refuse() }
    func stop() { refuse() }
    func resources() -> (memMiB: Int, cpu: Int) { refuse() }
    func setResources(memMiB: Int, cpu: Int) -> Bool { refuse() }
    func runInGuest(_ command: String) -> (output: String, code: Int32) { refuse() }
}

extension HvfAppUICapture {
    func makeModel(_ config: VMConfig) -> ControlModel {
        guard work.admit(config) else { tripwire("model_creations") }
        return ControlModel(config: config, backend: HvfAppUIInertBackend(refuse: { [self] in backendTripwire() }),
                            startsAutomatically: false)
    }

    nonisolated func backendTripwire() -> Never {
        if Thread.isMainThread { MainActor.assumeIsolated { tripwire("backend_calls") } }
        DispatchQueue.main.sync { MainActor.assumeIsolated { tripwire("backend_calls") } }
    }
}

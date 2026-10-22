#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation

/// Domain-work accounting for the owned UI fixture. The invariant is no backend,
/// process, Keychain or file effect: every tripwire below is fatal and must stay
/// zero. The only admitted domain object is one control model over an inert
/// backend per seeded fixture VM, as each LibraryWorkspaceList row requests it;
/// LibraryModel caches it, none may exist before seeding and no other is admitted.
@MainActor
final class AppUIHostDomainWork {
    private(set) var tripwires = Dictionary(uniqueKeysWithValues: ["runtime_creations", "install_creations",
        "file_jobs", "backend_calls"].map { ($0, 0) })
    private(set) var beforeSeeding = 0
    private(set) var afterSeeding = 0
    private var seeded: Set<String>?
    private var created: Set<String> = []

    /// Counts a fatal tripwire; a refused model request is already counted by `admit`.
    func trip(_ name: String) {
        precondition(tripwires[name] != nil || name == "model_creations", "Unknown domain-work tripwire")
        tripwires[name]? += 1
    }

    /// Admits the synthetic fixture VMs; each may then receive exactly one model.
    func seed(_ configs: [VMConfig]) -> [VMConfig] {
        precondition(seeded == nil, "Overview fixture VMs are seeded once")
        seeded = Set(configs.map(\.slug))
        return configs
    }

    /// Counts every model request that reaches the factory. Only the first
    /// request for each seeded fixture VM is admitted.
    func admit(_ config: VMConfig) -> Bool {
        guard let seeded else { beforeSeeding += 1; return false }
        afterSeeding += 1
        return seeded.contains(config.slug) && created.insert(config.slug).inserted
    }

    var modelCreations: [String: Int] {
        ["before_seeding": beforeSeeding, "after_seeding": afterSeeding, "seeded_fixture_vms": seeded?.count ?? 0]
    }

    var complete: Bool {
        guard let seeded, !seeded.isEmpty else { return false }
        return tripwires.values.allSatisfy { $0 == 0 } && beforeSeeding == 0
            && afterSeeding == seeded.count && created == seeded
    }
}

extension AppUIHostCapture {
    /// The fixture library's model factory. Models never start automatically, so
    /// no refresh or polling timer exists; any backend call is a fatal tripwire.
    func makeModel(_ config: VMConfig) -> ControlModel {
        guard work.admit(config) else { tripwire("model_creations") }
        return ControlModel(config: config, backend: AppUIHostInertBackend(refuse: { [self] in backendTripwire() }),
                            startsAutomatically: false)
    }
}
#endif

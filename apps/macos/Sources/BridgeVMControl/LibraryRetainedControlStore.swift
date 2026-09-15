import Foundation
import Combine

@MainActor
final class LibraryRetainedControlStore {
    private(set) var records: [LibraryRetainedControlRecord] = []
    private var subscriptions: [LibraryRetainedControlIdentity: AnyCancellable] = [:]
    var onChange: (@MainActor () -> Void)?

    func capture(_ descriptors: [LibraryRetainedControlDescriptor]) {
        let additions = descriptors.filter { $0.isActive && subscriptions[$0.identity] == nil }
            .sorted { $0.sortKey.lexicographicallyPrecedes($1.sortKey) }
        guard !additions.isEmpty else { return }
        for descriptor in additions {
            let identity = descriptor.identity
            guard subscriptions[identity] == nil else { continue }
            records.append(LibraryRetainedControlRecord(
                id: "__bridgevm_retained_\(UUID().uuidString)__", descriptor: descriptor))
            subscriptions[identity] = descriptor.observeState { [weak self] in self?.onChange?() }
        }
        onChange?()
    }

    @discardableResult
    func dismiss(_ token: String) -> Bool {
        guard let index = records.firstIndex(where: { $0.id == token }),
              !records[index].descriptor.isActive else { return false }
        let identity = records[index].descriptor.identity
        subscriptions.removeValue(forKey: identity)?.cancel()
        records.remove(at: index)
        onChange?()
        return true
    }
}

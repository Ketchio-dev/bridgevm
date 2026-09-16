#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation

@MainActor
struct AppUIHostOwnedAXEntries {
    static let limit = 128
    private(set) var inspected = 0
    private(set) var omitted = 0

    mutating func project<Entry>(_ entries: [Entry], describe: (Entry) -> [String: Any]) -> [String: Any] {
        let count = min(entries.count, Self.limit - inspected)
        let records = entries.prefix(count).map(describe)
        inspected += count
        omitted += entries.count - count
        return ["raw_count": entries.count, "inspected_count": count,
                "omitted_count": entries.count - count, "entries": records]
    }

    var summary: [String: Any] {
        ["limit": Self.limit, "inspected": inspected, "omitted": omitted,
         "exhausted": inspected == Self.limit, "truncated": omitted > 0]
    }
}
#endif

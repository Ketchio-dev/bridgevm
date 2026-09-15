#if DEBUG && BRIDGEVM_APP_UI_HOST
import AppKit
import CryptoKit

@MainActor
final class AppUIHostCapture {
    static let screenshotNames = ["welcome-light-default", "welcome-light-minimum", "welcome-dark-default",
        "welcome-dark-minimum", "create-sheet", "import-form", "overview", "overview-search"]
    let output: URL
    private weak var owner: NSWindow?
    private var screenshots: [[String: Any]] = []
    private var actions = Dictionary(uniqueKeysWithValues: ["welcome_visible", "create_opened",
        "create_cancelled", "import_opened", "overview_opened", "search_filtered", "search_cleared"].map { ($0, false) })
    private var tripwires = Dictionary(uniqueKeysWithValues: ["model_creations", "runtime_creations",
        "install_creations", "file_jobs"].map { ($0, 0) })
    private var failure: String?

    init(output: URL) throws {
        self.output = output
        guard !FileManager.default.fileExists(atPath: output.appendingPathComponent("ui-observations.json").path)
        else { throw AppUIHostError.refused("Output already contains a report") }
        try save()
    }
    func bindOwner(_ window: NSWindow) throws {
        guard owner == nil || owner === window else { throw AppUIHostError.refused("Capture owner changed") }
        owner = window
    }
    func action(_ name: String) throws {
        guard actions[name] == false else { throw AppUIHostError.refused("Unknown or duplicate action") }
        actions[name] = true
        try save()
    }
    func failed(_ error: Error) {
        failure = String(describing: error)
        try? save()
    }
    func tripwire(_ name: String) -> Never {
        tripwires[name, default: 0] += 1
        failure = "Forbidden domain work: \(name)"
        try? save()
        try? writeCompletion(cleanupVerified: false)
        fatalError("Native app UI host refused domain work: \(name)")
    }
    func capture(_ view: NSView, name: String) throws {
        guard let owner, let window = view.window, view === window.contentView,
              window === owner || owner.sheets.contains(where: { $0 === window }),
              Self.screenshotNames.contains(name), !screenshots.contains(where: { $0["name"] as? String == name })
        else { throw AppUIHostError.refused("Capture is not a unique owned content/sheet view") }
        view.layoutSubtreeIfNeeded()
        view.displayIfNeeded()
        guard let bitmap = view.bitmapImageRepForCachingDisplay(in: view.bounds)
        else { throw AppUIHostError.refused("Owned content has no bitmap representation") }
        view.effectiveAppearance.performAsCurrentDrawingAppearance { view.cacheDisplay(in: view.bounds, to: bitmap) }
        guard bitmap.pixelsWide > 0, bitmap.pixelsHigh > 0,
              let data = bitmap.representation(using: .png, properties: [:]), !data.isEmpty
        else { throw AppUIHostError.refused("Owned content produced an empty image") }
        let file = name + ".png"
        try data.write(to: output.appendingPathComponent(file), options: .withoutOverwriting)
        screenshots.append(["name": name, "file": file, "sha256": Self.digest(data),
            "width": bitmap.pixelsWide, "height": bitmap.pixelsHigh])
        try save()
    }
    func save() throws {
        try write(["schema_version": 1, "kind": "native-app-ui-diagnostic", "fixture_data": true,
            "guest_behavior_proof": false, "screenshots": screenshots, "actions": actions,
            "tripwires": tripwires, "failure": failure.map { $0 as Any } ?? NSNull()], name: "ui-observations.json")
    }
    func writeCompletion(cleanupVerified: Bool) throws {
        let complete = actions.values.allSatisfy { $0 } && screenshots.count == Self.screenshotNames.count
            && tripwires.values.allSatisfy { $0 == 0 }
        if failure == nil && (!complete || !cleanupVerified) {
            failure = "Host ended without all required observations and verified fixture cleanup"
        }
        try save()
        let report = output.appendingPathComponent("ui-observations.json")
        let record: [String: Any] = ["schema_version": 1, "kind": "native-app-ui-host-completion",
            "pid": Int(getpid()), "success": complete && cleanupVerified && failure == nil,
            "cleanup_verified": cleanupVerified, "report_sha256": try Self.digest(report),
            "failure": failure.map { $0 as Any } ?? NSNull()]
        try write(record, name: "host-completion.json", final: true)
    }
    func write(_ record: [String: Any], name: String, final: Bool = false) throws {
        let data = try JSONSerialization.data(withJSONObject: record, options: [.prettyPrinted, .sortedKeys])
        try data.write(to: output.appendingPathComponent(name), options: final ? .withoutOverwriting : .atomic)
    }
    static func digest(_ data: Data) -> String { SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined() }
    static func digest(_ url: URL) throws -> String {
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        var hasher = SHA256()
        while let data = try handle.read(upToCount: 1_048_576), !data.isEmpty { hasher.update(data: data) }
        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }
}

enum AppUIHostError: Error { case refused(String) }
#endif

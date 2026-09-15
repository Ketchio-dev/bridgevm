import AppKit
import CryptoKit

@MainActor
final class HvfAppUICapture {
    let output: URL
    private var screenshots: [[String: Any]] = []
    private var actions = Dictionary(uniqueKeysWithValues: ["welcome_visible", "create_opened",
        "create_cancelled", "import_opened", "overview_opened", "search_filtered", "search_cleared"].map { ($0, false) })
    private var tripwires = Dictionary(uniqueKeysWithValues: ["model_creations", "runtime_creations",
        "install_creations", "file_jobs"].map { ($0, 0) })
    private var failure: String?

    init(output: URL) throws {
        self.output = output
        try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
        guard !FileManager.default.fileExists(atPath: output.appendingPathComponent("ui-observations.json").path)
        else { throw HvfAppUIError.refused("Output already contains an observation; use a fresh owned directory") }
        try save()
    }

    func action(_ name: String) throws {
        guard actions[name] != nil else { throw HvfAppUIError.refused("Unknown action: \(name)") }
        actions[name] = true
        try save()
    }

    func failed(_ error: Error) {
        failure = String(describing: error)
        try? save()
    }

    // Persist the counter before terminating; a regression cannot progress to a
    // VM constructor, install worker or scheduled file operation in this harness.
    func tripwire(_ name: String) -> Never {
        tripwires[name, default: 0] += 1
        failure = "Forbidden domain work: \(name)"
        try? save()
        fatalError("Native UI diagnostic refused domain work: \(name)")
    }

    func capture(_ view: NSView, name: String) throws {
        guard !name.contains("/"), !screenshots.contains(where: { $0["name"] as? String == name })
        else { throw HvfAppUIError.refused("Invalid or duplicate capture name") }
        view.layoutSubtreeIfNeeded()
        view.displayIfNeeded()
        guard let bitmap = view.bitmapImageRepForCachingDisplay(in: view.bounds)
        else { throw HvfAppUIError.refused("Owned content view has no bitmap representation") }
        view.effectiveAppearance.performAsCurrentDrawingAppearance { view.cacheDisplay(in: view.bounds, to: bitmap) }
        guard bitmap.pixelsWide > 0, bitmap.pixelsHigh > 0,
              let data = bitmap.representation(using: .png, properties: [:]), !data.isEmpty
        else { throw HvfAppUIError.refused("Owned content view produced an empty image") }
        let file = name + ".png"
        try data.write(to: output.appendingPathComponent(file), options: .withoutOverwriting)
        screenshots.append(["name": name, "file": file,
            "sha256": SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined(),
            "width": bitmap.pixelsWide, "height": bitmap.pixelsHigh])
        try save()
    }

    func save() throws {
        let record: [String: Any] = ["schema_version": 1, "kind": "native-app-ui-diagnostic",
            "fixture_data": true, "guest_behavior_proof": false, "screenshots": screenshots,
            "actions": actions, "tripwires": tripwires, "failure": failure.map { $0 as Any } ?? NSNull()]
        let data = try JSONSerialization.data(withJSONObject: record, options: [.prettyPrinted, .sortedKeys])
        try data.write(to: output.appendingPathComponent("ui-observations.json"), options: .atomic)
    }
}

enum HvfAppUIError: Error {
    case refused(String)
}

import Darwin
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfWindowsInstallRejectedRequestTests: XCTestCase {
    func testRejectedMetadataShowsPreparationFailureAndPreservesOwnedFiles() async throws {
        for kind in ["oversized", "malformed", "symlink", "fifo", "missing"] {
            let f = try HvfRuntimeWorkAdmissionFixture()
            defer { f.clean() }
            let config = f.config(pending: true)
            f.save(config)
            f.saveRequest(config)
            let requestURL = URL(fileURLWithPath: config.bundlePath)
                .appendingPathComponent(HvfWindowsInstallRequest.fileName)
            try reject(kind, at: requestURL)
            XCTAssertNil(HvfWindowsInstallRequest.load(bundlePath: config.bundlePath), kind)
            var scheduled = 0, created = 0
            let library = LibraryModel(rootURL: f.root, migrateLegacy: false,
                installSessionFactory: { plan in
                    created += 1
                    return HvfWindowsInstallSession(plan: plan, schedule: { _ in scheduled += 1 })
                }, runtimeSessionFactory: { runtimeConfig in
                    f.runtimeCreations += 1
                    return f.makeRuntime(runtimeConfig)
                }, installPreparation: .init(repoRoot: f.installRepo),
                actionScheduler: f.queue.enqueue, modelFactory: { config in
                    f.modelCreations += 1
                    return ControlModel(config: config, backend: f.backend(config), startsAutomatically: false)
                })
            library.selectedID = config.slug
            let before = try f.snapshot()
            let body = LibraryDetailView(library: library).body
            let preparation = library.windowsInstallPreparation(for: config)
            await preparation.preparationTask?.value
            let host = try XCTUnwrap(values(HvfWindowsInstallPreparationView.self, in: body).first)
            XCTAssertTrue(host.preparation === preparation)
            XCTAssertTrue(values(HvfEngineView.self, in: body).isEmpty)
            if case let .failed(message) = host.preparation.state {
                XCTAssertTrue(message.contains("설치 정보"))
            } else {
                XCTFail("Rejected metadata must show preparation failure: \(kind)")
            }
            XCTAssertTrue(values(HvfWindowsInstallView.self, in: host.body).isEmpty)
            XCTAssertEqual(created, 0)
            XCTAssertEqual(scheduled, 0)
            XCTAssertEqual(f.runtimeCreations, 0)
            XCTAssertEqual(f.backend(config).calls, .init())
            XCTAssertEqual(library.vms.first { $0.slug == config.slug }?.installPending, true)
            XCTAssertEqual(VMLibrary.scan(rootURL: f.root).configs.first { $0.slug == config.slug }, config)
            XCTAssertEqual(try f.snapshot(), before)
            if kind == "fifo" { XCTAssertEqual(lstatType(requestURL), S_IFIFO) }
            if kind == "symlink" { XCTAssertEqual(lstatType(requestURL), S_IFLNK) }
        }
    }

    private func reject(_ kind: String, at url: URL) throws {
        switch kind {
        case "oversized":
            var bytes = try Data(contentsOf: url)
            bytes.append(Data(repeating: 0x20, count: VMLibrary.maximumConfigBytes + 1 - bytes.count))
            try bytes.write(to: url)
        case "malformed": try Data("not JSON".utf8).write(to: url)
        case "missing": try FileManager.default.removeItem(at: url)
        case "symlink":
            let target = url.deletingLastPathComponent().appendingPathComponent("other-request.json")
            try FileManager.default.moveItem(at: url, to: target)
            try FileManager.default.createSymbolicLink(at: url, withDestinationURL: target)
        default:
            try FileManager.default.removeItem(at: url)
            XCTAssertEqual(mkfifo(url.path, 0o600), 0)
        }
    }

    private func lstatType(_ url: URL) -> mode_t {
        var value = stat()
        XCTAssertEqual(lstat(url.path, &value), 0)
        return value.st_mode & S_IFMT
    }

    private func values<T>(_ type: T.Type, in value: Any) -> [T] {
        if let result = value as? T { return [result] }
        let mirror = Mirror(reflecting: value)
        guard mirror.displayStyle != .class else { return [] }
        return mirror.children.flatMap { values(type, in: $0.value) }
    }
}

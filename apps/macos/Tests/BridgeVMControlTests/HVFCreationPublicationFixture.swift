import XCTest
@testable import BridgeVMControl

final class HVFCreationPublicationFixture {
    let root: URL
    let separateStorage: Bool
    var library: URL { root.appendingPathComponent("library", isDirectory: true) }
    var storage: URL? { separateStorage ? root.appendingPathComponent("storage", isDirectory: true) : nil }
    var capturedConfig: VMConfig?
    var registrationBytes: Data?
    var requestBytes: Data?
    var attemptedSync = false
    static let privateFailureDetail = "private injected directory-sync failure"

    init(separateStorage: Bool) throws {
        self.separateStorage = separateStorage
        root = FileManager.default.temporaryDirectory.appendingPathComponent("hvf-publication-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
    }

    func remove() { try? FileManager.default.removeItem(at: root) }

    func bundle(_ config: VMConfig) -> URL {
        (storage ?? library).appendingPathComponent(config.slug + "/bundle.vmbridge", isDirectory: true)
    }

    func registration(_ config: VMConfig) -> URL { library.appendingPathComponent(config.slug + "/vm.json") }

    func publishWithSyncFailure(_ config: VMConfig, _ libraryRoot: URL) -> VMRegistrationCommitOutcome {
        capturedConfig = config
        XCTAssertEqual(libraryRoot, library)
        requestBytes = FileManager.default.contents(atPath: config.bundlePath + "/" + HvfWindowsInstallRequest.fileName)
        if config.installPending == true { XCTAssertNotNil(requestBytes) }
        return VMLibrary.saveOutcome(config, rootURL: libraryRoot) { data, url in
            registrationBytes = data
            let outcome = VMRegistrationWriter.commit(data, to: url, syncParent: { parent in
                XCTAssertEqual(url, registration(config))
                XCTAssertEqual(parent, url.deletingLastPathComponent())
                XCTAssertEqual(try Data(contentsOf: url), data)
                XCTAssertEqual(try JSONDecoder().decode(VMConfig.self, from: Data(contentsOf: url)), config)
                XCTAssertTrue(FileManager.default.fileExists(atPath: config.bundlePath))
                attemptedSync = true
                throw NSError(domain: "HVFCreationPublicationTests", code: 1,
                    userInfo: [NSLocalizedDescriptionKey: Self.privateFailureDetail])
            })
            guard case .publishedButUnsynced = outcome else {
                XCTFail("expected real publication followed by injected directory-sync failure")
                return outcome
            }
            return outcome
        }
    }

    func publishWithRenameRefusal(_ config: VMConfig, _ libraryRoot: URL) -> VMRegistrationCommitOutcome {
        capturedConfig = config
        XCTAssertEqual(libraryRoot, library)
        XCTAssertTrue(FileManager.default.fileExists(atPath: config.bundlePath))
        return VMLibrary.saveOutcome(config, rootURL: libraryRoot) { data, url in
            do {
                try FileManager.default.createDirectory(at: url, withIntermediateDirectories: false)
            } catch { XCTFail("could not prepare rename refusal: \(error)"); return .notPublished(error) }
            let outcome = VMRegistrationWriter.commit(data, to: url, syncParent: { _ in
                XCTFail("rename refusal must not reach directory sync")
            })
            guard case .notPublished = outcome else {
                XCTFail("expected real rename refusal")
                return outcome
            }
            return outcome
        }
    }

    func assertRetained(_ config: VMConfig, warning: String) throws {
        XCTAssertTrue(attemptedSync)
        XCTAssertEqual(capturedConfig, config)
        XCTAssertEqual(config.bundlePath, bundle(config).path)
        XCTAssertEqual(try Data(contentsOf: registration(config)), try XCTUnwrap(registrationBytes))
        XCTAssertEqual(try JSONDecoder().decode(VMConfig.self, from: Data(contentsOf: registration(config))), config)
        XCTAssertTrue(FileManager.default.fileExists(atPath: bundle(config).path))
        XCTAssertEqual(warning,
            "VM 파일과 등록 정보는 보존했지만 저장 완료를 확인하지 못했습니다. 다시 만들지 말고 라이브러리를 다시 확인하세요.")
        XCTAssertFalse(warning.contains(Self.privateFailureDetail))
        XCTAssertFalse(warning.contains(root.path))
    }

    func assertRolledBack() throws {
        let config = try XCTUnwrap(capturedConfig)
        XCTAssertFalse(FileManager.default.fileExists(atPath: bundle(config).path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: bundle(config).deletingLastPathComponent().path))
        XCTAssertNil(FileManager.default.contents(atPath: registration(config).path))
    }
}

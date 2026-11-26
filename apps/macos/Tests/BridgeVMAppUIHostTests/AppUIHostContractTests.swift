#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class AppUIHostContractTests: XCTestCase {
    func testAcceptsOnlyExactRequestWithoutWritingToOutput() throws {
        let fixture = try fixture()
        XCTAssertEqual(try AppUIHost.outputDirectory(arguments: arguments(fixture.output)), fixture.output)
        try assertRequestTransports(output: fixture.output)
        for invalid in [[], ["--app-ui-host"], ["--output", fixture.output.path, "--app-ui-host"],
                        arguments(fixture.output) + ["--vtpm-lifecycle"],
                        ["--app-ui-host", "--output", "app-ui-private/host-observations"],
                        ["--app-ui-host", "--output", fixture.output.path + "/../host-observations"]] {
            XCTAssertThrowsError(try AppUIHost.outputDirectory(arguments: invalid))
            XCTAssertThrowsError(try AppUIHostRequest.resolve(arguments: invalid, environment: [:]))
        }
        try assertMalformedRequestEnvelopes(output: fixture.output)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: fixture.output.path), [])
    }
    func testRejectsNonemptyOrNonprivateOutputWithoutRemovingEvidence() throws {
        let fixture = try fixture()
        let evidence = fixture.output.appendingPathComponent("retained.txt")
        try Data("retained failure".utf8).write(to: evidence)
        XCTAssertThrowsError(try AppUIHost.outputDirectory(arguments: arguments(fixture.output)))
        assertEnvironmentRefuses(output: fixture.output)
        XCTAssertEqual(try String(contentsOf: evidence, encoding: .utf8), "retained failure")
        try FileManager.default.removeItem(at: evidence)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: fixture.output.path)
        XCTAssertThrowsError(try AppUIHost.outputDirectory(arguments: arguments(fixture.output)))
        assertEnvironmentRefuses(output: fixture.output)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: fixture.output.path)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: fixture.output.deletingLastPathComponent().path)
        XCTAssertThrowsError(try AppUIHost.outputDirectory(arguments: arguments(fixture.output)))
        assertEnvironmentRefuses(output: fixture.output)
    }
    func testRejectsSymlinkedParentAndOutput() throws {
        let fixture = try fixture()
        let aliasRoot = fixture.root.appendingPathComponent("alias", isDirectory: true)
        try FileManager.default.createDirectory(at: aliasRoot, withIntermediateDirectories: false)
        let aliasParent = aliasRoot.appendingPathComponent("app-ui-private", isDirectory: true)
        try FileManager.default.createSymbolicLink(at: aliasParent, withDestinationURL: fixture.output.deletingLastPathComponent())
        XCTAssertThrowsError(try AppUIHost.outputDirectory(arguments: arguments(aliasParent.appendingPathComponent("host-observations"))))
        assertEnvironmentRefuses(output: aliasParent.appendingPathComponent("host-observations"))
        let actual = fixture.output.deletingLastPathComponent().appendingPathComponent("real-output", isDirectory: true)
        try FileManager.default.moveItem(at: fixture.output, to: actual)
        try FileManager.default.createSymbolicLink(at: fixture.output, withDestinationURL: actual)
        XCTAssertThrowsError(try AppUIHost.outputDirectory(arguments: arguments(fixture.output)))
        assertEnvironmentRefuses(output: fixture.output)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: actual.path), [])
    }
    func testCancellationMarkerRefusesWithoutChangingOutputAdmission() throws {
        let fixture = try fixture()
        let marker = fixture.output.deletingLastPathComponent().appendingPathComponent("cancel.requested")
        XCTAssertNoThrow(try AppUIHost.checkCancellation(output: fixture.output))
        try Data().write(to: marker)
        XCTAssertEqual(try AppUIHost.outputDirectory(arguments: arguments(fixture.output)), fixture.output)
        XCTAssertThrowsError(try AppUIHost.checkCancellation(output: fixture.output))
        try FileManager.default.removeItem(at: marker)
        try FileManager.default.createSymbolicLink(at: marker,
            withDestinationURL: fixture.root.appendingPathComponent("missing-cancel-target"))
        XCTAssertThrowsError(try AppUIHost.checkCancellation(output: fixture.output))
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: fixture.output.path), [])
    }
    func testFixtureConstructionHasNoDomainWorkOrLibraryWrites() throws {
        let fixture = try fixture()
        let capture = try AppUIHostCapture(output: fixture.output)
        let libraryRoot = fixture.output.appendingPathComponent("fixture-library", isDirectory: true)
        try FileManager.default.createDirectory(at: libraryRoot, withIntermediateDirectories: false)
        let library = AppUIHost.makeLibrary(root: libraryRoot, capture: capture)
        XCTAssertTrue(library.vms.isEmpty)
        XCTAssertNil(library.selectedID)
        XCTAssertFalse(library.preparingNewVM)
        XCTAssertEqual(library.rootURL, libraryRoot)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: libraryRoot.path), [])
        let report = try object(fixture.output, "ui-observations.json")
        XCTAssertEqual(report["tripwires"] as? [String: Int], ["backend_calls": 0, "runtime_creations": 0,
            "install_creations": 0, "file_jobs": 0])
    }
    func testIncompleteObservationCannotBecomeSuccessfulCompletion() async throws {
        try await assertExpandedHostObservationContracts()
    }
    func testUnpackagedTestProcessRefusesBeforeAppOrFixtureCreation() throws {
        let fixture = try fixture()
        XCTAssertNotEqual(Bundle.main.bundleIdentifier, "dev.bridgevm.app-ui-host")
        XCTAssertNil(AppUIHost.prepared)
        XCTAssertThrowsError(try AppUIHost.prepare(arguments: arguments(fixture.output)))
        XCTAssertNil(AppUIHost.prepared)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.output.appendingPathComponent("fixture-library").path))
        let report = try object(fixture.output, "ui-observations.json")
        XCTAssertNotNil(report["failure"] as? String)
        XCTAssertEqual(try object(fixture.output, "host-completion.json")["success"] as? Bool, false)
    }
}
#endif

#if DEBUG && BRIDGEVM_APP_UI_HOST

extension AppUIHostContractTests {
    func assertExpandedHostObservationContracts() async throws {
        try await assertAllIncompleteHostObservationContracts()
        try assertOwnedViewObservationContracts()
        try assertWindowServerCapturePolicy()
        try await assertAwaitedCaptureRefusesIncompleteRows()
    }

    func assertAwaitedCaptureRefusesIncompleteRows() async throws {
        for cancellation in [false, true] {
            let matrix = try matrixFixture()
            var resumed = false
            matrix.capture = { _ in
                XCTAssertEqual(matrix.persistedCounts, [0])
                XCTAssertEqual(matrix.observations, 0)
                await Task.yield()
                resumed = true
                XCTAssertEqual(try self.matrixRows(matrix).count, 0)
                if cancellation { withUnsafeCurrentTask { $0?.cancel() } }
                else { throw AppUIHostMatrixFixture.Failure.ownership }
            }
            let error = await Task { @MainActor in await self.matrixFailure(matrix) }.value
            XCTAssertTrue(resumed)
            if cancellation { XCTAssertTrue(error is CancellationError) }
            else { XCTAssertEqual(error as? AppUIHostMatrixFixture.Failure, .ownership) }
            XCTAssertEqual(matrix.captureAttempts, ["welcome-light-default"])
            XCTAssertEqual(matrix.presentations, ["welcome-light-default"])
            XCTAssertEqual(matrix.persistedCounts, [0])
            XCTAssertEqual(matrix.observations, 0)
            XCTAssertEqual(try matrixRows(matrix).count, 0)
            XCTAssertEqual(matrix.latchCount, 0)
        }
    }
}
#endif

#if DEBUG && BRIDGEVM_APP_UI_HOST

extension AppUIHostContractTests {
    func assertOwnedViewMetadataAndWrites() throws {
        let unknown = AppUIHostAXButtonFixture(), create = AppUIHostAXButtonFixture(), importButton = AppUIHostAXButtonFixture()
        create.identifier = "bridgevm.first-run.create"
        create.role = "AXButton"
        importButton.identifier = "bridgevm.first-run.import"
        let entries = [unknown, create, importButton].map { AppUIHostOwnedViewFields.entry($0) }
        XCTAssertEqual(entries.compactMap { $0["allowed_identifier_match"] as? String },
                       ["bridgevm.first-run.create", "bridgevm.first-run.import"])
        XCTAssertEqual(entries.compactMap { $0["allowed_role"] as? String }, ["AXButton"])
        XCTAssertTrue(entries.allSatisfy { (($0["type"] as? String)?.count ?? 129) <= 128 })
        XCTAssertEqual([unknown, create, importButton].map { $0.childrenReads }, [0, 0, 0])
        XCTAssertEqual([unknown, create, importButton].map { $0.forbiddenReads }, [0, 0, 0])
        let encoded = try JSONSerialization.data(withJSONObject: entries)
        let text = String(decoding: encoded, as: UTF8.self)
        for secret in [unknown.identifier, unknown.role, "private-label", "private-value", "private-title"] {
            XCTAssertFalse(text.contains(secret))
        }
        let geometry = AppUIHostOwnedViewFields.rect(CGRect(x: 1, y: 2, width: CGFloat.infinity, height: CGFloat.nan))
        XCTAssertEqual(geometry["x"] as? Double, 1)
        XCTAssertEqual(geometry["y"] as? Double, 2)
        XCTAssertTrue(geometry["width"] is NSNull)
        XCTAssertTrue(geometry["height"] is NSNull)
        XCTAssertNoThrow(try JSONSerialization.data(withJSONObject: geometry))

        let fixture = try fixture()
        let capture = try AppUIHostCapture(output: fixture.output)
        for stage in [AppUIHostOwnedViewProjection.Stage.welcomeTimeout, .darkDefault] {
            try AppUIHostOwnedViewObservation.write(["stage": stage.rawValue, "fixture": true], stage: stage, capture: capture)
            let path = fixture.output.appendingPathComponent(stage.filename)
            let original = try Data(contentsOf: path)
            XCTAssertThrowsError(try AppUIHostOwnedViewObservation.write(["replacement": true], stage: stage, capture: capture))
            XCTAssertEqual(try Data(contentsOf: path), original)
        }
        XCTAssertEqual(AppUIHostOwnedViewProjection.Stage.welcomeTimeout.filename, "host-owned-views-welcome-timeout.json")
        XCTAssertEqual(AppUIHostOwnedViewProjection.Stage.darkDefault.filename, "host-owned-views-dark-default.json")
        assertWelcomeTimeout {
            try AppUIHostAccessibility.timeout("welcome controls", onTimeout: {
                try AppUIHostOwnedViewObservation.write([:], stage: .welcomeTimeout, capture: capture)
            })
        }
        XCTAssertTrue(try object(fixture.output, "ui-observations.json")["failure"] is NSNull)
    }
}
#endif

#if DEBUG && BRIDGEVM_APP_UI_HOST

extension AppUIHostContractTests {
    func fixture() throws -> (root: URL, output: URL) {
        let root = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("bridgevm-ui-host-contract-" + UUID().uuidString, isDirectory: true)
        let parent = root.appendingPathComponent("app-ui-private", isDirectory: true)
        let output = parent.appendingPathComponent("host-observations", isDirectory: true)
        for directory in [root, parent, output] {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false,
                                                   attributes: [.posixPermissions: 0o700])
        }
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        return (root, output)
    }
    func arguments(_ output: URL) -> [String] { ["--app-ui-host", "--output", output.path] }
    func object(_ output: URL, _ name: String) throws -> [String: Any] {
        try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: output.appendingPathComponent(name))) as? [String: Any])
    }
    func environment(_ output: URL) -> [String: String] {
        ["BRIDGEVM_APP_UI_HOST_MODE": "1", "BRIDGEVM_APP_UI_HOST_OUTPUT": output.path]
    }
    func assertRequestTransports(output: URL) throws {
        let argumentRequest = try AppUIHostRequest.resolve(arguments: arguments(output), environment: [:])
        XCTAssertEqual(argumentRequest.output, output)
        XCTAssertEqual(argumentRequest.transport, .arguments)
        XCTAssertEqual(argumentRequest.argumentCount, 3)
        let environmentRequest = try AppUIHostRequest.resolve(arguments: [], environment: environment(output))
        XCTAssertEqual(environmentRequest.output, output)
        XCTAssertEqual(environmentRequest.transport, .environment)
        XCTAssertEqual(environmentRequest.argumentCount, 0)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: output.path), [])
    }
    func assertMalformedRequestEnvelopes(output: URL) throws {
        let mode = AppUIHostRequest.modeKey
        let path = AppUIHostRequest.outputKey
        let malformed: [([String], [String: String])] = [
            ([], [mode: "1"]),
            ([], [path: output.path]),
            ([], [mode: "0", path: output.path]),
            ([], [mode: "true", path: output.path]),
            ([], [mode: "", path: output.path]),
            ([], [mode: "1", path: ""]),
            ([], [mode: "1", path: "app-ui-private/host-observations"]),
            ([], [mode: "1", path: output.path + "/../host-observations"]),
            ([], [mode: "1", path: output.path + "\0"]),
            ([], [mode: "1", path: "/" + String(repeating: "x", count: 4096)]),
            (arguments(output), environment(output)),
            (arguments(output), [mode: "1"]),
            (arguments(output), [path: output.path]),
            (["--vtpm-lifecycle"], environment(output))
        ]
        for (arguments, environment) in malformed {
            XCTAssertThrowsError(try AppUIHostRequest.resolve(arguments: arguments, environment: environment))
            XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: output.path), [])
        }
    }
    func assertEnvironmentRefuses(output: URL, file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertThrowsError(try AppUIHostRequest.resolve(arguments: [], environment: environment(output)),
                             file: file, line: line)
    }
}
#endif

#if DEBUG && BRIDGEVM_APP_UI_HOST

extension AppUIHostContractTests {
    func assertWindowServerCapturePolicy() throws {
        typealias Policy = AppUIHostWindowServerPolicy
        let own = Policy.Candidate(windowID: 7, processID: 42, onScreen: true)
        let other = Policy.Candidate(windowID: 8, processID: 900, onScreen: true)
        XCTAssertEqual(try Policy.select([other, own], windowNumber: 7, processID: 42), 1)
        let invalid: [[Policy.Candidate]] = [[], [other], [own, own],
            [.init(windowID: 7, processID: nil, onScreen: true)],
            [.init(windowID: 7, processID: 900, onScreen: true)],
            [.init(windowID: 7, processID: 42, onScreen: false)],
            [own, .init(windowID: 7, processID: 900, onScreen: true)],
            Array(repeating: other, count: 256) + [own]]
        for candidates in invalid {
            XCTAssertThrowsError(try Policy.select(candidates, windowNumber: 7, processID: 42))
        }
        for number in [0, -1, Int(UInt32.max) + 1] {
            XCTAssertThrowsError(try Policy.select([own], windowNumber: number, processID: 42))
        }
        XCTAssertThrowsError(try Policy.select([own], windowNumber: 7, processID: 1))
        XCTAssertEqual(try Policy.dimensions(x: -20, y: 30, width: 1320, height: 860, scale: 2),
                       .init(width: 2640, height: 1720))
        XCTAssertEqual(try Policy.dimensions(x: 0, y: 0, width: 1.2, height: 1.1, scale: 1),
                       .init(width: 2, height: 2))
        XCTAssertEqual(try Policy.dimensions(x: 0, y: 0, width: 8192, height: 1024, scale: 1),
                       .init(width: 8192, height: 1024))
        for values in [[Double.nan, 0, 1, 1, 1], [0, Double.infinity, 1, 1, 1],
                       [0, 0, 0, 1, 1], [0, 0, -1, 1, 1], [0, 0, 1, 1, 0],
                       [0, 0, 1, 1, Double.nan], [0, 0, Double.infinity, 1, 1],
                       [0, 0, 1, 1, Double.infinity], [0, 0, 8193, 1, 1],
                       [0, 0, 8192, 1025, 1], [0, 0, Double.greatestFiniteMagnitude, 1, 2]] {
            XCTAssertThrowsError(try Policy.dimensions(x: values[0], y: values[1], width: values[2],
                                                      height: values[3], scale: values[4]))
        }
        try assertWindowServerOutputOwnership()
    }

    private func assertWindowServerOutputOwnership() throws {
        typealias Output = AppUIHostWindowServerPolicy.Output
        let fresh = try fixture()
        let output = try Output(parent: fresh.output)
        XCTAssertEqual(output.directory.lastPathComponent, "window-server")
        XCTAssertEqual((try FileManager.default.attributesOfItem(atPath: output.directory.path)[.posixPermissions] as? NSNumber)?.intValue, 0o700)
        let bytes = Data("owned output guard fixture; not a screenshot".utf8)
        try output.write(bytes, name: "owned-dark-default.png")
        XCTAssertThrowsError(try output.write(Data("replacement".utf8), name: "owned-dark-default.png"))
        XCTAssertEqual(try Data(contentsOf: output.directory.appendingPathComponent("owned-dark-default.png")), bytes)
        XCTAssertThrowsError(try Output(parent: fresh.output))
        XCTAssertThrowsError(try output.write(bytes, name: "../outside.png"))
        XCTAssertThrowsError(try output.write(Data(), name: "owned-dark-default.json"))
        XCTAssertThrowsError(try output.write(Data(repeating: 0, count: 65_537), name: "owned-dark-default.json"))
        let outside = fresh.root.appendingPathComponent("retained.txt")
        try bytes.write(to: outside)
        try FileManager.default.createSymbolicLink(at: output.directory.appendingPathComponent("owned-dark-default.json"),
                                                 withDestinationURL: outside)
        XCTAssertThrowsError(try output.write(Data("replacement".utf8), name: "owned-dark-default.json"))
        XCTAssertEqual(try Data(contentsOf: outside), bytes)
        let replaced = fresh.output.appendingPathComponent("retained-window-server", isDirectory: true)
        try FileManager.default.moveItem(at: output.directory, to: replaced)
        try FileManager.default.createDirectory(at: output.directory, withIntermediateDirectories: false,
                                               attributes: [.posixPermissions: 0o700])
        XCTAssertThrowsError(try output.write(bytes, name: "owned-dark-default.json"))
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: output.directory.path), [])
        let linked = try fixture()
        try FileManager.default.createSymbolicLink(at: linked.output.appendingPathComponent("window-server"),
                                                 withDestinationURL: linked.root.appendingPathComponent("absent"))
        XCTAssertThrowsError(try Output(parent: linked.output))
        let nonprivate = try fixture()
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: nonprivate.output.path)
        XCTAssertThrowsError(try Output(parent: nonprivate.output))
    }
}
#endif

#if DEBUG && BRIDGEVM_APP_UI_HOST

extension AppUIHostContractTests {
    func assertOwnedViewObservationContracts() throws {
        try assertOwnedViewStageAndRefusal()
        try assertOwnedViewEntryBudget()
        try assertOwnedViewMetadataAndWrites()
    }

    func assertOwnedViewStageAndRefusal() throws {
        let root = AppUIHostAXNode(), owned = AppUIHostAXNode()
        let refused = AppUIHostAXNode(), unreachable = AppUIHostAXNode()
        root.children = [owned, refused]
        refused.children = [unreachable]
        refused.rejected = true
        var fieldReads = 0, childrenReads = 0, axReads = 0
        let dark = AppUIHostOwnedViewProjection.collect(root, stage: .darkDefault,
            owned: { !$0.rejected }, fields: { node in fieldReads += 1; node.inspections += 1; return [:] },
            children: { node in childrenReads += 1; return node.children.compactMap { $0 as? AppUIHostAXNode } },
            accessibility: { _, _ in axReads += 1; return [:] })
        XCTAssertEqual(fieldReads, 2)
        XCTAssertEqual(childrenReads, 2)
        XCTAssertEqual(axReads, 0)
        XCTAssertEqual(refused.inspections, 0)
        XCTAssertEqual(unreachable.inspections, 0)
        XCTAssertEqual(dark["node_count"] as? Int, 3)
        XCTAssertEqual(dark["owned_window_refusal_count"] as? Int, 1)
        XCTAssertNil(dark["full_protocol_rejection_count"])
        XCTAssertEqual(dark["ax_queries_enabled"] as? Bool, false)
        XCTAssertEqual((dark["ax_entry_budget"] as? [String: Any])?["inspected"] as? Int, 0)
        let rows = try axRows(dark)
        XCTAssertTrue(rows.allSatisfy { $0["accessibility"] == nil })
        XCTAssertEqual(rows[1]["same_owned_window"] as? Bool, false)
        XCTAssertEqual(rows[1]["children_read"] as? Bool, false)
        XCTAssertEqual(dark["node_limit"] as? Int, 256)
        XCTAssertEqual(dark["depth_limit"] as? Int, 32)
        XCTAssertEqual(dark["child_limit"] as? Int, 128)

        for detachAfterFields in [true, false] {
            let node = AppUIHostAXNode()
            var children = 0, accessibility = 0
            let result = AppUIHostOwnedViewProjection.collect(node, stage: .welcomeTimeout,
                owned: { !$0.rejected }, fields: { value in value.rejected = detachAfterFields; return [:] },
                children: { _ in children += 1; return [AppUIHostAXNode()] },
                accessibility: { value, _ in accessibility += 1; value.rejected = true; return [:] })
            XCTAssertEqual(accessibility, detachAfterFields ? 0 : 1)
            XCTAssertEqual(children, 0)
            XCTAssertEqual(result["owned_window_refusal_count"] as? Int, 1)
            XCTAssertEqual(result["node_count"] as? Int, 1)
        }
    }

    func assertOwnedViewEntryBudget() throws {
        let root = AppUIHostAXNode(), child = AppUIHostAXNode(), last = AppUIHostAXNode()
        root.children = [child]
        child.children = [last]
        var described = 0, calls = 0
        let result = AppUIHostOwnedViewProjection.collect(root, stage: .welcomeTimeout,
            owned: { _ in true }, fields: { _ in [:] },
            children: { $0.children.compactMap { $0 as? AppUIHostAXNode } }, accessibility: { node, budget in
                calls += 1
                let counts = node === root ? (80, 30) : node === child ? (12, 20) : (7, 9)
                let describe: (Int) -> [String: Any] = { _ in described += 1; return [:] }
                let ordinary = budget.project(Array(0..<counts.0), describe: describe)
                let navigation = budget.project(Array(0..<counts.1), describe: describe)
                return ["ordinary_children": ordinary, "navigation_children": navigation]
            })
        XCTAssertEqual(calls, 3)
        XCTAssertEqual(described, 128)
        let summary = try XCTUnwrap(result["ax_entry_budget"] as? [String: Any])
        XCTAssertEqual(summary["limit"] as? Int, 128)
        XCTAssertEqual(summary["inspected"] as? Int, 128)
        XCTAssertEqual(summary["omitted"] as? Int, 30)
        XCTAssertEqual(summary["exhausted"] as? Bool, true)
        XCTAssertEqual(summary["truncated"] as? Bool, true)
        let accessibility = try axRows(result).map { try XCTUnwrap($0["accessibility"] as? [String: Any]) }
        let ordinary = accessibility.compactMap { $0["ordinary_children"] as? [String: Any] }
        let navigation = accessibility.compactMap { $0["navigation_children"] as? [String: Any] }
        XCTAssertEqual(ordinary.compactMap { $0["raw_count"] as? Int }, [80, 12, 7])
        XCTAssertEqual(navigation.compactMap { $0["raw_count"] as? Int }, [30, 20, 9])
        XCTAssertEqual(ordinary.compactMap { $0["inspected_count"] as? Int }, [80, 12, 0])
        XCTAssertEqual(navigation.compactMap { $0["inspected_count"] as? Int }, [30, 6, 0])
        XCTAssertEqual(navigation.compactMap { $0["omitted_count"] as? Int }, [0, 14, 9])
    }
}
#endif

#if DEBUG && BRIDGEVM_APP_UI_HOST

extension AppUIHostContractTests {
    func assertPresentationMatrixCancellation() async throws {
        let before = try matrixFixture()
        let beforeError = await Task { @MainActor in
            withUnsafeCurrentTask { $0?.cancel() }
            return await self.matrixFailure(before)
        }.value
        XCTAssertTrue(beforeError is CancellationError)
        XCTAssertTrue(before.persistAttempts.isEmpty)
        XCTAssertTrue(before.presentations.isEmpty)
        for duringCapture in [false, true] {
            let matrix = try matrixFixture()
            matrix.present = { variant in
                if variant.minimum && !duringCapture { withUnsafeCurrentTask { $0?.cancel() } }
            }
            matrix.capture = { variant in
                if variant.minimum && duringCapture { withUnsafeCurrentTask { $0?.cancel() } }
            }
            let error = await Task { @MainActor in await self.matrixFailure(matrix) }.value
            XCTAssertTrue(error is CancellationError)
            XCTAssertEqual(matrix.presentations.count, 2)
            XCTAssertEqual(matrix.captureAttempts.count, duringCapture ? 2 : 1)
            XCTAssertEqual(matrix.persistedCounts, [0, 1])
            XCTAssertEqual(matrix.observations, 1)
        }
        let marker = try matrixFixture()
        marker.admission = { try AppUIHost.checkCancellation(output: marker.output) }
        marker.present = { variant in
            if variant.minimum {
                try Data().write(to: marker.output.deletingLastPathComponent().appendingPathComponent("cancel.requested"))
                throw AppUIHostPresentationMismatch()
            }
        }
        let markerError = await matrixFailure(marker)
        XCTAssertTrue(markerError is AppUIHostError)
        XCTAssertEqual(marker.presentations.count, 2)
        XCTAssertEqual(marker.captureAttempts, ["welcome-light-default"])
        XCTAssertEqual(marker.persistedCounts, [0, 1])
        XCTAssertEqual(marker.latchCount, 0)
    }

    func assertPresentationMatrixPersistenceFailures() async throws {
        for afterMismatch in [false, true] {
            let matrix = try matrixFixture()
            var retained: Data?
            matrix.present = { variant in
                if variant.minimum { throw AppUIHostPresentationMismatch() }
            }
            matrix.beforePersist = { rows in
                if rows.count == (afterMismatch ? 2 : 0) {
                    if afterMismatch {
                        retained = try matrix.matrixData()
                        XCTAssertEqual(try self.object(matrix.output, "ui-observations.json")["failure"] as? String,
                                       String(describing: AppUIHostPresentationMismatch()))
                    }
                    try matrix.report.write([:], name: "missing-parent/matrix.json")
                }
            }
            let error = await matrixFailure(matrix)
            XCTAssertEqual((error as NSError?)?.domain, NSCocoaErrorDomain)
            XCTAssertEqual(matrix.presentations.count, afterMismatch ? 2 : 0)
            XCTAssertEqual(matrix.captureAttempts.count, afterMismatch ? 1 : 0)
            XCTAssertEqual(matrix.persistAttempts, afterMismatch ? [0, 1, 2] : [0])
            XCTAssertEqual(matrix.persistedCounts, afterMismatch ? [0, 1] : [])
            XCTAssertEqual(matrix.latchCount, afterMismatch ? 1 : 0)
            if afterMismatch {
                XCTAssertEqual(try matrix.matrixData(), retained)
                XCTAssertEqual(try matrixRows(matrix).count, 1)
            } else {
                XCTAssertFalse(FileManager.default.fileExists(atPath: matrix.output.appendingPathComponent("host-presentation-matrix.json").path))
            }
        }
    }

    func assertPresentationMatrixTypedCallbackFailures() async throws {
        for phase in ["admission", "capture", "latch", "persist"] {
            let matrix = try matrixFixture()
            matrix.admission = {
                if phase == "admission" { throw AppUIHostPresentationMismatch() }
            }
            matrix.present = { _ in
                if phase == "latch" { throw AppUIHostPresentationMismatch() }
            }
            matrix.capture = { _ in
                if phase == "capture" { throw AppUIHostPresentationMismatch() }
            }
            matrix.afterLatch = { throw AppUIHostPresentationMismatch() }
            matrix.beforePersist = { rows in
                if phase == "persist" && rows.count == 1 { throw AppUIHostPresentationMismatch() }
            }
            let error = await matrixFailure(matrix)
            XCTAssertTrue(error is AppUIHostPresentationMismatch, phase)
            XCTAssertEqual(matrix.presentations.count, phase == "admission" ? 0 : 1, phase)
            XCTAssertEqual(matrix.persistedCounts, [0], phase)
            XCTAssertEqual(matrix.latchCount, phase == "latch" ? 1 : 0, phase)
            XCTAssertEqual(try matrixRows(matrix).count, 0, phase)
        }
    }
}
#endif

#if DEBUG && BRIDGEVM_APP_UI_HOST

extension AppUIHostContractTests {
    func assertPresentationMatrixOrderAndMismatchLatch() async throws {
        let names = ["welcome-light-default", "welcome-light-minimum", "welcome-dark-default", "welcome-dark-minimum"]
        let successful = try matrixFixture()
        var requests: [String] = []
        successful.present = { variant in
            requests.append("\(variant.dark):\(variant.minimum)")
            XCTAssertEqual(successful.persistedCounts.count, successful.presentations.count)
        }
        successful.beforePersist = { rows in
            XCTAssertEqual(successful.captures.count, rows.count)
            XCTAssertEqual(successful.observations, rows.count)
        }
        try await successful.run()
        XCTAssertEqual(requests, ["false:false", "false:true", "true:false", "true:true"])
        XCTAssertEqual(successful.presentations, names)
        XCTAssertEqual(successful.captures, names)
        XCTAssertEqual(successful.admissionCount, 8)
        XCTAssertEqual(successful.persistedCounts, [0, 1, 2, 3, 4])
        XCTAssertEqual(successful.latchCount, 0)
        let passed = try matrixRows(successful)
        XCTAssertEqual(passed.compactMap { $0["name"] as? String }, names)
        XCTAssertEqual(passed.compactMap { $0["outcome"] as? String }, Array(repeating: "passed", count: 4))
        XCTAssertEqual(passed.compactMap { ($0["observed"] as? [String: Int])?["fixture_observation"] }, [1, 2, 3, 4])
        XCTAssertTrue(try object(successful.output, "ui-observations.json")["failure"] is NSNull)

        let mismatched = try matrixFixture()
        let failure = String(describing: AppUIHostPresentationMismatch())
        mismatched.present = { variant in
            if variant.dark {
                XCTAssertEqual(try self.object(mismatched.output, "ui-observations.json")["failure"] as? String, failure)
                XCTAssertEqual(try self.matrixRows(mismatched)[1]["outcome"] as? String, "mismatch")
            }
            if variant.minimum { throw AppUIHostPresentationMismatch() }
        }
        mismatched.beforePersist = { rows in
            if rows.contains(where: { $0.outcome == .mismatch }) {
                XCTAssertEqual(try self.object(mismatched.output, "ui-observations.json")["failure"] as? String, failure)
            }
        }
        try await mismatched.run()
        XCTAssertEqual(mismatched.presentations, names)
        XCTAssertEqual(mismatched.captures, [names[0], names[2]])
        XCTAssertEqual(mismatched.captureAttempts, mismatched.captures)
        XCTAssertEqual(mismatched.latchCount, 2)
        XCTAssertEqual(mismatched.persistedCounts, [0, 1, 2, 3, 4])
        let rows = try matrixRows(mismatched)
        XCTAssertEqual(rows.compactMap { $0["name"] as? String }, names)
        XCTAssertEqual(rows.compactMap { $0["outcome"] as? String }, ["passed", "mismatch", "passed", "mismatch"])
        try mismatched.report.writeCompletion(cleanupVerified: true)
        let completion = try object(mismatched.output, "host-completion.json")
        XCTAssertEqual(completion["success"] as? Bool, false)
        XCTAssertEqual(completion["failure"] as? String, failure)
        XCTAssertEqual(completion["report_sha256"] as? String,
                       try AppUIHostCapture.digest(mismatched.output.appendingPathComponent("ui-observations.json")))
        XCTAssertEqual((try object(mismatched.output, "ui-observations.json")["screenshots"] as? [Any])?.count, 0)
    }

    func assertPresentationMatrixFatalBoundaries() async throws {
        for point in [AppUIHostMatrixFixture.Failure.ordinary, .ownership, .capture] {
            let matrix = try matrixFixture()
            matrix.present = { variant in
                guard variant.minimum else { return }
                if point == .ordinary { throw point }
                if point == .ownership { throw AppUIHostPresentationMismatch() }
            }
            matrix.admission = {
                if point == .ownership && matrix.admissionCount == 4 { throw point }
            }
            matrix.capture = { variant in
                if point == .capture && variant.minimum { throw point }
            }
            let error = await matrixFailure(matrix)
            XCTAssertEqual(error as? AppUIHostMatrixFixture.Failure, point)
            XCTAssertEqual(matrix.presentations, ["welcome-light-default", "welcome-light-minimum"])
            XCTAssertEqual(matrix.captures, ["welcome-light-default"])
            XCTAssertEqual(matrix.persistedCounts, [0, 1])
            XCTAssertEqual(matrix.observations, 1)
            XCTAssertEqual(matrix.latchCount, 0)
            XCTAssertEqual(try matrixRows(matrix).count, 1)
        }
        let laterFatal = try matrixFixture()
        laterFatal.present = { variant in
            if variant.dark { throw AppUIHostMatrixFixture.Failure.ordinary }
            if variant.minimum { throw AppUIHostPresentationMismatch() }
        }
        let error = await matrixFailure(laterFatal)
        XCTAssertEqual(error as? AppUIHostMatrixFixture.Failure, .ordinary)
        XCTAssertEqual(laterFatal.presentations.count, 3)
        XCTAssertEqual(laterFatal.captures, ["welcome-light-default"])
        XCTAssertEqual(laterFatal.persistedCounts, [0, 1, 2])
        XCTAssertEqual(try matrixRows(laterFatal).compactMap { $0["outcome"] as? String }, ["passed", "mismatch"])
        let retained = try laterFatal.matrixData()
        laterFatal.report.failed(try XCTUnwrap(error))
        try laterFatal.report.writeCompletion(cleanupVerified: true)
        XCTAssertEqual(try laterFatal.matrixData(), retained)
        XCTAssertEqual(try object(laterFatal.output, "host-completion.json")["success"] as? Bool, false)
    }
}
#endif

#if DEBUG && BRIDGEVM_APP_UI_HOST

extension AppUIHostContractTests {
    func assertAllIncompleteHostObservationContracts() async throws {
        try assertIncompleteObservationCompletion()
        try await assertPresentationMatrixOrderAndMismatchLatch()
        try await assertPresentationMatrixFatalBoundaries()
        try await assertPresentationMatrixCancellation()
        try await assertPresentationMatrixPersistenceFailures()
        try await assertPresentationMatrixTypedCallbackFailures()
        try assertAXWalkBounds()
        try assertAXProtocolAndIdentifierObservations()
        try await assertAXTimeoutPreservesResult()
    }

    func assertAXWalkBounds() throws {
        let root = AppUIHostAXNode(), child = AppUIHostAXNode()
        root.children = [child, child]
        child.children = [root]
        defer { root.children = []; child.children = [] }
        let cycle = axWalk(root)
        XCTAssertEqual(cycle["node_count"] as? Int, 2)
        XCTAssertEqual(cycle["duplicate_count"] as? Int, 2)
        XCTAssertEqual(root.inspections, 1)
        XCTAssertEqual(child.inspections, 1)
        XCTAssertEqual(cycle["truncated"] as? Bool, false)

        let deep = AppUIHostAXNode()
        var tail = deep
        for _ in 0..<35 { let next = AppUIHostAXNode(); tail.children = [next]; tail = next }
        let depth = axWalk(deep)
        XCTAssertEqual(depth["node_count"] as? Int, 33)
        XCTAssertEqual(try axRows(depth).compactMap { $0["depth"] as? Int }.max(), 32)
        XCTAssertEqual(depth["depth_truncated"] as? Bool, true)
        XCTAssertEqual(depth["children_omitted_by_depth_limit"] as? Int, 1)
        XCTAssertEqual(tail.inspections, 0)

        let wide = AppUIHostAXNode()
        wide.children = (0..<130).map { _ in AppUIHostAXNode() }
        let width = axWalk(wide)
        XCTAssertEqual(width["node_count"] as? Int, 129)
        XCTAssertEqual(width["child_limit"] as? Int, 128)
        XCTAssertEqual(width["child_truncated"] as? Bool, true)
        XCTAssertEqual(width["children_omitted_by_child_limit"] as? Int, 2)
        XCTAssertEqual(try axRows(width).first?["child_count"] as? Int, 130)
        XCTAssertEqual(try axRows(width).first?["children_queued"] as? Int, 128)

        let broad = AppUIHostAXNode()
        broad.children = (0..<128).map { _ in
            let branch = AppUIHostAXNode()
            branch.children = [AppUIHostAXNode(), AppUIHostAXNode()]
            return branch
        }
        let bounded = axWalk(broad)
        XCTAssertEqual(bounded["node_count"] as? Int, 256)
        XCTAssertEqual(bounded["node_limit"] as? Int, 256)
        XCTAssertEqual(bounded["node_truncated"] as? Bool, true)
        XCTAssertLessThanOrEqual(try XCTUnwrap(bounded["maximum_pending"] as? Int), 256)
        XCTAssertGreaterThan(try XCTUnwrap(bounded["children_omitted_by_node_limit"] as? Int), 0)
        let rejected = AppUIHostAXNode(), unseen = AppUIHostAXNode()
        rejected.rejected = true
        rejected.children = [unseen]
        let refusal = axWalk(rejected)
        XCTAssertEqual(refusal["full_protocol_rejection_count"] as? Int, 1)
        XCTAssertEqual(refusal["node_count"] as? Int, 1)
        XCTAssertTrue(try axRows(refusal)[0]["child_count"] is NSNull)
        XCTAssertEqual(unseen.inspections, 0)
    }

    func assertAXProtocolAndIdentifierObservations() throws {
        let root = AppUIHostAXButtonFixture(), create = AppUIHostAXButtonFixture(), importButton = AppUIHostAXButtonFixture()
        create.identifier = "bridgevm.first-run.create"
        importButton.identifier = "bridgevm.first-run.import"
        create.role = "AXButton"
        root.children = [create, importButton]
        let strict = AppUIHostAXObservation.graph(root, publicSelectors: false)
        XCTAssertEqual(strict["node_count"] as? Int, 1)
        XCTAssertEqual(root.childrenReads, 0)
        let strictRoot = try axRows(strict)[0]
        XCTAssertEqual(strictRoot["conforms_element_protocol"] as? Bool, true)
        XCTAssertEqual(strictRoot["conforms_button_protocol"] as? Bool, true)
        XCTAssertEqual(strictRoot["conforms_full_protocol"] as? Bool, false)
        XCTAssertEqual(strictRoot["existing_walker_rejection"] as? String, "not-full-protocol")
        let observed = AppUIHostAXObservation.graph(root, publicSelectors: true)
        XCTAssertEqual(observed["node_count"] as? Int, 3)
        XCTAssertEqual(observed["full_protocol_rejection_count"] as? Int, 3)
        XCTAssertEqual(root.childrenReads, 1)
        XCTAssertEqual(observed["route"] as? String, "responding-public-getters-observation-only")
        XCTAssertEqual(observed["allowed_identifier_counts"] as? [String: Int],
                       ["bridgevm.first-run.create": 1, "bridgevm.first-run.import": 1])
        XCTAssertEqual(try axRows(observed).compactMap { $0["allowed_role"] as? String }, ["AXButton"])
        let encoded = try JSONSerialization.data(withJSONObject: observed)
        let text = String(decoding: encoded, as: UTF8.self)
        for secret in [root.identifier, root.role, "private-label", "private-value", "private-title"] {
            XCTAssertFalse(text.contains(secret))
        }
        XCTAssertEqual([root, create, importButton].map { $0.forbiddenReads }, [0, 0, 0])
        XCTAssertTrue(try axRows(observed).allSatisfy { (($0["type"] as? String)?.count ?? 129) <= 128 })
    }

    func assertAXTimeoutPreservesResult() async throws {
        var observations = 0
        try await AppUIHostAccessibility.wait("welcome controls", onTimeout: { observations += 1 }) { true }
        XCTAssertEqual(observations, 0)
        do {
            try await AppUIHostAccessibility.wait("welcome controls", onTimeout: { observations += 1 }) {
                throw AppUIHostMatrixFixture.Failure.ordinary
            }
            XCTFail("Predicate error unexpectedly passed")
        } catch { XCTAssertEqual(error as? AppUIHostMatrixFixture.Failure, .ordinary) }
        XCTAssertEqual(observations, 0)
        let cancelled = await Task { @MainActor in
            withUnsafeCurrentTask { $0?.cancel() }
            do {
                try await AppUIHostAccessibility.wait("welcome controls", onTimeout: { observations += 1 }) { false }
                return false
            } catch is CancellationError { return true }
            catch { return false }
        }.value
        XCTAssertTrue(cancelled)
        XCTAssertEqual(observations, 0)
        assertWelcomeTimeout {
            try AppUIHostAccessibility.timeout("welcome controls", onTimeout: { observations += 1 })
        }
        XCTAssertEqual(observations, 1)
        let fixture = try fixture(), capture = try AppUIHostCapture(output: fixture.output)
        let retained: [String: Any] = ["fixture": "retained first observation"]
        try AppUIHostAXObservation.write(retained, capture: capture)
        let path = fixture.output.appendingPathComponent(AppUIHostAXObservation.filename)
        let before = try Data(contentsOf: path)
        assertWelcomeTimeout {
            try AppUIHostAccessibility.timeout("welcome controls", onTimeout: {
                observations += 1
                try AppUIHostAXObservation.write(["replacement": true], capture: capture)
            })
        }
        XCTAssertEqual(observations, 2)
        XCTAssertEqual(try Data(contentsOf: path), before)
        XCTAssertEqual(try object(fixture.output, "ui-observations.json")["failure"] is NSNull, true)
    }
}
#endif

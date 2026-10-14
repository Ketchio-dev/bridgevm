import XCTest
@testable import BridgeVMControl

@MainActor
final class LibraryWarningNavigationTests: XCTestCase {
    func testMalformedOnlyLibraryExposesAllIssueDetailsFromCurrentOverview() throws {
        try assertWarningRoute(includeRegisteredVM: false)
    }

    func testMixedLibraryKeepsWarningsReachableAlongsideRegisteredVMs() throws {
        try assertWarningRoute(includeRegisteredVM: true)
    }

    func testHealthyLibraryDoesNotDisplayAWarningAction() throws {
        try assertWarningRoute(includeRegisteredVM: true, issueCount: 0)
    }

    private func assertWarningRoute(includeRegisteredVM: Bool, issueCount: Int = 5) throws {
        let base = ProcessInfo.processInfo.environment["TMPDIR"] ?? NSTemporaryDirectory()
        let root = URL(fileURLWithPath: base).appendingPathComponent("library-warning-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        var issuePaths: Set<String> = []
        for index in 0..<issueCount {
            let directory = root.appendingPathComponent("broken-\(index)")
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            let file = directory.appendingPathComponent("vm.json")
            try Data("{invalid".utf8).write(to: file)
            issuePaths.insert(file.path)
        }
        if includeRegisteredVM {
            let config = VMConfig(id: "registered", name: "Registered", displayName: "Registered",
                backendKind: "hvf-engine", bootMode: "windows-hvf", bundlePath: root.appendingPathComponent("registered/bundle").path,
                runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "",
                guestName: "owned", displayWidth: 1280, displayHeight: 800)
            XCTAssertTrue(VMLibrary.save(config, rootURL: root))
        }
        let library = LibraryModel(rootURL: root, migrateLegacy: false, startsModelsAutomatically: false)
        XCTAssertEqual(library.vms.count, includeRegisteredVM ? 1 : 0)
        XCTAssertEqual(Set(library.libraryIssues.map(\.path)), issuePaths)
        let overview = LibraryOverviewView(library: library).body
        let workspace = try XCTUnwrap(values(LibraryWorkspaceList.self, in: overview).first)
        let disclosure = values(LibraryIssuesSection.self, in: workspace.body)
        XCTAssertEqual(disclosure.count, issueCount == 0 ? 0 : 1, "Current overview must expose the issue detail control, not only a count")
        XCTAssertEqual(disclosure.first?.issues, issueCount == 0 ? nil : library.libraryIssues)
        XCTAssertEqual(disclosure.first?.compact, issueCount == 0 ? nil : true, "The dashboard keeps the compact warning action")
        for path in issuePaths {
            XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: path)), Data("{invalid".utf8))
        }
    }

    private func values<T>(_ type: T.Type, in value: Any) -> [T] {
        if let typed = value as? T { return [typed] }
        let mirror = Mirror(reflecting: value)
        guard mirror.displayStyle != .class else { return [] }
        return mirror.children.flatMap { values(type, in: $0.value) }
    }
}

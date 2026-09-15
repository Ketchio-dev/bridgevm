#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class AppUIHostMatrixFixture {
    typealias Matrix = AppUIHostPresentationMatrix
    enum Failure: Error, Equatable { case ordinary, ownership, capture }
    let output: URL
    let report: AppUIHostCapture
    var presentations: [String] = []
    var captureAttempts: [String] = []
    var captures: [String] = []
    var admissionCount = 0
    var observations = 0
    var persistAttempts: [Int] = []
    var persistedCounts: [Int] = []
    var latchCount = 0
    var admission: (() throws -> Void)?
    var present: ((Matrix.Variant) async throws -> Void)?
    var capture: ((Matrix.Variant) throws -> Void)?
    var afterLatch: (() throws -> Void)?
    var beforePersist: (([Matrix.Row]) throws -> Void)?

    init(output: URL) throws {
        self.output = output
        report = try AppUIHostCapture(output: output)
    }
    func run() async throws {
        defer {
            admission = nil
            present = nil
            capture = nil
            afterLatch = nil
            beforePersist = nil
        }
        try await Matrix.run(admission: {
            self.admissionCount += 1
            try self.admission?()
        }, present: { variant in
            self.presentations.append(variant.name)
            try await self.present?(variant)
        }, capture: { variant in
            self.captureAttempts.append(variant.name)
            try self.capture?(variant)
            self.captures.append(variant.name)
        }, mismatch: { error in
            self.latchCount += 1
            self.report.failed(error)
            try self.report.save()
            try self.afterLatch?()
        }, observe: {
            self.observations += 1
            return ["fixture_observation": self.observations]
        }, persist: { rows in
            self.persistAttempts.append(rows.count)
            try self.beforePersist?(rows)
            try self.report.write(["rows": rows.map {
                ["name": $0.variant.name, "outcome": $0.outcome.rawValue, "observed": $0.observation] as [String: Any]
            }], name: "host-presentation-matrix.json")
            self.persistedCounts.append(rows.count)
        })
    }
    func matrixData() throws -> Data {
        try Data(contentsOf: output.appendingPathComponent("host-presentation-matrix.json"))
    }
}

extension AppUIHostContractTests {
    func matrixFixture() throws -> AppUIHostMatrixFixture {
        try AppUIHostMatrixFixture(output: fixture().output)
    }
    func matrixRows(_ matrix: AppUIHostMatrixFixture) throws -> [[String: Any]] {
        try XCTUnwrap(object(matrix.output, "host-presentation-matrix.json")["rows"] as? [[String: Any]])
    }
    func matrixFailure(_ matrix: AppUIHostMatrixFixture,
                       file: StaticString = #filePath, line: UInt = #line) async -> Error? {
        do {
            try await matrix.run()
            XCTFail("Matrix unexpectedly continued through a fatal callback", file: file, line: line)
            return nil
        } catch { return error }
    }
    func assertIncompleteObservationCompletion() throws {
        let fixture = try fixture()
        let capture = try AppUIHostCapture(output: fixture.output)
        try capture.writeCompletion(cleanupVerified: true)
        let completion = try object(fixture.output, "host-completion.json")
        XCTAssertEqual(completion["success"] as? Bool, false)
        XCTAssertNotNil(completion["failure"] as? String)
        XCTAssertEqual(completion["report_sha256"] as? String,
                       try AppUIHostCapture.digest(fixture.output.appendingPathComponent("ui-observations.json")))
        XCTAssertThrowsError(try capture.writeCompletion(cleanupVerified: true))
    }
}
#endif

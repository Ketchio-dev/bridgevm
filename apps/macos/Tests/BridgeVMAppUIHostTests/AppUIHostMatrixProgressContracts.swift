#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation
import XCTest
@testable import BridgeVMControl

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

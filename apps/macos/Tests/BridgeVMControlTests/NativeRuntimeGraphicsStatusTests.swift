import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class NativeRuntimeGraphicsStatusTests: XCTestCase {
    func testOwnedExperimentalLaunchReportsFrozenPolicy() throws {
        let f = try HvfRuntimeRetainedControlFixture()
        defer { f.clean() }
        var config = f.work.config()
        config.experimental3DAllowed = true
        f.work.save(config)
        let library = f.work.library(), session = try f.work.runtime(config, in: library)
        session.ownedProcessIdentity = .init(token: UUID(), processID: 42)
        let value = try XCTUnwrap(library.runtimeObservations(
            slug: config.slug, requestedConfigurationIdentity: nil).first)
        XCTAssertEqual(value.ownership, .owned)
        XCTAssertEqual(value.graphicsMode, .experimental3D)
    }

    func testMissingFieldKeepsOldV1ResponseDecodable() throws {
        let value = NativeRuntimeSessionObservation(
            ownership: .owned, connectionState: "booting", acceptedConfigurationDigest: nil,
            configurationMatch: .unknown,
            ownedProcess: .init(token: UUID().uuidString, processID: 42), lastOwnedExit: nil,
            graphicsMode: nil)
        let bytes = try NativeRuntimeCodec.encode(value)
        XCTAssertFalse(String(decoding: bytes, as: UTF8.self).contains("graphicsMode"))
        XCTAssertEqual(try NativeRuntimeCodec.decode(NativeRuntimeSessionObservation.self, from: bytes, limit: 8192), value)
        try NativeRuntimeCodec.validate(value, saved: .init(state: .missing, digest: nil))
    }

    func testGraphicsClaimMustMatchOwnershipEvidence() throws {
        let process = NativeRuntimeProcessObservation(token: UUID().uuidString, processID: 42)
        let accepted: [(NativeRuntimeSessionObservation.Ownership, NativeRuntimeSessionObservation.GraphicsMode)] = [
            (.owned, .basic3DOff), (.owned, .experimental3D), (.attachedObservation, .unverified)]
        for (ownership, mode) in accepted {
            let value = NativeRuntimeSessionObservation(
                ownership: ownership, connectionState: "booting", acceptedConfigurationDigest: nil,
                configurationMatch: .unknown, ownedProcess: ownership == .owned ? process : nil,
                lastOwnedExit: nil, graphicsMode: mode)
            try NativeRuntimeCodec.validate(value, saved: .init(state: .missing, digest: nil))
        }
        let rejected = NativeRuntimeSessionObservation(
            ownership: .attachedObservation, connectionState: "booting", acceptedConfigurationDigest: nil,
            configurationMatch: .unknown, ownedProcess: nil, lastOwnedExit: nil, graphicsMode: .experimental3D)
        XCTAssertThrowsError(try NativeRuntimeCodec.validate(
            rejected, saved: .init(state: .missing, digest: nil)))
    }
}

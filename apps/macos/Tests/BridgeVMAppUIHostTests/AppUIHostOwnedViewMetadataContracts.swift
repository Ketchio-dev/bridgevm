#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation
import XCTest
@testable import BridgeVMControl

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

import Foundation
import XCTest
@testable import BridgeVMProductE2E

enum T17FileChooserDiagnosticEnvelope {
    static func inspect(_ failure: T17Blocker?) -> (original: String, footer: String) {
        guard let failure else { XCTFail("Expected a chooser refusal"); return ("", "") }
        XCTAssertEqual(failure.code, "input-selection-failed")
        let marker = "; chooser_timing{"
        guard let boundary = failure.detail.range(of: marker, options: .backwards), failure.detail.hasSuffix("}") else {
            XCTFail("Missing complete chooser timing envelope"); return (failure.detail, "")
        }
        let original = String(failure.detail[..<boundary.lowerBound])
        let footer = String(failure.detail[boundary.lowerBound...])
        let payload = String(footer.dropFirst(marker.count).dropLast())
        guard !payload.isEmpty, !payload.contains("{"), !payload.contains("}"), !payload.contains("\n"), !payload.contains("\r") else {
            XCTFail("Malformed chooser timing envelope"); return (failure.detail, "")
        }
        XCTAssertEqual(original + footer, failure.detail)
        XCTAssertTrue(footer.count <= 900)
        XCTAssertTrue(footer.hasPrefix(marker + "clock=valid,last_complete=show-location-field,records=5,dropped=0;"))
        XCTAssertTrue(footer.hasSuffix("location-field-ready:entry_ms=0.000,elapsed_ms=1000.000,remaining_ms=0.000,return=threw}"))
        XCTAssertFalse(footer.contains("/private/input.iso"))
        return (original, footer)
    }
}

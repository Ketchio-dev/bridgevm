import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfFramebufferCachedFrameTests: XCTestCase {
    private typealias Frame = HvfFramebufferAvailabilityFixture

    func testIdleAndOddSequenceKeepValidCachedFrameAndComposition() throws {
        try Frame.run { fixture, session, view, path in
            let image = try Frame.image(view), target = session.textInputTarget
            Frame.mark(view)
            view.refreshFrame(at: 1)
            try Frame.update(UInt64(3), offset: 24, path: path)
            view.refreshFrame(at: 2)
            XCTAssertTrue(try Frame.image(view) === image)
            XCTAssertTrue(view.guestInputAvailable)
            XCTAssertTrue(view.hasMarkedText())
            XCTAssertEqual(session.textInputTarget, target)
            XCTAssertEqual(try fixture.commands().count, 1)
        }
    }

    func testProviderFailureKeepsCachedFrameAndRetriesSameSequence() throws {
        let defaults = HvfFramebufferImageFactory()
        var factory = defaults, calls = 0
        factory.provider = { bytes, count in
            calls += 1
            return calls == 2 ? nil : defaults.provider(bytes, count)
        }
        try Frame.run(factory: factory) { _, session, view, path in
            let image = try Frame.image(view), target = session.textInputTarget
            try Frame.update(UInt64(4), offset: 24, path: path)
            view.refreshFrame(at: 1)
            XCTAssertTrue(try Frame.image(view) === image)
            XCTAssertTrue(view.guestInputAvailable)
            XCTAssertEqual(session.textInputTarget, target)
            view.refreshFrame(at: 2)
            XCTAssertEqual(calls, 3)
            XCTAssertFalse(try Frame.image(view) === image)
        }
    }

    func testReplacementWithSameSequenceRequiresNewValidatedFrame() throws {
        try Frame.run { _, session, view, path in
            let target = session.textInputTarget
            let replacement = path.appendingPathExtension("new")
            try Frame.writeFrame(replacement, pixel: 0xffaabbcc)
            XCTAssertEqual(Darwin.rename(replacement.path, path.path), 0)
            view.refreshFrame(at: 1)
            XCTAssertFalse(view.guestInputAvailable)
            XCTAssertNil(view.layer?.contents)
            XCTAssertNotEqual(session.textInputTarget, target)
            view.refreshFrame(at: 2)
            XCTAssertTrue(view.guestInputAvailable)
            let bytes = try XCTUnwrap(try Frame.image(view).dataProvider?.data) as Data
            XCTAssertEqual(bytes, Data([0xcc, 0xbb, 0xaa, 0xff]))
        }
    }
}

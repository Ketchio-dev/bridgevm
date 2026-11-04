import AppKit
import CoreGraphics
import Foundation
import QuartzCore
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfRetinaResolutionTests: XCTestCase {
    private func ownedArguments(_ config: HvfEngineConfig) -> [String] {
        config.runnerArguments(manifestPath: "m", runnerPath: "r", firmwareCodePath: "f", probePath: "p", ownedRuntime: true)
    }

    /// Off until the user saves it, offered only with the signed display driver and 3D off, and the
    /// launch is the same either way: only the app's resize request changes.
    func testRetinaResolutionIsOffUntilTheUserSavesIt() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        var saved = fixture.config(); saved.virtioDisplayDriverInstalled = true; fixture.save(saved)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        XCTAssertFalse(session.config.retinaResolution, "An absent key keeps the 1x request")
        XCTAssertTrue(session.config.offersRetinaResolution)
        XCTAssertFalse(session.config.requestsRetinaResolution)
        let arguments = ownedArguments(session.config), wrapper = session.config.wrapperArguments()

        XCTAssertTrue(session.applyRetinaResolutionToggle(true) == (true, nil))
        let latest = library.latestConfiguration(for: saved)
        XCTAssertEqual(latest.retinaResolution, true)
        XCTAssertTrue(library.hvfRuntimeSessions.owns(session, for: latest), "Rebound in place")
        XCTAssertTrue(session.config.requestsRetinaResolution)
        XCTAssertEqual(ownedArguments(session.config), arguments)
        XCTAssertEqual(session.config.wrapperArguments(), wrapper)

        // Turning it off again restores the never-touched registration.
        XCTAssertTrue(session.applyRetinaResolutionToggle(false) == (false, nil))
        XCTAssertNil(library.latestConfiguration(for: saved).retinaResolution)
        let restored = try Data(contentsOf: fixture.registration(saved))
        XCTAssertFalse(String(decoding: restored, as: UTF8.self).contains("retinaResolution"))

        for (driver, allows3D, offered) in [(nil, nil, false), (true, true, false), (true, false, true)] as [(Bool?, Bool?, Bool)] {
            var other = saved; other.virtioDisplayDriverInstalled = driver; other.experimental3DAllowed = allows3D
            XCTAssertEqual(HvfEngineConfig.libraryVM(other, rootURL: fixture.root)?.offersRetinaResolution, offered)
        }
    }

    /// Stopped-only, like the other saved preferences: a running VM keeps the registration it started with.
    func testTheToggleIsDisabledAndTheSaveRefusedWhileRunning() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        var saved = fixture.config(); saved.virtioDisplayDriverInstalled = true; fixture.save(saved)
        let url = fixture.registration(saved), original = try Data(contentsOf: url)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        let toggle = HvfRetinaResolutionToggle(session: session, isOn: .constant(false))
        XCTAssertFalse(toggle.isDisabled)
        session.connectionState = .connected(host: "fixture")
        XCTAssertTrue(toggle.isDisabled)
        XCTAssertTrue(session.applyRetinaResolutionToggle(true)
            == (false, "VM을 완전히 중지한 뒤 Retina 해상도 설정을 변경하세요."))
        XCTAssertEqual(try Data(contentsOf: url), original)
        XCTAssertFalse(session.config.requestsRetinaResolution)
        session.connectionState = .stopped
    }

    func testHelpTextIsKoreanAndMarkedExperimental() {
        let text = HvfRetinaResolutionToggle.helpText(persists: true)
        for needle in ["실험적 기능", "픽셀 수", "200%", "CPU", "흐리게", "일반 해상도", "VM이 중지된 동안"] {
            XCTAssertTrue(text.contains(needle), needle)
        }
        XCTAssertFalse(HvfRetinaResolutionToggle.helpText(persists: false).contains("VM이 중지된 동안"))
    }

    func testAFrameOfTheBackingSizeIsShownOneToOne() {
        func gravity(_ frame: CGSize, _ view: CGSize, _ scale: CGFloat) -> CALayerContentsGravity {
            FBLayerView.contentsGravity(frame: frame, view: view, scale: scale)
        }
        let window = CGSize(width: 1280, height: 800)
        XCTAssertEqual(gravity(CGSize(width: 2560, height: 1600), window, 2), CALayerContentsGravity.center)
        XCTAssertEqual(gravity(CGSize(width: 2001, height: 1400), CGSize(width: 1000.5, height: 700), 2),
                       CALayerContentsGravity.center)
        XCTAssertEqual(gravity(window, window, 1), CALayerContentsGravity.center)
        XCTAssertEqual(gravity(window, window, 2), CALayerContentsGravity.resizeAspect, "a frame in points on a 2x screen")
        XCTAssertEqual(gravity(CGSize(width: 2560, height: 1600), CGSize(width: 1300, height: 800), 2),
                       CALayerContentsGravity.resizeAspect, "the window changed before the guest's mode did")
        XCTAssertEqual(gravity(.zero, .zero, 2), CALayerContentsGravity.resizeAspect)
    }

    func testTheViewPlacesFramesAgainAfterAResize() throws {
        try HvfFramebufferAvailabilityFixture.run { _, _, view, _ in
            // The fixture shows a 1x1 frame in a 100x100 view without a window, so at scale 1.
            XCTAssertEqual(view.layer?.contentsGravity, CALayerContentsGravity.resizeAspect)
            view.setFrameSize(NSSize(width: 1, height: 1))
            XCTAssertEqual(view.layer?.contentsGravity, CALayerContentsGravity.center)
            XCTAssertEqual(view.layer?.contentsScale, 1)
            view.setFrameSize(NSSize(width: 100, height: 100))
            XCTAssertEqual(view.layer?.contentsGravity, CALayerContentsGravity.resizeAspect)
        }
    }

    /// The pointer is normalised to 0...32767 over the shown frame, so a frame in backing pixels maps
    /// exactly as the same window's frame in points.
    func testPointerMappingIsTheSameForAFrameInBackingPixels() {
        let view = CGSize(width: 1280, height: 800), pixels = CGSize(width: 2560, height: 1600)
        for location in [CGPoint(x: 0, y: 0), CGPoint(x: 640, y: 400), CGPoint(x: 1280, y: 800), CGPoint(x: 333.5, y: 701.25)] {
            let expected = HvfDisplayCoordinates.absolutePointer(location: location, viewSize: view, imageSize: view)
            let mapped = HvfDisplayCoordinates.absolutePointer(location: location, viewSize: view, imageSize: pixels)
            XCTAssertNotNil(mapped)
            XCTAssertEqual(mapped?.x, expected?.x)
            XCTAssertEqual(mapped?.y, expected?.y)
        }
        let corner = HvfDisplayCoordinates.absolutePointer(location: CGPoint(x: 1280, y: 800), viewSize: view, imageSize: pixels)
        XCTAssertEqual(corner?.x, 32_767)
        XCTAssertEqual(corner?.y, 32_767)
    }

    /// Pointer mapping and the guest's hardware cursor (its sprite, and the Mac cursor's size) place the
    /// frame with HvfDisplayFit. For a frame of the backing size that is the whole view at 1/scale,
    /// exactly where `.center` at `contentsScale = scale` shows it.
    func testTheDisplayFitOfABackingSizeFrameIsTheWholeView() throws {
        for view in [CGSize(width: 1280, height: 800), CGSize(width: 1728, height: 1117), CGSize(width: 1000.5, height: 700)] {
            let frame = CGSize(width: view.width * 2, height: view.height * 2)
            XCTAssertEqual(FBLayerView.contentsGravity(frame: frame, view: view, scale: 2), CALayerContentsGravity.center)
            let fit = try XCTUnwrap(HvfDisplayFit(viewSize: view, imageSize: frame))
            XCTAssertEqual(fit.rect, CGRect(origin: .zero, size: view), "\(view)")
            XCTAssertEqual(fit.scale, 0.5, "a 64-pixel guest cursor is 32 points")
        }
    }
}

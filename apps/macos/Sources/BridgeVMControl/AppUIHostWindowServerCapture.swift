#if DEBUG && BRIDGEVM_APP_UI_HOST
import AppKit
import ScreenCaptureKit
import Darwin

@MainActor
enum AppUIHostWindowServerCapture {
    static func captureDarkDefault(_ host: AppUIHost, window: NSWindow, content: NSView) async throws {
        guard #available(macOS 14.4, *) else {
            throw AppUIHostError.refused("Current-process WindowServer observation requires macOS 14.4")
        }
        let windowNumber = window.windowNumber
        func admit() throws {
            try Task.checkCancellation()
            try AppUIHost.checkCancellation(output: host.capture.output)
            guard AppUIHost.prepared === host, window.isVisible, window.windowNumber == windowNumber,
                  windowNumber > 0, windowNumber <= Int(UInt32.max),
                  window.contentView === content, content.window === window else {
                throw AppUIHostError.refused("WindowServer observation lost its visible owned original content")
            }
        }
        try admit()
        let output = try AppUIHostWindowServerPolicy.Output(parent: host.capture.output)
        let began = ProcessInfo.processInfo.systemUptime
        // This API exposes consent-free current-process content. Never fall back to global content or a picker.
        try admit()
        let shareable = try await SCShareableContent.currentProcess
        try admit()
        let candidates = shareable.windows.map {
            AppUIHostWindowServerPolicy.Candidate(windowID: $0.windowID,
                processID: $0.owningApplication.map { Int($0.processID) }, onScreen: $0.isOnScreen)
        }
        let index = try AppUIHostWindowServerPolicy.select(candidates, windowNumber: windowNumber, processID: Int(getpid()))
        let selected = shareable.windows[index]
        let filter = SCContentFilter(desktopIndependentWindow: selected)
        let rect = filter.contentRect, scale = Double(filter.pointPixelScale)
        let size = try AppUIHostWindowServerPolicy.dimensions(x: rect.origin.x, y: rect.origin.y,
            width: rect.width, height: rect.height, scale: scale)
        let configuration = SCStreamConfiguration()
        configuration.width = size.width
        configuration.height = size.height
        configuration.showsCursor = false
        configuration.capturesAudio = false
        configuration.includeChildWindows = false
        configuration.ignoreShadowsSingleWindow = true
        try admit()
        let image = try await SCScreenshotManager.captureImage(contentFilter: filter, configuration: configuration)
        try admit()
        guard image.width == size.width, image.height == size.height,
              let png = NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:]) else {
            throw AppUIHostError.refused("WindowServer image dimensions or PNG encoding differ")
        }
        let record: [String: Any] = [
            "schema_version": 1, "kind": "native-app-ui-host-window-server", "supplemental_only": true,
            "pid": Int(getpid()), "window_id": selected.windowID, "owner_pid": candidates[index].processID!,
            "scope": "current-process-exact-owned-window", "requested_dark": true, "requested_minimum": false,
            "began_uptime": began, "completed_uptime": ProcessInfo.processInfo.systemUptime,
            "content_rect_points": ["x": rect.origin.x, "y": rect.origin.y, "width": rect.width, "height": rect.height],
            "point_pixel_scale": scale, "configuration": ["width": configuration.width, "height": configuration.height,
                "shows_cursor": configuration.showsCursor, "captures_audio": configuration.capturesAudio,
                "include_child_windows": configuration.includeChildWindows,
                "ignore_shadows_single_window": configuration.ignoreShadowsSingleWindow] as [String: Any],
            "image": ["file": "owned-dark-default.png", "width": image.width, "height": image.height,
                      "bytes": png.count, "sha256": AppUIHostCapture.digest(png)] as [String: Any],
            "record_time_nonatomic": true, "owned_visible_content_revalidated": true
        ]
        let json = try JSONSerialization.data(withJSONObject: record, options: [.prettyPrinted, .sortedKeys])
        try admit()
        try output.write(png, name: "owned-dark-default.png")
        try admit()
        try output.write(json, name: "owned-dark-default.json")
    }
}
#endif

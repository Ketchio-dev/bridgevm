import Foundation

enum GuestWindowProxyFrameLimits {
  static let maximumPixelCount = 32 * 1024 * 1024

  static func supports(width: Int, height: Int) -> Bool {
    width > 0 && height > 0 && width <= maximumPixelCount / height
  }
}

struct GuestWindowProxyPlan: Equatable {
  struct HostSize: Equatable {
    var width: Int
    var height: Int
  }

  struct FramebufferSize: Equatable {
    var width: Int
    var height: Int
  }

  struct HostPoint: Equatable {
    var x: Double
    var y: Double
  }

  struct HostFrame: Equatable {
    var x: Double
    var y: Double
    var width: Double
    var height: Double
  }

  struct GuestPoint: Equatable {
    var x: Int
    var y: Int
  }

  var vmName: String
  var windowID: String
  var title: String
  var guestBounds: GuestToolsWindowBounds
  var hostSize: HostSize
  var scale: Double
  var pid: Int?
  var desktop: Int?
  var cropFrameSummaryPath: String?

  var inputScaleX: Double {
    Double(guestBounds.width) / Double(hostSize.width)
  }

  var inputScaleY: Double {
    Double(guestBounds.height) / Double(hostSize.height)
  }

  var minimumFramebufferSize: FramebufferSize {
    let right = guestBounds.x.addingReportingOverflow(guestBounds.width)
    let bottom = guestBounds.y.addingReportingOverflow(guestBounds.height)
    return FramebufferSize(
      width: max(1, max(guestBounds.width, right.overflow ? guestBounds.width : right.partialValue)),
      height: max(1, max(guestBounds.height, bottom.overflow ? guestBounds.height : bottom.partialValue))
    )
  }

  var summary: String {
    let pidText = pid.map { "pid \($0), " } ?? ""
    return
      "\(title) (\(windowID), \(pidText)guest \(guestBounds.displayText), host \(hostSize.width)x\(hostSize.height))"
  }

  func displaydWindowRegionArguments(
    framebufferSize requestedFramebufferSize: FramebufferSize? = nil,
    backingScale requestedBackingScale: Int = 1
  ) -> [String] {
    let framebufferSize = requestedFramebufferSize ?? minimumFramebufferSize
    let backingScale = max(1, requestedBackingScale)
    var arguments = [
      "--framebuffer-width",
      "\(max(1, framebufferSize.width))",
      "--framebuffer-height",
      "\(max(1, framebufferSize.height))",
      "--scale",
      "\(backingScale)",
      "--window-id",
      windowID,
    ]
    if !title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
      arguments.append(contentsOf: ["--window-title", title])
    }
    arguments.append(contentsOf: [
      "--window-x",
      "\(guestBounds.x)",
      "--window-y",
      "\(guestBounds.y)",
      "--window-width",
      "\(guestBounds.width)",
      "--window-height",
      "\(guestBounds.height)",
      "--window-host-width",
      "\(hostSize.width)",
      "--window-host-height",
      "\(hostSize.height)",
    ])
    return arguments
  }

  func displaydWindowCropArguments(
    framebufferSize requestedFramebufferSize: FramebufferSize? = nil,
    backingScale requestedBackingScale: Int = 1,
    framebufferRGBAFile: String,
    windowCropRGBAFile: String
  ) -> [String] {
    displaydWindowRegionArguments(
      framebufferSize: requestedFramebufferSize,
      backingScale: requestedBackingScale
    ) + [
      "--framebuffer-rgba-file",
      framebufferRGBAFile,
      "--window-crop-rgba-file",
      windowCropRGBAFile,
    ]
  }

  /// Where this window should open on the host so that it keeps the position it
  /// has inside the guest desktop.
  ///
  /// Guest desktops measure y downward from the top; AppKit measures it upward
  /// from the bottom, so the y axis is flipped against the visible frame. The
  /// result is clamped so a window positioned off the guest desktop, or one
  /// scaled down to fit, still lands somewhere the user can reach it.
  func hostOrigin(inVisibleFrame visibleFrame: HostFrame) -> HostPoint {
    let width = Double(hostSize.width)
    let height = Double(hostSize.height)
    let x = visibleFrame.x + Double(guestBounds.x) * scale
    let flippedY = Double(guestBounds.y) * scale + height
    let y = visibleFrame.y + visibleFrame.height - flippedY
    let maxX = visibleFrame.x + max(0, visibleFrame.width - width)
    let maxY = visibleFrame.y + max(0, visibleFrame.height - height)
    return HostPoint(
      x: min(max(x, visibleFrame.x), maxX),
      y: min(max(y, visibleFrame.y), maxY)
    )
  }

  func guestPoint(forHostPoint hostPoint: HostPoint) -> GuestPoint {
    let guestXOffset = Int((hostPoint.x * inputScaleX).rounded())
    let guestYOffset = Int((hostPoint.y * inputScaleY).rounded())
    let maxGuestX = guestBounds.x + max(0, guestBounds.width - 1)
    let maxGuestY = guestBounds.y + max(0, guestBounds.height - 1)
    return GuestPoint(
      x: clamp(guestBounds.x + guestXOffset, lower: guestBounds.x, upper: maxGuestX),
      y: clamp(guestBounds.y + guestYOffset, lower: guestBounds.y, upper: maxGuestY)
    )
  }

  func guestBounds(
    forHostContentFrame hostFrame: HostFrame,
    relativeTo baselineHostFrame: HostFrame
  ) -> GuestToolsWindowBounds {
    let deltaX = hostFrame.x - baselineHostFrame.x
    let deltaY = hostFrame.y - baselineHostFrame.y
    return GuestToolsWindowBounds(
      x: guestBounds.x + Int((deltaX * inputScaleX).rounded()),
      y: guestBounds.y - Int((deltaY * inputScaleY).rounded()),
      width: max(1, Int((hostFrame.width * inputScaleX).rounded())),
      height: max(1, Int((hostFrame.height * inputScaleY).rounded()))
    )
  }

  private func clamp(_ value: Int, lower: Int, upper: Int) -> Int {
    min(max(value, lower), upper)
  }
}

#if canImport(AppKit)
import AppKit

extension GuestWindowProxyPlan {
  /// Places a freshly created proxy window where its guest window sits, falling
  /// back to centring when no screen can be resolved.
  func placeKeepingGuestPosition(_ window: NSWindow) {
    guard let visible = (window.screen ?? NSScreen.main)?.visibleFrame else {
      window.center()
      return
    }
    let origin = hostOrigin(
      inVisibleFrame: HostFrame(
        x: visible.origin.x, y: visible.origin.y,
        width: visible.width, height: visible.height
      )
    )
    window.setFrameOrigin(NSPoint(x: origin.x, y: origin.y))
  }
}
#endif

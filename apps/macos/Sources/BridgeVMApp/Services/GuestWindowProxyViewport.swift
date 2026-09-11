import Foundation

extension GuestWindowProxyPlan {
  /// Invert the centered aspect-fit image transform, including resized viewports.
  func viewportPoint(_ point: HostPoint, width: Double, height: Double) -> GuestPoint? {
    guard width.isFinite, height.isFinite, width > 0, height > 0,
      point.x.isFinite, point.y.isFinite,
      guestBounds.width > 0, guestBounds.height > 0,
      hostSize.width > 0, hostSize.height > 0
    else { return nil }
    let factor = min(width / Double(guestBounds.width), height / Double(guestBounds.height))
    let imageWidth = Double(guestBounds.width) * factor
    let imageHeight = Double(guestBounds.height) * factor
    guard imageWidth.isFinite, imageHeight.isFinite, imageWidth > 0, imageHeight > 0
    else { return nil }
    let x = min(max((point.x - (width - imageWidth) / 2) / imageWidth, 0), 1)
    let y = min(max((point.y - (height - imageHeight) / 2) / imageHeight, 0), 1)
    return guestPoint(forHostPoint: HostPoint(
      x: x * Double(hostSize.width), y: y * Double(hostSize.height)
    ))
  }
}

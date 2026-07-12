import Foundation

private enum GuestWindowProxyFrameLimits {
  static let maximumPixelCount = 32 * 1024 * 1024

  static func supports(width: Int, height: Int) -> Bool {
    width > 0 && height > 0 && width <= maximumPixelCount / height
  }
}

#if canImport(AppKit)
import AppKit
import SwiftUI
#endif

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

enum GuestWindowProxyPointerAction: String, Equatable {
  case move
  case press
  case release
  case click
}

enum GuestWindowProxyPointerButton: String, Equatable {
  case left
  case middle
  case right
}

enum GuestWindowProxyKeyAction: String, Equatable {
  case press
  case release
  case tap
}

enum GuestWindowProxyInputEvent: Equatable {
  case bounds(windowID: String, bounds: GuestToolsWindowBounds)
  case close(windowID: String)
  case pointer(
    windowID: String,
    point: GuestWindowProxyPlan.GuestPoint,
    action: GuestWindowProxyPointerAction,
    button: GuestWindowProxyPointerButton?
  )
  case key(windowID: String, key: String, action: GuestWindowProxyKeyAction)
}

typealias GuestWindowProxyInputSender = @MainActor (GuestWindowProxyInputEvent) -> Void

struct GuestWindowProxyRGBAFrame: Equatable {
  enum FrameError: LocalizedError, Equatable {
    case invalidDimensions(width: Int, height: Int)
    case byteCountMismatch(expected: Int, actual: Int)
    case byteCountOverflow(width: Int, height: Int)
    case pixelCountLimitExceeded(width: Int, height: Int)
    case imageCreationFailed

    var errorDescription: String? {
      switch self {
      case let .invalidDimensions(width, height):
        return "RGBA proxy frame dimensions must be positive; got \(width)x\(height)."
      case let .byteCountMismatch(expected, actual):
        return "RGBA proxy frame has \(actual) bytes; expected \(expected)."
      case let .byteCountOverflow(width, height):
        return "RGBA proxy frame dimensions \(width)x\(height) overflow byte-count calculation."
      case let .pixelCountLimitExceeded(width, height):
        return "RGBA proxy frame dimensions \(width)x\(height) exceed the 32-megapixel limit."
      case .imageCreationFailed:
        return "RGBA proxy frame could not be converted into a host image."
      }
    }
  }

  var width: Int
  var height: Int
  var data: Data

  init(width: Int, height: Int, data: Data) throws {
    let expectedByteCount = try Self.expectedByteCount(width: width, height: height)
    guard data.count == expectedByteCount else {
      throw FrameError.byteCountMismatch(expected: expectedByteCount, actual: data.count)
    }

    self.width = width
    self.height = height
    self.data = data
  }

  static func load(from url: URL, width: Int, height: Int) throws -> GuestWindowProxyRGBAFrame {
    let expectedByteCount = try expectedByteCount(width: width, height: height)
    let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
    let actualByteCount = (attributes[.size] as? NSNumber)?.intValue ?? -1
    guard actualByteCount == expectedByteCount else {
      throw FrameError.byteCountMismatch(expected: expectedByteCount, actual: actualByteCount)
    }
    return try GuestWindowProxyRGBAFrame(
      width: width,
      height: height,
      data: Data(contentsOf: url, options: .mappedIfSafe)
    )
  }

  static func expectedByteCount(width: Int, height: Int) throws -> Int {
    guard width > 0, height > 0 else {
      throw FrameError.invalidDimensions(width: width, height: height)
    }
    guard GuestWindowProxyFrameLimits.supports(width: width, height: height) else {
      throw FrameError.pixelCountLimitExceeded(width: width, height: height)
    }

    let pixelCount = width.multipliedReportingOverflow(by: height)
    guard !pixelCount.overflow else {
      throw FrameError.byteCountOverflow(width: width, height: height)
    }

    let byteCount = pixelCount.partialValue.multipliedReportingOverflow(by: 4)
    guard !byteCount.overflow else {
      throw FrameError.byteCountOverflow(width: width, height: height)
    }

    return byteCount.partialValue
  }

  var bytesPerRow: Int {
    width * 4
  }

  #if canImport(AppKit)
  func makeCGImage() throws -> CGImage {
    guard
      let provider = CGDataProvider(data: data as CFData),
      let image = CGImage(
        width: width,
        height: height,
        bitsPerComponent: 8,
        bitsPerPixel: 32,
        bytesPerRow: bytesPerRow,
        space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.last.rawValue),
        provider: provider,
        decode: nil,
        shouldInterpolate: false,
        intent: .defaultIntent
      )
    else {
      throw FrameError.imageCreationFailed
    }

    return image
  }

  func makeNSImage() throws -> NSImage {
    let image = try makeCGImage()
    return NSImage(
      cgImage: image,
      size: NSSize(width: width, height: height)
    )
  }
  #endif
}

struct GuestWindowProxyCropFrameArtifact: Equatable {
  enum ArtifactError: LocalizedError, Equatable {
    case missingCropFrame
    case unsupportedPixelFormat(String)
    case invalidDimensions(width: Int, height: Int)

    var errorDescription: String? {
      switch self {
      case .missingCropFrame:
        return "displayd summary does not contain a window_crop_frame artifact."
      case let .unsupportedPixelFormat(pixelFormat):
        return "Unsupported proxy crop pixel format '\(pixelFormat)'; expected rgba8."
      case let .invalidDimensions(width, height):
        return "Proxy crop artifact dimensions must be positive; got \(width)x\(height)."
      }
    }
  }

  var outputURL: URL
  var width: Int
  var height: Int
  var pixelFormat: String

  init(outputPath: String, width: Int, height: Int, pixelFormat: String = "rgba8") throws {
    guard pixelFormat == "rgba8" else {
      throw ArtifactError.unsupportedPixelFormat(pixelFormat)
    }
    guard width > 0, height > 0 else {
      throw ArtifactError.invalidDimensions(width: width, height: height)
    }

    self.outputURL = URL(fileURLWithPath: outputPath)
    self.width = width
    self.height = height
    self.pixelFormat = pixelFormat
  }

  static func decode(fromDisplaydSummary data: Data) throws -> GuestWindowProxyCropFrameArtifact {
    let summary = try JSONDecoder().decode(DisplaydSummary.self, from: data)
    guard let cropFrame = summary.windowCropFrame else {
      throw ArtifactError.missingCropFrame
    }

    return try GuestWindowProxyCropFrameArtifact(
      outputPath: cropFrame.outputPath,
      width: cropFrame.outputWidth,
      height: cropFrame.outputHeight,
      pixelFormat: cropFrame.pixelFormat
    )
  }

  static func decode(fromDisplaydSummaryAt url: URL) throws -> GuestWindowProxyCropFrameArtifact {
    try decode(fromDisplaydSummary: Data(contentsOf: url))
  }

  func loadFrame() throws -> GuestWindowProxyRGBAFrame {
    try GuestWindowProxyRGBAFrame.load(from: outputURL, width: width, height: height)
  }
}

private struct DisplaydSummary: Decodable {
  var windowCropFrame: DisplaydWindowCropFrame?

  private enum CodingKeys: String, CodingKey {
    case windowCropFrame = "window_crop_frame"
  }
}

private struct DisplaydWindowCropFrame: Decodable {
  var outputPath: String
  var pixelFormat: String
  var outputWidth: Int
  var outputHeight: Int

  private enum CodingKeys: String, CodingKey {
    case outputPath = "output_path"
    case pixelFormat = "pixel_format"
    case outputWidth = "output_width"
    case outputHeight = "output_height"
  }
}

enum GuestWindowProxyPlanner {
  enum PlanningError: LocalizedError, Equatable {
    case missingBounds
    case invalidBounds
    case boundsTooLarge

    var errorDescription: String? {
      switch self {
      case .missingBounds:
        return "Guest window bounds are required before opening a proxy shell."
      case .invalidBounds:
        return "Guest window bounds must have positive width and height."
      case .boundsTooLarge:
        return "Guest window bounds exceed the 32-megapixel proxy limit."
      }
    }
  }

  static let defaultMaximumHostSize = GuestWindowProxyPlan.HostSize(width: 1440, height: 900)
  private static let minimumScale = 0.25

  static func plan(
    vmName: String,
    window: GuestToolsWindowAction,
    maximumHostSize: GuestWindowProxyPlan.HostSize = defaultMaximumHostSize
  ) throws -> GuestWindowProxyPlan {
    guard let bounds = window.bounds else {
      throw PlanningError.missingBounds
    }
    guard bounds.width > 0 && bounds.height > 0 else {
      throw PlanningError.invalidBounds
    }
    let right = bounds.x.addingReportingOverflow(bounds.width)
    let bottom = bounds.y.addingReportingOverflow(bounds.height)
    guard !right.overflow, !bottom.overflow,
      GuestWindowProxyFrameLimits.supports(width: bounds.width, height: bounds.height)
    else {
      throw PlanningError.boundsTooLarge
    }

    let maxWidth = max(1, maximumHostSize.width)
    let maxHeight = max(1, maximumHostSize.height)
    let fitScale = min(
      1.0,
      Double(maxWidth) / Double(bounds.width),
      Double(maxHeight) / Double(bounds.height)
    )
    let scale = max(Self.minimumScale, fitScale)
    let hostSize = GuestWindowProxyPlan.HostSize(
      width: max(1, Int((Double(bounds.width) * scale).rounded())),
      height: max(1, Int((Double(bounds.height) * scale).rounded()))
    )

    return GuestWindowProxyPlan(
      vmName: vmName,
      windowID: window.id,
      title: window.title,
      guestBounds: bounds,
      hostSize: hostSize,
      scale: scale,
      pid: window.pid,
      desktop: window.desktop,
      cropFrameSummaryPath: window.cropFrameSummaryPath
    )
  }
}

#if canImport(AppKit)
@MainActor
enum GuestWindowProxyLauncher {
  private static var retainedWindows: [String: NSWindow] = [:]
  private static var retainedWindowSyncControllers: [String: GuestWindowProxyWindowSyncController]
    = [:]

  static func open(
    plan: GuestWindowProxyPlan,
    frame: GuestWindowProxyRGBAFrame? = nil,
    artifact: GuestWindowProxyCropFrameArtifact? = nil,
    inputSender: GuestWindowProxyInputSender? = nil
  ) {
    let key = retentionKey(for: plan)
    retainedWindowSyncControllers[key]?.closeWithoutGuestCommand()
    retainedWindows[key]?.close()
    let contentRect = NSRect(
      x: 0,
      y: 0,
      width: CGFloat(plan.hostSize.width),
      height: CGFloat(plan.hostSize.height)
    )
    let window = NSWindow(
      contentRect: contentRect,
      styleMask: [.titled, .closable, .miniaturizable, .resizable],
      backing: .buffered,
      defer: false
    )
    window.title = "\(plan.title) — \(plan.vmName)"
    window.contentMinSize = NSSize(width: 240, height: 160)
    let cropSummaryURL = cropFrameSummaryURL(from: plan.cropFrameSummaryPath)
    let resolvedArtifact =
      artifact
      ?? cropSummaryURL.flatMap {
        try? GuestWindowProxyCropFrameArtifact.decode(fromDisplaydSummaryAt: $0)
      }
    window.contentView = NSHostingView(
      rootView: GuestWindowProxyShellView(
        plan: plan,
        frame: frame,
        artifact: resolvedArtifact,
        cropFrameSummaryURL: cropSummaryURL,
        inputSender: inputSender
      )
    )
    window.center()
    let syncController = GuestWindowProxyWindowSyncController(
      plan: plan,
      baselineHostFrame: hostContentFrame(for: window),
      inputSender: inputSender
    ) { closedPlan in
      let key = retentionKey(for: closedPlan)
      retainedWindows[key] = nil
      retainedWindowSyncControllers[key] = nil
    }
    window.delegate = syncController
    window.makeKeyAndOrderFront(nil)
    retainedWindows[key] = window
    retainedWindowSyncControllers[key] = syncController
  }

  static func open(plan: GuestWindowProxyPlan, artifact: GuestWindowProxyCropFrameArtifact) {
    open(plan: plan, frame: nil, artifact: artifact)
  }

  static func closeWithoutGuestCommand(vmName: String, windowID: String) {
    let key = retentionKey(vmName: vmName, windowID: windowID)
    retainedWindowSyncControllers[key]?.closeWithoutGuestCommand()
    retainedWindows[key]?.close()
  }

  private static func cropFrameSummaryURL(from summaryPath: String?) -> URL? {
    guard let summaryPath = summaryPath?.trimmingCharacters(in: .whitespacesAndNewlines),
      !summaryPath.isEmpty
    else {
      return nil
    }

    return URL(fileURLWithPath: summaryPath)
  }

  private static func hostContentFrame(for window: NSWindow) -> GuestWindowProxyPlan.HostFrame {
    let contentRect = window.contentRect(forFrameRect: window.frame)
    return GuestWindowProxyPlan.HostFrame(
      x: Double(contentRect.origin.x),
      y: Double(contentRect.origin.y),
      width: Double(contentRect.width),
      height: Double(contentRect.height)
    )
  }

  private static func retentionKey(for plan: GuestWindowProxyPlan) -> String {
    retentionKey(vmName: plan.vmName, windowID: plan.windowID)
  }

  private static func retentionKey(vmName: String, windowID: String) -> String {
    "\(vmName)::\(windowID)"
  }
}

@MainActor
private final class GuestWindowProxyWindowSyncController: NSObject, NSWindowDelegate {
  private let plan: GuestWindowProxyPlan
  private let baselineHostFrame: GuestWindowProxyPlan.HostFrame
  private let inputSender: GuestWindowProxyInputSender?
  private let onClose: (GuestWindowProxyPlan) -> Void
  private let debounceNanoseconds: UInt64
  private var lastSentBounds: GuestToolsWindowBounds?
  private var pendingBounds: GuestToolsWindowBounds?
  private var pendingTask: Task<Void, Never>?
  private var sendsGuestCloseOnWindowClose = true

  init(
    plan: GuestWindowProxyPlan,
    baselineHostFrame: GuestWindowProxyPlan.HostFrame,
    inputSender: GuestWindowProxyInputSender?,
    debounceNanoseconds: UInt64 = 80_000_000,
    onClose: @escaping (GuestWindowProxyPlan) -> Void
  ) {
    self.plan = plan
    self.baselineHostFrame = baselineHostFrame
    self.inputSender = inputSender
    self.debounceNanoseconds = debounceNanoseconds
    self.onClose = onClose
    self.lastSentBounds = plan.guestBounds
  }

  func closeWithoutGuestCommand() {
    sendsGuestCloseOnWindowClose = false
  }

  func windowDidMove(_ notification: Notification) {
    scheduleBoundsSync(from: notification.object as? NSWindow)
  }

  func windowDidResize(_ notification: Notification) {
    scheduleBoundsSync(from: notification.object as? NSWindow)
  }

  func windowWillClose(_ notification: Notification) {
    pendingTask?.cancel()
    pendingTask = nil
    if sendsGuestCloseOnWindowClose {
      inputSender?(.close(windowID: plan.windowID))
    }
    onClose(plan)
  }

  private func scheduleBoundsSync(from window: NSWindow?) {
    guard let window, inputSender != nil else {
      return
    }

    let nextBounds = plan.guestBounds(
      forHostContentFrame: Self.hostContentFrame(for: window),
      relativeTo: baselineHostFrame
    )
    guard nextBounds != lastSentBounds else {
      return
    }

    pendingBounds = nextBounds
    guard pendingTask == nil else {
      return
    }

    pendingTask = Task { @MainActor [weak self] in
      guard let self else {
        return
      }
      try? await Task.sleep(nanoseconds: debounceNanoseconds)
      flushPendingBounds()
    }
  }

  private func flushPendingBounds() {
    pendingTask = nil
    guard let bounds = pendingBounds else {
      return
    }
    pendingBounds = nil
    guard bounds != lastSentBounds else {
      return
    }

    lastSentBounds = bounds
    inputSender?(.bounds(windowID: plan.windowID, bounds: bounds))
  }

  private static func hostContentFrame(for window: NSWindow) -> GuestWindowProxyPlan.HostFrame {
    let contentRect = window.contentRect(forFrameRect: window.frame)
    return GuestWindowProxyPlan.HostFrame(
      x: Double(contentRect.origin.x),
      y: Double(contentRect.origin.y),
      width: Double(contentRect.width),
      height: Double(contentRect.height)
    )
  }
}

@MainActor
final class GuestWindowProxyFrameModel: ObservableObject {
  @Published private(set) var frame: GuestWindowProxyRGBAFrame?
  @Published private(set) var lastError: String?

  private var artifact: GuestWindowProxyCropFrameArtifact?
  private let cropFrameSummaryURL: URL?
  private let refreshIntervalNanoseconds: UInt64

  init(
    frame: GuestWindowProxyRGBAFrame? = nil,
    artifact: GuestWindowProxyCropFrameArtifact? = nil,
    cropFrameSummaryURL: URL? = nil,
    refreshIntervalNanoseconds: UInt64 = 500_000_000
  ) {
    self.frame = frame
    self.artifact = artifact
    self.cropFrameSummaryURL = cropFrameSummaryURL
    self.refreshIntervalNanoseconds = refreshIntervalNanoseconds
  }

  var hasRefreshableArtifact: Bool {
    artifact != nil || cropFrameSummaryURL != nil
  }

  func refresh() {
    guard hasRefreshableArtifact else {
      return
    }

    do {
      let currentArtifact = try refreshedArtifact()
      artifact = currentArtifact
      frame = try currentArtifact.loadFrame()
      lastError = nil
    } catch {
      lastError = error.localizedDescription
    }
  }

  func refreshLoop() async {
    guard hasRefreshableArtifact else {
      return
    }

    while !Task.isCancelled {
      refresh()
      try? await Task.sleep(nanoseconds: refreshIntervalNanoseconds)
    }
  }

  private func refreshedArtifact() throws -> GuestWindowProxyCropFrameArtifact {
    if let cropFrameSummaryURL {
      return try GuestWindowProxyCropFrameArtifact.decode(
        fromDisplaydSummaryAt: cropFrameSummaryURL
      )
    }
    guard let artifact else {
      throw GuestWindowProxyCropFrameArtifact.ArtifactError.missingCropFrame
    }
    return artifact
  }
}

private struct GuestWindowProxyShellView: View {
  var plan: GuestWindowProxyPlan
  var inputSender: GuestWindowProxyInputSender?
  @StateObject private var frameModel: GuestWindowProxyFrameModel

  init(
    plan: GuestWindowProxyPlan,
    frame: GuestWindowProxyRGBAFrame? = nil,
    artifact: GuestWindowProxyCropFrameArtifact? = nil,
    cropFrameSummaryURL: URL? = nil,
    inputSender: GuestWindowProxyInputSender? = nil
  ) {
    self.plan = plan
    self.inputSender = inputSender
    _frameModel = StateObject(
      wrappedValue: GuestWindowProxyFrameModel(
        frame: frame,
        artifact: artifact,
        cropFrameSummaryURL: cropFrameSummaryURL
      )
    )
  }

  var body: some View {
    ZStack {
      Color(nsColor: .windowBackgroundColor)
      if let image = proxyImage {
        Image(nsImage: image)
          .resizable()
          .interpolation(.none)
          .aspectRatio(contentMode: .fit)
          .frame(maxWidth: .infinity, maxHeight: .infinity)
          .background(Color.black)
      }
      VStack(alignment: .leading, spacing: 8) {
        Text(plan.title)
          .font(.headline)
          .lineLimit(1)
        Text(plan.summary)
          .font(.caption.monospaced())
          .foregroundStyle(.secondary)
          .lineLimit(3)
        Text(statusText)
          .font(.caption.monospaced())
          .foregroundStyle(.secondary)
      }
      .padding(16)
      .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
      if let inputSender {
        GuestWindowProxyInputCaptureView(plan: plan, inputSender: inputSender)
          .frame(maxWidth: .infinity, maxHeight: .infinity)
      }
    }
    .task {
      await frameModel.refreshLoop()
    }
  }

  private func formattedScale(_ value: Double) -> String {
    String(format: "%.3f", value)
  }

  private var statusText: String {
    let scaleText = "Input scale \(formattedScale(plan.inputScaleX))x\(formattedScale(plan.inputScaleY))"
    if let error = frameModel.lastError {
      return "\(scaleText), frame error: \(error)"
    }

    guard let frame = frameModel.frame else {
      if frameModel.hasRefreshableArtifact {
        return "\(scaleText), waiting for crop artifact"
      }
      return "\(scaleText), waiting for crop frame"
    }

    return "\(scaleText), frame \(frame.width)x\(frame.height)"
  }

  private var proxyImage: NSImage? {
    try? frameModel.frame?.makeNSImage()
  }
}

private struct GuestWindowProxyInputCaptureView: NSViewRepresentable {
  var plan: GuestWindowProxyPlan
  var inputSender: GuestWindowProxyInputSender

  func makeNSView(context: Context) -> GuestWindowProxyInputCaptureNSView {
    GuestWindowProxyInputCaptureNSView(plan: plan, inputSender: inputSender)
  }

  func updateNSView(_ nsView: GuestWindowProxyInputCaptureNSView, context: Context) {
    nsView.plan = plan
    nsView.inputSender = inputSender
  }
}

private final class GuestWindowProxyInputCaptureNSView: NSView {
  var plan: GuestWindowProxyPlan
  var inputSender: GuestWindowProxyInputSender

  init(plan: GuestWindowProxyPlan, inputSender: @escaping GuestWindowProxyInputSender) {
    self.plan = plan
    self.inputSender = inputSender
    super.init(frame: .zero)
    wantsLayer = true
    layer?.backgroundColor = NSColor.clear.cgColor
  }

  required init?(coder: NSCoder) {
    return nil
  }

  override var acceptsFirstResponder: Bool {
    true
  }

  override func viewDidMoveToWindow() {
    super.viewDidMoveToWindow()
    window?.makeFirstResponder(self)
  }

  override func mouseDown(with event: NSEvent) {
    window?.makeFirstResponder(self)
    sendPointer(event, action: .press, button: .left)
  }

  override func mouseDragged(with event: NSEvent) {
    sendPointer(event, action: .move, button: nil)
  }

  override func mouseUp(with event: NSEvent) {
    sendPointer(event, action: .release, button: .left)
  }

  override func rightMouseDown(with event: NSEvent) {
    window?.makeFirstResponder(self)
    sendPointer(event, action: .press, button: .right)
  }

  override func rightMouseDragged(with event: NSEvent) {
    sendPointer(event, action: .move, button: nil)
  }

  override func rightMouseUp(with event: NSEvent) {
    sendPointer(event, action: .release, button: .right)
  }

  override func otherMouseDown(with event: NSEvent) {
    window?.makeFirstResponder(self)
    sendPointer(event, action: .press, button: .middle)
  }

  override func otherMouseDragged(with event: NSEvent) {
    sendPointer(event, action: .move, button: nil)
  }

  override func otherMouseUp(with event: NSEvent) {
    sendPointer(event, action: .release, button: .middle)
  }

  override func keyDown(with event: NSEvent) {
    guard let key = guestKeyName(for: event) else {
      return
    }
    inputSender(.key(windowID: plan.windowID, key: key, action: .press))
  }

  override func keyUp(with event: NSEvent) {
    guard let key = guestKeyName(for: event) else {
      return
    }
    inputSender(.key(windowID: plan.windowID, key: key, action: .release))
  }

  private func sendPointer(
    _ event: NSEvent,
    action: GuestWindowProxyPointerAction,
    button: GuestWindowProxyPointerButton?
  ) {
    let local = convert(event.locationInWindow, from: nil)
    let hostPoint = GuestWindowProxyPlan.HostPoint(
      x: Double(local.x),
      y: Double(bounds.height - local.y)
    )
    inputSender(
      .pointer(
        windowID: plan.windowID,
        point: plan.guestPoint(forHostPoint: hostPoint),
        action: action,
        button: button
      )
    )
  }

  private func guestKeyName(for event: NSEvent) -> String? {
    switch event.keyCode {
    case 36:
      return "Return"
    case 48:
      return "Tab"
    case 49:
      return "space"
    case 51:
      return "BackSpace"
    case 53:
      return "Escape"
    case 117:
      return "Delete"
    case 123:
      return "Left"
    case 124:
      return "Right"
    case 125:
      return "Down"
    case 126:
      return "Up"
    default:
      guard let characters = event.charactersIgnoringModifiers, !characters.isEmpty else {
        return nil
      }
      if characters.count == 1, let scalar = characters.unicodeScalars.first {
        if CharacterSet.alphanumerics.contains(scalar) {
          return String(scalar)
        }
        let character = String(scalar)
        if character == "-" {
          return "minus"
        }
        if character == "=" {
          return "equal"
        }
        if character == "." {
          return "period"
        }
        if character == "," {
          return "comma"
        }
        if character == "/" {
          return "slash"
        }
        if character == ";" {
          return "semicolon"
        }
        if character == "'" {
          return "apostrophe"
        }
        if character == "\\" {
          return "backslash"
        }
        if character == "[" {
          return "bracketleft"
        }
        if character == "]" {
          return "bracketright"
        }
      }
      return nil
    }
  }
}
#endif

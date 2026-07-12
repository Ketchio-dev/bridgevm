import XCTest

@testable import BridgeVMApp

final class GuestWindowProxyFrameTests: XCTestCase {
  func testFrameRejectsDimensionsBeyondMemoryLimit() {
    XCTAssertThrowsError(
      try GuestWindowProxyRGBAFrame.expectedByteCount(width: 8192, height: 8192)
    ) { error in
      XCTAssertEqual(
        error as? GuestWindowProxyRGBAFrame.FrameError,
        .pixelCountLimitExceeded(width: 8192, height: 8192)
      )
    }
  }

  func testCropFrameArtifactRejectsOversizedSummaryFileBeforeReading() throws {
    let url = FileManager.default.temporaryDirectory
      .appendingPathComponent("oversized-displayd-\(UUID().uuidString).json")
    defer { try? FileManager.default.removeItem(at: url) }
    try Data(repeating: 0x20, count: 1024 * 1024 + 1).write(to: url)

    XCTAssertThrowsError(
      try GuestWindowProxyCropFrameArtifact.decode(fromDisplaydSummaryAt: url)
    ) { error in
      XCTAssertEqual(
        error as? GuestWindowProxyCropFrameArtifact.ArtifactError,
        .summaryTooLarge(maximumByteCount: 1024 * 1024)
      )
    }
  }

  func testExpectedByteCountUsesRGBA8Dimensions() throws {
    XCTAssertEqual(
      try GuestWindowProxyRGBAFrame.expectedByteCount(width: 3, height: 2),
      24
    )
  }

  func testFrameAcceptsExactRGBA8ByteCount() throws {
    let frame = try GuestWindowProxyRGBAFrame(
      width: 2,
      height: 2,
      data: Data([
        255, 0, 0, 255,
        0, 255, 0, 255,
        0, 0, 255, 255,
        255, 255, 255, 255,
      ])
    )

    XCTAssertEqual(frame.width, 2)
    XCTAssertEqual(frame.height, 2)
    XCTAssertEqual(frame.bytesPerRow, 8)
    XCTAssertEqual(frame.data.count, 16)
  }

  func testFrameRejectsWrongByteCount() {
    XCTAssertThrowsError(
      try GuestWindowProxyRGBAFrame(width: 2, height: 2, data: Data([0, 1, 2]))
    ) { error in
      XCTAssertEqual(
        error as? GuestWindowProxyRGBAFrame.FrameError,
        .byteCountMismatch(expected: 16, actual: 3)
      )
    }
  }

  func testFrameRejectsInvalidDimensions() {
    XCTAssertThrowsError(
      try GuestWindowProxyRGBAFrame.expectedByteCount(width: 0, height: 2)
    ) { error in
      XCTAssertEqual(
        error as? GuestWindowProxyRGBAFrame.FrameError,
        .invalidDimensions(width: 0, height: 2)
      )
    }
  }

  func testFrameLoadsFromRawRGBAFile() throws {
    let directory = FileManager.default.temporaryDirectory
      .appendingPathComponent(UUID().uuidString, isDirectory: true)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: directory) }

    let file = directory.appendingPathComponent("window-crop.rgba")
    let data = Data([
      10, 20, 30, 255,
      40, 50, 60, 255,
    ])
    try data.write(to: file)

    let frame = try GuestWindowProxyRGBAFrame.load(from: file, width: 2, height: 1)

    XCTAssertEqual(frame.width, 2)
    XCTAssertEqual(frame.height, 1)
    XCTAssertEqual(frame.data, data)
  }

  func testCropFrameArtifactDecodesFromDisplaydSummaryAndLoadsFrame() throws {
    let directory = FileManager.default.temporaryDirectory
      .appendingPathComponent(UUID().uuidString, isDirectory: true)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: directory) }

    let cropFile = directory.appendingPathComponent("window-crop.rgba")
    let frameData = Data([
      1, 2, 3, 255,
      4, 5, 6, 255,
      7, 8, 9, 255,
      10, 11, 12, 255,
    ])
    try frameData.write(to: cropFile)

    let summary = """
      {
        "window_crop_frame": {
          "source_path": "/tmp/framebuffer.rgba",
          "output_path": "\(cropFile.path)",
          "pixel_format": "rgba8",
          "output_width": 2,
          "output_height": 2,
          "presentation": "proxy-window-crop-frame"
        }
      }
      """
      .data(using: .utf8)!

    let artifact = try GuestWindowProxyCropFrameArtifact.decode(fromDisplaydSummary: summary)
    let frame = try artifact.loadFrame()

    XCTAssertEqual(artifact.outputURL, cropFile)
    XCTAssertEqual(artifact.width, 2)
    XCTAssertEqual(artifact.height, 2)
    XCTAssertEqual(artifact.pixelFormat, "rgba8")
    XCTAssertEqual(frame.data, frameData)
  }

  func testCropFrameArtifactRequiresDisplaydCropFrame() {
    let summary = #"{"window_crop_frame": null}"#.data(using: .utf8)!

    XCTAssertThrowsError(
      try GuestWindowProxyCropFrameArtifact.decode(fromDisplaydSummary: summary)
    ) { error in
      XCTAssertEqual(
        error as? GuestWindowProxyCropFrameArtifact.ArtifactError,
        .missingCropFrame
      )
    }
  }

  func testCropFrameArtifactRejectsUnsupportedPixelFormat() {
    XCTAssertThrowsError(
      try GuestWindowProxyCropFrameArtifact(
        outputPath: "/tmp/window-crop.bgra",
        width: 1,
        height: 1,
        pixelFormat: "bgra8"
      )
    ) { error in
      XCTAssertEqual(
        error as? GuestWindowProxyCropFrameArtifact.ArtifactError,
        .unsupportedPixelFormat("bgra8")
      )
    }
  }

  @MainActor
  func testFrameModelRefreshesFromUpdatedCropArtifact() throws {
    let directory = FileManager.default.temporaryDirectory
      .appendingPathComponent(UUID().uuidString, isDirectory: true)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: directory) }

    let cropFile = directory.appendingPathComponent("window-crop.rgba")
    let firstFrame = Data([
      1, 2, 3, 255,
      4, 5, 6, 255,
    ])
    let secondFrame = Data([
      7, 8, 9, 255,
      10, 11, 12, 255,
    ])
    try firstFrame.write(to: cropFile)

    let artifact = try GuestWindowProxyCropFrameArtifact(
      outputPath: cropFile.path,
      width: 2,
      height: 1
    )
    let model = GuestWindowProxyFrameModel(artifact: artifact)

    model.refresh()
    XCTAssertEqual(model.frame?.data, firstFrame)
    XCTAssertNil(model.lastError)

    try secondFrame.write(to: cropFile)
    model.refresh()

    XCTAssertEqual(model.frame?.data, secondFrame)
    XCTAssertNil(model.lastError)
  }

  @MainActor
  func testFrameModelReloadsSummaryWhenCropArtifactDimensionsChange() throws {
    let directory = FileManager.default.temporaryDirectory
      .appendingPathComponent(UUID().uuidString, isDirectory: true)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: directory) }

    let cropFile = directory.appendingPathComponent("window-crop.rgba")
    let summaryFile = directory.appendingPathComponent("window-crop.json")
    let firstFrame = Data([
      1, 2, 3, 255,
      4, 5, 6, 255,
    ])
    let secondFrame = Data([
      7, 8, 9, 255,
      10, 11, 12, 255,
      13, 14, 15, 255,
    ])

    try firstFrame.write(to: cropFile)
    try writeCropSummary(
      to: summaryFile,
      cropFile: cropFile,
      width: 2,
      height: 1
    )

    let model = GuestWindowProxyFrameModel(cropFrameSummaryURL: summaryFile)
    model.refresh()

    XCTAssertEqual(model.frame?.width, 2)
    XCTAssertEqual(model.frame?.height, 1)
    XCTAssertEqual(model.frame?.data, firstFrame)
    XCTAssertNil(model.lastError)

    try secondFrame.write(to: cropFile)
    try writeCropSummary(
      to: summaryFile,
      cropFile: cropFile,
      width: 3,
      height: 1
    )
    model.refresh()

    XCTAssertEqual(model.frame?.width, 3)
    XCTAssertEqual(model.frame?.height, 1)
    XCTAssertEqual(model.frame?.data, secondFrame)
    XCTAssertNil(model.lastError)
  }

  @MainActor
  func testFrameModelRecordsRefreshErrorForMissingArtifactFile() throws {
    let artifact = try GuestWindowProxyCropFrameArtifact(
      outputPath: "/tmp/bridgevm-missing-window-crop-\(UUID().uuidString).rgba",
      width: 2,
      height: 1
    )
    let model = GuestWindowProxyFrameModel(artifact: artifact)

    model.refresh()

    XCTAssertNil(model.frame)
    XCTAssertNotNil(model.lastError)
  }

  #if canImport(AppKit)
  func testFrameCreatesHostImageWithMatchingDimensions() throws {
    let frame = try GuestWindowProxyRGBAFrame(
      width: 2,
      height: 1,
      data: Data([
        255, 0, 0, 255,
        0, 255, 0, 255,
      ])
    )

    let cgImage = try frame.makeCGImage()
    let nsImage = try frame.makeNSImage()

    XCTAssertEqual(cgImage.width, 2)
    XCTAssertEqual(cgImage.height, 1)
    XCTAssertEqual(nsImage.size.width, 2)
    XCTAssertEqual(nsImage.size.height, 1)
  }
  #endif

  private func writeCropSummary(
    to summaryFile: URL,
    cropFile: URL,
    width: Int,
    height: Int
  ) throws {
    let summary = """
      {
        "window_crop_frame": {
          "source_path": "/tmp/framebuffer.rgba",
          "output_path": "\(cropFile.path)",
          "pixel_format": "rgba8",
          "output_width": \(width),
          "output_height": \(height),
          "presentation": "proxy-window-crop-frame"
        }
      }
      """
      .data(using: .utf8)!
    try summary.write(to: summaryFile)
  }
}

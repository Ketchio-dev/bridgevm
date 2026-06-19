import XCTest

@testable import BridgeVMApp

final class GuestToolsCommandResultTests: XCTestCase {
  func testBackendSourceSummaryReportsRealDesktopApplicationBackendAndItemCount() {
    let result = GuestToolsCommandResult(
      requestID: "apps-1",
      capability: "applications",
      ok: true,
      errorCode: nil,
      message: "applications: org.gnome.Terminal:Terminal,firefox:Firefox",
      result: GuestToolsCommandPayload(
        value: .object([
          "applications": .array([
            .object([
              "id": .string("org.gnome.Terminal"),
              "name": .string("Terminal"),
              "source": .string("linux-desktop-file"),
            ]),
            .object([
              "id": .string("firefox"),
              "name": .string("Firefox"),
              "source": .string("linux-desktop-file"),
            ]),
          ])
        ])
      ),
      metadata: nil,
      completedAtUnix: 1_710_000_000
    )

    XCTAssertEqual(result.backendSourceSummary, "linux-desktop-file (applications: 2)")
  }

  func testBackendSourceSummaryReportsRealWindowBackendFromNestedMetadata() {
    let result = GuestToolsCommandResult(
      requestID: "windows-1",
      capability: "windows",
      ok: true,
      errorCode: nil,
      message: "windows: 0x01200007:Terminal",
      result: nil,
      metadata: GuestToolsCommandPayload(
        value: .object([
          "trace": .object([
            "windows": .array([
              .object([
                "id": .string("0x01200007"),
                "title": .string("Terminal"),
                "source": .string("wmctrl"),
              ])
            ])
          ])
        ])
      ),
      completedAtUnix: 1_710_000_000
    )

    XCTAssertEqual(result.backendSourceSummary, "wmctrl (windows: 1)")
  }

  func testBackendSourceSummaryIsAbsentForScaffoldPayloadWithoutSource() {
    let result = GuestToolsCommandResult(
      requestID: "apps-1",
      capability: "applications",
      ok: true,
      errorCode: nil,
      message: "applications: org.bridgevm.terminal:Terminal",
      result: GuestToolsCommandPayload(
        value: .object([
          "applications": .array([
            .object([
              "id": .string("org.bridgevm.terminal"),
              "name": .string("Terminal"),
            ])
          ])
        ])
      ),
      metadata: nil,
      completedAtUnix: 1_710_000_000
    )

    XCTAssertNil(result.backendSourceSummary)
  }

  func testApplicationActionsExtractLaunchableRowsAndDeduplicateByID() {
    let result = GuestToolsCommandResult(
      requestID: "apps-1",
      capability: "applications",
      ok: true,
      errorCode: nil,
      message: "applications: org.gnome.Terminal:Terminal,firefox:Firefox",
      result: GuestToolsCommandPayload(
        value: .object([
          "source": .string("linux-desktop-file"),
          "applications": .array([
            .object([
              "id": .string("org.gnome.Terminal"),
              "name": .string("Terminal"),
              "launched": .bool(true),
            ]),
            .object([
              "id": .string("firefox"),
              "name": .string("Firefox"),
              "source": .string("linux-desktop-file"),
            ]),
          ]),
        ])
      ),
      metadata: GuestToolsCommandPayload(
        value: .object([
          "trace": .object([
            "applications": .array([
              .object([
                "id": .string("org.gnome.Terminal"),
                "name": .string("Terminal duplicate"),
                "source": .string("metadata"),
              ]),
              .object([
                "id": .string("org.gnome.TextEditor"),
                "name": .string("Text Editor"),
                "source": .string("metadata"),
              ]),
            ])
          ])
        ])
      ),
      completedAtUnix: 1_710_000_000
    )

    XCTAssertEqual(
      result.applicationActions,
      [
        GuestToolsApplicationAction(
          id: "org.gnome.Terminal",
          name: "Terminal",
          source: "linux-desktop-file",
          launched: true
        ),
        GuestToolsApplicationAction(
          id: "firefox",
          name: "Firefox",
          source: "linux-desktop-file",
          launched: nil
        ),
        GuestToolsApplicationAction(
          id: "org.gnome.TextEditor",
          name: "Text Editor",
          source: "metadata",
          launched: nil
        ),
      ]
    )
  }

  func testApplicationActionsExtractSingularLaunchResult() {
    let result = GuestToolsCommandResult(
      requestID: "launch-1",
      capability: "applications",
      ok: true,
      errorCode: nil,
      message: "launched application org.gnome.Terminal",
      result: GuestToolsCommandPayload(
        value: .object([
          "application": .object([
            "id": .string("org.gnome.Terminal"),
            "name": .string("Terminal"),
            "source": .string("linux-desktop-file"),
            "launched": .bool(true),
          ])
        ])
      ),
      metadata: nil,
      completedAtUnix: 1_710_000_000
    )

    XCTAssertEqual(
      result.applicationActions,
      [
        GuestToolsApplicationAction(
          id: "org.gnome.Terminal",
          name: "Terminal",
          source: "linux-desktop-file",
          launched: true
        )
      ]
    )
  }

  func testWindowActionsExtractRowsAndDeduplicateByID() {
    let result = GuestToolsCommandResult(
      requestID: "windows-1",
      capability: "windows",
      ok: true,
      errorCode: nil,
      message: "windows: 0x01200007:Terminal,0x01200008:Files",
      result: GuestToolsCommandPayload(
        value: .object([
          "source": .string("wmctrl"),
          "windows": .array([
            .object([
              "id": .string("0x01200007"),
              "title": .string("Terminal"),
              "desktop": .number("0"),
              "pid": .number("4242"),
              "bounds": .object([
                "x": .number("30"),
                "y": .number("40"),
                "width": .number("800"),
                "height": .number("600"),
              ]),
              "focused": .bool(true),
            ]),
            .object([
              "id": .string("0x01200008"),
              "title": .string("Files"),
              "source": .string("wmctrl"),
            ]),
          ]),
        ])
      ),
      metadata: GuestToolsCommandPayload(
        value: .object([
          "nested": .object([
            "windows": .array([
              .object([
                "id": .string("0x01200007"),
                "title": .string("Terminal duplicate"),
                "source": .string("metadata"),
              ]),
              .object([
                "id": .string("0x01200009"),
                "title": .string("Settings"),
                "source": .string("metadata"),
              ]),
            ])
          ])
        ])
      ),
      completedAtUnix: 1_710_000_000
    )

    XCTAssertEqual(
      result.windowActions,
      [
        GuestToolsWindowAction(
          id: "0x01200007",
          title: "Terminal",
          source: "wmctrl",
          focused: true,
          desktop: 0,
          pid: 4242,
          bounds: GuestToolsWindowBounds(x: 30, y: 40, width: 800, height: 600)
        ),
        GuestToolsWindowAction(
          id: "0x01200008",
          title: "Files",
          source: "wmctrl",
          focused: nil
        ),
        GuestToolsWindowAction(
          id: "0x01200009",
          title: "Settings",
          source: "metadata",
          focused: nil
        ),
      ]
    )
  }

  func testWindowActionsExtractSingularFocusResult() {
    let result = GuestToolsCommandResult(
      requestID: "focus-1",
      capability: "windows",
      ok: true,
      errorCode: nil,
      message: "focused window 0x01200007",
      result: GuestToolsCommandPayload(
        value: .object([
          "window": .object([
            "id": .string("0x01200007"),
            "title": .string("Terminal"),
            "source": .string("wmctrl"),
            "desktop": .number("0"),
            "pid": .number("4242"),
            "bounds": .object([
              "x": .number("30"),
              "y": .number("40"),
              "width": .number("800"),
              "height": .number("600"),
            ]),
            "focused": .bool(true),
          ])
        ])
      ),
      metadata: nil,
      completedAtUnix: 1_710_000_000
    )

    XCTAssertEqual(
      result.windowActions,
      [
        GuestToolsWindowAction(
          id: "0x01200007",
          title: "Terminal",
          source: "wmctrl",
          focused: true,
          desktop: 0,
          pid: 4242,
          bounds: GuestToolsWindowBounds(x: 30, y: 40, width: 800, height: 600)
        )
      ]
    )
  }

  func testWindowActionsExtractCropFrameSummaryPath() {
    let result = GuestToolsCommandResult(
      requestID: "windows-1",
      capability: "windows",
      ok: true,
      errorCode: nil,
      message: "windows: 0x01200007:Terminal",
      result: GuestToolsCommandPayload(
        value: .object([
          "windows": .array([
            .object([
              "id": .string("0x01200007"),
              "title": .string("Terminal"),
              "source": .string("wmctrl"),
              "window_crop_frame_summary_path": .string("/tmp/displayd-window-crop.json"),
            ])
          ])
        ])
      ),
      metadata: nil,
      completedAtUnix: 1_710_000_000
    )

    XCTAssertEqual(
      result.windowActions,
      [
        GuestToolsWindowAction(
          id: "0x01200007",
          title: "Terminal",
          source: "wmctrl",
          focused: nil,
          cropFrameSummaryPath: "/tmp/displayd-window-crop.json"
        )
      ]
    )
  }
}

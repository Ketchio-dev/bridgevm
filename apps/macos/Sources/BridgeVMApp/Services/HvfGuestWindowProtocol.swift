import Foundation
import BridgeVMWindowProtocol

/// Parses the agent-console window protocol the Windows guest agent speaks.
///
/// `WINLIST` answers one `WIN <hwnd> <pid> <x> <y> <w> <h> <base64(title)>`
/// line per visible titled top-level window, terminated by `WINEND`. The
/// commands going the other way are single lines: `WINBOUNDS <hwnd> <x> <y>
/// <w> <h>`, `WINFOCUS <hwnd>`, `WINCLOSE <hwnd>`.
enum HvfGuestWindowProtocol {
  /// Decodes the lines between a WINLIST request and its WINEND terminator
  /// into the same window model the Linux guest path produces, so the
  /// dashboard's proxy-window machinery consumes both without caring which
  /// guest produced them.
  static func parseWindowList(_ lines: [String]) -> [GuestToolsWindowAction] {
    var windows: [GuestToolsWindowAction] = []
    for line in lines {
      if line == "WINEND" { break }
      guard let record = GuestWindowRecord(protocolLine: line) else { continue }
      windows.append(
        GuestToolsWindowAction(
          id: record.id,
          title: record.title,
          source: "bvagent",
          focused: nil,
          pid: Int(record.processID),
          bounds: GuestToolsWindowBounds(x: record.x, y: record.y, width: record.width, height: record.height)
        ))
    }
    return windows
  }

  static func boundsCommand(id: String, bounds: GuestToolsWindowBounds) -> String? {
    guard let id = HvfGuestWindowValidation.handle(id),
      HvfGuestWindowValidation.bounds(x: bounds.x, y: bounds.y, width: bounds.width, height: bounds.height)
    else { return nil }
    return "WINBOUNDS \(id) \(bounds.x) \(bounds.y) \(bounds.width) \(bounds.height)"
  }

  static func focusCommand(id: String) -> String? { HvfGuestWindowValidation.handle(id).map { "WINFOCUS \($0)" } }

  static func closeCommand(id: String) -> String? { HvfGuestWindowValidation.handle(id).map { "WINCLOSE \($0)" } }
}

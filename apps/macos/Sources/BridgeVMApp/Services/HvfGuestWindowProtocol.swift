import Foundation

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
      let parts = line.split(separator: " ", maxSplits: 7).map(String.init)
      guard parts.count == 8, parts[0] == "WIN" else { continue }
      guard let id = HvfGuestWindowValidation.handle(parts[1]),
        let pid = UInt32(parts[2]), pid > 0,
        let x = Int(parts[3]), let y = Int(parts[4]),
        let width = Int(parts[5]), let height = Int(parts[6]),
        HvfGuestWindowValidation.bounds(x: x, y: y, width: width, height: height)
      else { continue }
      guard let titleData = Data(base64Encoded: parts[7]),
        let title = String(data: titleData, encoding: .utf8),
        !title.isEmpty
      else { continue }
      windows.append(
        GuestToolsWindowAction(
          id: id,
          title: title,
          source: "bvagent",
          focused: nil,
          pid: Int(pid),
          bounds: GuestToolsWindowBounds(x: x, y: y, width: width, height: height)
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

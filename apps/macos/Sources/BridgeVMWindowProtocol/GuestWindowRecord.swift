import Foundation

public struct GuestWindowRecord: Equatable, Identifiable {
  public let id: String
  public let processID: UInt32
  public let x: Int
  public let y: Int
  public let width: Int
  public let height: Int
  public let title: String

  public init?(protocolLine: String) {
    let parts = protocolLine.split(separator: " ", maxSplits: 7).map(String.init)
    guard parts.count == 8, parts[0] == "WIN",
      let id = HvfGuestWindowValidation.handle(parts[1]),
      let pid = UInt32(parts[2]), pid > 0,
      let x = Int(parts[3]), let y = Int(parts[4]),
      let width = Int(parts[5]), let height = Int(parts[6]),
      HvfGuestWindowValidation.bounds(x: x, y: y, width: width, height: height),
      let data = Data(base64Encoded: parts[7]),
      let title = String(data: data, encoding: .utf8), !title.isEmpty
    else { return nil }
    self.id = id
    self.processID = pid
    self.x = x
    self.y = y
    self.width = width
    self.height = height
    self.title = title
  }
}

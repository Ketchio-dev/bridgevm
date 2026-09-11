import Foundation

public enum HvfGuestWindowValidation {
  public static func handle(_ value: String) -> String? {
    guard !value.isEmpty, value.utf8.allSatisfy({ (48...57).contains($0) }),
      let id = UInt64(value), id > 0 else { return nil }
    return String(id)
  }

  public static func bounds(x: Int, y: Int, width: Int, height: Int) -> Bool {
    Int32(exactly: x) != nil && Int32(exactly: y) != nil
      && width > 0 && height > 0
      && Int32(exactly: width) != nil && Int32(exactly: height) != nil
  }
}

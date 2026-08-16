import Foundation

// Split out of EmbeddedDisplayLauncher.swift, which is at its structural
// ceiling; the error type is self-contained.
extension EmbeddedDisplayLauncher {
  enum LaunchError: Error, LocalizedError, Equatable {
    case helperMissing(String)
    case invalidDisplaySize(DisplaySize)
    case spawnFailed(String)

    var errorDescription: String? {
      switch self {
      case .helperMissing(let name):
        return "The bundled helper '\(name)' is missing from the app bundle."
      case .invalidDisplaySize(let size):
        return "Display size \(size.width)x\(size.height) is too large (maximum 32 megapixels)."
      case .spawnFailed(let message):
        return "Could not open the display window: \(message)"
      }
    }
  }
}

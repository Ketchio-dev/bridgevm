import XCTest

@testable import BridgeVMControl

/// The legacy QEMU engine used to name /opt/homebrew directly, which is where
/// Homebrew installs on Apple Silicon and nowhere on Intel.
final class QemuCompatToolLookupTests: XCTestCase {
  func testIntelHomebrewPrefixIsAlsoSearched() {
    let candidates = QemuCompatBackend.toolCandidates("/bin/swtpm", override: nil)

    XCTAssertEqual(candidates, ["/opt/homebrew/bin/swtpm", "/usr/local/bin/swtpm"])
  }

  func testAnOverrideIsPreferredOverBothPrefixes() {
    let candidates = QemuCompatBackend.toolCandidates(
      "/bin/qemu-system-aarch64",
      override: "/custom/qemu"
    )

    XCTAssertEqual(candidates.first, "/custom/qemu")
    XCTAssertEqual(candidates.count, 3)
  }

  func testAnEmptyOverrideIsIgnoredRatherThanSearchedFor() {
    let candidates = QemuCompatBackend.toolCandidates("/bin/swtpm", override: "")

    XCTAssertEqual(candidates, ["/opt/homebrew/bin/swtpm", "/usr/local/bin/swtpm"])
  }

  func testATildeOverrideIsExpanded() {
    let candidates = QemuCompatBackend.toolCandidates("/bin/swtpm", override: "~/bin/swtpm")

    XCTAssertFalse(candidates[0].hasPrefix("~"))
    XCTAssertTrue(candidates[0].hasSuffix("/bin/swtpm"))
  }

  /// Every tool the engine needs has to be searched the same way; a fixed path
  /// left in any one of them still breaks the engine on Intel.
  func testAllThreeToolSuffixesResolveUnderBothPrefixes() {
    for suffix in [
      "/bin/qemu-system-aarch64", "/share/qemu/edk2-aarch64-code.fd", "/bin/swtpm",
    ] {
      let candidates = QemuCompatBackend.toolCandidates(suffix, override: nil)
      XCTAssertEqual(candidates, ["/opt/homebrew" + suffix, "/usr/local" + suffix], suffix)
    }
  }
}

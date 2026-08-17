import XCTest

@testable import BridgeVMControl

/// The legacy QEMU engine used to name /opt/homebrew directly, which is where
/// Homebrew installs on Apple Silicon and nowhere on Intel.
final class QemuCompatToolLookupTests: XCTestCase {
  func testIntelHomebrewPrefixIsAlsoSearched() {
    let candidates = QemuCompatBackend.toolCandidates("/bin/qemu-system-aarch64", override: nil)

    XCTAssertEqual(candidates, ["/opt/homebrew/bin/qemu-system-aarch64", "/usr/local/bin/qemu-system-aarch64"])
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
    let candidates = QemuCompatBackend.toolCandidates("/bin/qemu-system-aarch64", override: "")

    XCTAssertEqual(candidates, ["/opt/homebrew/bin/qemu-system-aarch64", "/usr/local/bin/qemu-system-aarch64"])
  }

  func testATildeOverrideIsExpanded() {
    let candidates = QemuCompatBackend.toolCandidates("/bin/qemu-system-aarch64", override: "~/bin/qemu")

    XCTAssertFalse(candidates[0].hasPrefix("~"))
    XCTAssertTrue(candidates[0].hasSuffix("/bin/qemu"))
  }

  /// qemu and its firmware are searched; swtpm deliberately is not, because it
  /// holds the vTPM's sealed state and goes through the bundle-first path in
  /// VTPMStateSecurity instead.
  func testSearchedToolSuffixesResolveUnderBothPrefixes() {
    for suffix in ["/bin/qemu-system-aarch64", "/share/qemu/edk2-aarch64-code.fd"] {
      let candidates = QemuCompatBackend.toolCandidates(suffix, override: nil)
      XCTAssertEqual(candidates, ["/opt/homebrew" + suffix, "/usr/local" + suffix], suffix)
    }
  }
}

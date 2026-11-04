import Foundation
import XCTest
@testable import BridgeVMControl

/// Dropped names are repaired into what the guest agent accepts (bvagent.ps1 `Test-DropName`).
final class HvfFileDropNamesTests: XCTestCase {
    func testReservedDeviceNamesGetAPrefixWithOrWithoutAnExtension() {
        for (name, expected) in [("CON", "_CON"), ("con.txt", "_con.txt"), ("Lpt1.tar.gz", "_Lpt1.tar.gz"),
                                 ("COM¹", "_COM¹"), ("nul .txt", "_nul .txt"), ("aux", "_aux"),
                                 ("CONSOLE.txt", "CONSOLE.txt"), ("COM10", "COM10"), ("x.con", "x.con")] {
            XCTAssertEqual(HvfFileDropNames.windowsName(name), expected, name)
        }
    }

    func testForbiddenCharactersAndTrailingDotsAndSpacesAreRepaired() {
        XCTAssertEqual(HvfFileDropNames.windowsName("a:b<c>d\"e|f?g*h\\i"), "a_b_c_d_e_f_g_h_i")
        XCTAssertEqual(HvfFileDropNames.windowsName("tab\there\u{1}"), "tab_here_")
        XCTAssertEqual(HvfFileDropNames.windowsName("report. . "), "report")
        XCTAssertEqual(HvfFileDropNames.windowsName("..."), "_")
        XCTAssertEqual(HvfFileDropNames.windowsName("  leading stays.txt"), "  leading stays.txt")
    }

    func testDecomposedHangulFromFinderBecomesPrecomposed() {
        let precomposed = "한글 문서.txt".precomposedStringWithCanonicalMapping
        let decomposed = precomposed.decomposedStringWithCanonicalMapping
        XCTAssertNotEqual(Array(decomposed.unicodeScalars), Array(precomposed.unicodeScalars))
        XCTAssertEqual(Array(HvfFileDropNames.windowsName(decomposed).unicodeScalars), Array(precomposed.unicodeScalars))
    }

    func testCollisionsIgnoreCaseKeepTheExtensionAndNumberFoldersWhole() {
        var taken = Set<String>()
        let names = [("Photo.JPG", false), ("photo.jpg", false), ("PHOTO.jpg", false),
                     ("My.Folder", true), ("my.folder", true), (".env", false), (".ENV", false)]
        let unique = names.map { HvfFileDropNames.unique($0.0, isDirectory: $0.1, taken: &taken) }
        XCTAssertEqual(unique, ["Photo.JPG", "photo (2).jpg", "PHOTO (3).jpg",
                                "My.Folder", "my.folder (2)", ".env", ".ENV (2)"])
    }
}

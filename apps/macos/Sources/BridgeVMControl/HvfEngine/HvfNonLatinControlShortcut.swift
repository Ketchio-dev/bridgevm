import SwiftUI

extension HvfHostKeyCommand {
    /// Keep logical commands first; non-Latin Ctrl letters may instead identify a supported physical position.
    static func resolve(characters: String, modifiers: EventModifiers, keyCode: UInt16) -> Self {
        let logical = resolve(characters: characters, modifiers: modifiers)
        guard logical == .ignored, modifiers == .control, characters.unicodeScalars.count == 1,
              let scalar = characters.unicodeScalars.first, !scalar.isASCII,
              nonLatinControlLetters.contains(scalar.properties.generalCategory), let key = nonLatinControlPositions[keyCode] else { return logical }
        return .key("ctrl+" + key)
    }

    private static let nonLatinControlLetters: [Unicode.GeneralCategory] = [.uppercaseLetter, .lowercaseLetter, .titlecaseLetter, .modifierLetter, .otherLetter]
    // macOS ANSI virtual key positions; only chords already supported by the guest route.
    private static let nonLatinControlPositions: [UInt16: String] = [
        0x00: "a", 0x08: "c", 0x09: "v", 0x07: "x", 0x10: "y", 0x06: "z"
    ]
}
